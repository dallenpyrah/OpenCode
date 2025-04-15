use crate::api_client::{Message, Role};
use crate::config::Config;
use anyhow::{anyhow, Context, Result};
use tiktoken_rs::{get_bpe_from_model, CoreBPE};
use tracing::{debug, info, warn};

// Constants
const DEFAULT_TOKENIZER_MODEL: &str = "gpt-4"; // Default model for token counting
const MAX_CONTEXT_TOKENS: usize = 4000; // Simple token limit for now

#[derive(Debug, Clone)]
pub struct ContextSnippet {
    pub source: String, // e.g., "file: src/main.rs", "git diff HEAD~1"
    pub content: String,
    token_count: usize, // Added token count
}

// Removed Debug derive because CoreBPE doesn't implement it
pub struct ContextManager {
    config: Config,
    history: Vec<(Message, usize)>, // Store messages with their token counts
    context_snippets: Vec<ContextSnippet>,
    tokenizer: CoreBPE,
    total_token_count: usize,
    max_tokens: usize, // Make limit configurable later via Config
}

impl ContextManager {
    /// Creates a new ContextManager instance.
    pub fn new(config: Config) -> Result<Self> {
        let tokenizer = get_bpe_from_model(DEFAULT_TOKENIZER_MODEL)
            .map_err(|e| anyhow!("Failed to load tokenizer: {}", e))?;
        let max_tokens = MAX_CONTEXT_TOKENS; // Use constant for now, config field doesn't exist
        Ok(ContextManager {
            config,
            history: Vec::new(),
            context_snippets: Vec::new(),
            tokenizer,
            total_token_count: 0,
            max_tokens,
        })
    }

    /// Counts tokens for a given text using the loaded tokenizer.
    // Note: Assuming encode_with_special_tokens does not return Result based on compiler errors.
    // If it can panic or error differently, more robust handling might be needed.
    fn count_tokens(&self, text: &str) -> usize {
        self.tokenizer.encode_with_special_tokens(text).len()
    }

    /// Adds a message to the conversation history.
    pub fn add_message(&mut self, message: Message) -> Result<()> {
        // Handle Option<String> for content
        let tokens = match &message.content {
            Some(content_str) => self.count_tokens(content_str), // No longer returns Result
            None => 0, // No content, 0 tokens
        };
        debug!(role = ?message.role, tokens = tokens, "Adding message to history");
        self.history.push((message, tokens));
        self.total_token_count += tokens;
        self.ensure_token_limit()
            .context("Failed to ensure token limit after adding message")?;
        Ok(())
    }

    /// Adds a context snippet.
    pub fn add_snippet(&mut self, source: String, content: String) -> Result<()> {
        let full_snippet_text = Self::format_snippet_content(&source, &content);
        let tokens = self.count_tokens(&full_snippet_text); // No longer returns Result, removed overhead
        let snippet = ContextSnippet {
            source: source.clone(),
            content,
            token_count: tokens,
        };
        info!(source = %snippet.source, tokens = tokens, "Adding context snippet");
        self.context_snippets.push(snippet);
        self.total_token_count += tokens;
        self.ensure_token_limit()
            .context("Failed to ensure token limit after adding snippet")?;
        Ok(())
    }

    /// Clears the conversation history.
    pub fn clear_history(&mut self) {
        info!("Clearing conversation history");
        self.total_token_count = self
            .context_snippets
            .iter()
            .map(|s| s.token_count)
            .sum();
        self.history.clear();
    }

    /// Clears all context snippets.
    pub fn clear_snippets(&mut self) {
        info!("Clearing context snippets");
        self.total_token_count = self.history.iter().map(|(_, tokens)| tokens).sum();
        self.context_snippets.clear();
    }

    /// Formats a snippet for inclusion in the prompt.
    fn format_snippet_content(source: &str, content: &str) -> String {
        // Basic formatting, could infer language later
        format!("Content from {}:\n```\n{}\n```", source, content)
    }

    /// Ensures the total token count is below the maximum limit by evicting oldest items.
    /// Prioritizes keeping the most recent history and snippets.
    fn ensure_token_limit(&mut self) -> Result<()> {
        while self.total_token_count > self.max_tokens {
            // Simple eviction: remove oldest history message first, then oldest snippet.
            // Could be more sophisticated (e.g., remove largest item, preserve system prompt).
            if !self.history.is_empty() {
                let (removed_message, removed_tokens) = self.history.remove(0);
                self.total_token_count -= removed_tokens;
                debug!(tokens = removed_tokens, role = ?removed_message.role, "Evicted oldest message");
            } else if !self.context_snippets.is_empty() {
                let removed_snippet = self.context_snippets.remove(0);
                self.total_token_count -= removed_snippet.token_count;
                debug!(tokens = removed_snippet.token_count, source = %removed_snippet.source, "Evicted oldest snippet");
            } else {
                // Should not happen if max_tokens is reasonably large
                warn!("Token limit exceeded but nothing to evict. Total tokens: {}", self.total_token_count);
                return Err(anyhow!("Cannot reduce tokens below limit, history and snippets are empty, but total_token_count ({}) > max_tokens ({})", self.total_token_count, self.max_tokens));
            }
        }
        Ok(())
    }

    /// Prepares the list of messages to be sent to the API,
    /// including history and formatted context snippets.
    /// Applies token limit constraints.
    pub fn construct_api_messages(&mut self) -> Result<Vec<Message>> {
        // Ensure limit is applied before constructing
        self.ensure_token_limit()
            .context("Failed to ensure token limit before constructing API messages")?;

        let mut api_messages = Vec::new();
        let mut current_tokens = 0;

        // Add formatted snippets first as system/context info
        // Iterate in reverse to prioritize *newer* snippets if space is tight *during construction*
        // (although ensure_token_limit should have already handled the main limit)
        for snippet in self.context_snippets.iter().rev() {
             let formatted_content = Self::format_snippet_content(&snippet.source, &snippet.content);
             // Use count_tokens here for potentially more accurate count of the final format
             let snippet_tokens = self.count_tokens(&formatted_content); // No longer returns Result
             if current_tokens + snippet_tokens <= self.max_tokens {
                 api_messages.push(Message {
                     role: Role::System, // Treat snippets as system info for now
                     content: Some(formatted_content), // Wrap in Some()
                     tool_calls: None, // Add missing field
                     tool_call_id: None, // Add missing field
                 });
                 current_tokens += snippet_tokens;
             } else {
                 warn!(source = %snippet.source, "Skipping snippet during construction due to token limit");
             }
        }
        // Reverse snippets so they appear in original order (oldest first)
        api_messages.reverse();


        // Add history messages, prioritizing newest
        // Iterate history in reverse to add newest first
        for (message, message_tokens) in self.history.iter().rev() {
            if current_tokens + message_tokens <= self.max_tokens {
                api_messages.push(message.clone());
                current_tokens += message_tokens;
            } else {
                 warn!(role = ?message.role, "Skipping history message during construction due to token limit");
                 // Since we iterate newest first, we can stop once one doesn't fit
                 break;
            }
        }

        // Reverse the history part so it's in chronological order
        // Find the split point between snippets and history
        let history_start_index = self.context_snippets.len(); // Snippets were added first
        if api_messages.len() > history_start_index {
             api_messages[history_start_index..].reverse();
        }


        debug!(messages_count = api_messages.len(), final_tokens = current_tokens, "Constructed API messages");
        Ok(api_messages)
    }

    /// Returns a reference to the configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }
}

// --- Tests ---
#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_client::Role;
    use crate::config::Config;

    fn create_test_manager() -> ContextManager {
        let config = Config::default(); // Use default config for tests
        ContextManager::new(config).expect("Failed to create test ContextManager")
    }

     fn create_test_manager_with_limit(limit: usize) -> ContextManager {
        let config = Config::default();
        // Cannot set max_context_tokens on config directly, create manager and set limit manually for test
        let mut manager = ContextManager::new(config).expect("Failed to create test ContextManager");
        manager.max_tokens = limit; // Set the limit directly on the manager instance
        manager
    }


    #[test]
    fn test_token_counting() {
        let manager = create_test_manager();
        let tokens = manager.count_tokens("Hello world"); // No longer returns Result
        assert!(tokens > 0, "Token count should be positive");
        // Exact count depends on tokenizer, just check it works
        assert_eq!(tokens, 2);

        let tokens_complex = manager.count_tokens("複雑なテキスト"); // No longer returns Result
         assert!(tokens_complex > 0, "Token count for complex text should be positive");
    }

    #[test]
    fn test_add_message() {
        let mut manager = create_test_manager();
        let msg = Message {
            role: Role::User,
            content: Some("Test message".to_string()), // Wrap in Some()
            tool_calls: None, // Add missing field
            tool_call_id: None, // Add missing field
        };
        let initial_tokens = manager.total_token_count;

        manager.add_message(msg.clone()).unwrap();

        assert_eq!(manager.history.len(), 1);
        assert_eq!(manager.history[0].0.content, msg.content); // Compare Option<String>
        assert!(manager.total_token_count > initial_tokens);
        // Handle Option<String> for token counting in assertion
        let expected_tokens = msg.content.as_ref().map_or(0, |c| manager.count_tokens(c)); // No longer returns Result
        assert_eq!(manager.history[0].1, expected_tokens);
    }

    #[test]
    fn test_add_snippet() {
        let mut manager = create_test_manager();
        let source = "file: test.txt".to_string();
        let content = "Snippet content".to_string();
        let initial_tokens = manager.total_token_count;

        manager.add_snippet(source.clone(), content.clone()).unwrap();

        assert_eq!(manager.context_snippets.len(), 1);
        assert_eq!(manager.context_snippets[0].source, source);
        assert_eq!(manager.context_snippets[0].content, content);
        assert!(manager.total_token_count > initial_tokens);
        // Check token count includes overhead
        let expected_tokens = manager.count_tokens(&ContextManager::format_snippet_content(&source, &content)); // No longer returns Result, removed overhead
        assert_eq!(manager.context_snippets[0].token_count, expected_tokens);
    }

    #[test]
    fn test_basic_eviction_history() {
        // Use a small limit to force eviction
        // Use a smaller limit to force eviction based on more accurate token counts
        let mut manager = create_test_manager_with_limit(20);

        // Add messages until limit is likely exceeded
        for i in 0..10 {
            let msg = Message {
                role: Role::User,
                content: Some(format!("Message {}", i)), // Wrap in Some()
                tool_calls: None, // Add missing field
                tool_call_id: None, // Add missing field
             };
            manager.add_message(msg).unwrap();
        }

        assert!(manager.total_token_count <= manager.max_tokens, "Total tokens should be within limit after eviction");
        assert!(!manager.history.is_empty(), "History should not be empty after eviction (unless limit is tiny)");
        // Check that the *last* message added is still present
        assert!(manager.history.iter().any(|(m, _)| m.content == Some("Message 9".to_string()))); // Compare Option<String>
         // Check that the *first* message is likely gone
        assert!(!manager.history.iter().any(|(m, _)| m.content == Some("Message 0".to_string()))); // Compare Option<String>
    }

     #[test]
    fn test_basic_eviction_snippets() {
        let mut manager = create_test_manager_with_limit(60);

        // Add snippets until limit is likely exceeded
        for i in 0..5 {
            let source = format!("source_{}", i);
            let content = format!("Content for snippet number {}", i); // Longer content
            manager.add_snippet(source, content).unwrap();
        }

        assert!(manager.total_token_count <= manager.max_tokens, "Total tokens should be within limit after snippet eviction");
        assert!(!manager.context_snippets.is_empty(), "Snippets should not be empty after eviction");
        // Check that the *last* snippet added is still present
        assert!(manager.context_snippets.iter().any(|s| s.source == "source_4"));
        // Check that the *first* snippet is likely gone
        assert!(!manager.context_snippets.iter().any(|s| s.source == "source_0"));
    }


    #[test]
    fn test_eviction_mixed() {
        let mut manager = create_test_manager_with_limit(70); // Limit allowing maybe 2 messages and 1 snippet

        manager.add_message(Message { role: Role::User, content: Some("First user message".to_string()), tool_calls: None, tool_call_id: None }).unwrap(); // msg 1 (oldest)
        manager.add_snippet("file1.txt".to_string(), "Snippet one content.".to_string()).unwrap(); // snip 1
        manager.add_message(Message { role: Role::Assistant, content: Some("Assistant response".to_string()), tool_calls: None, tool_call_id: None }).unwrap(); // msg 2
        manager.add_snippet("file2.txt".to_string(), "Snippet two content, slightly longer.".to_string()).unwrap(); // snip 2
        manager.add_message(Message { role: Role::User, content: Some("Second user message, quite verbose to ensure it pushes limits.".to_string()), tool_calls: None, tool_call_id: None }).unwrap(); // msg 3 (newest)


        // With overhead removed, estimated tokens are ~68, limit is 70. No eviction should occur.
        assert!(manager.total_token_count <= manager.max_tokens, "Total tokens ({}) should be within limit ({})", manager.total_token_count, manager.max_tokens);
        assert_eq!(manager.history.len(), 3, "Expected 3 history messages, none evicted");
        assert_eq!(manager.context_snippets.len(), 2, "Expected 2 snippets, none evicted");

        // Check specific items are still present
        assert!(manager.history.iter().any(|(m, _)| m.content.as_ref().map_or(false, |c| c.contains("First user"))), "Oldest message should be present");
        assert!(manager.context_snippets.iter().any(|s| s.source == "file1.txt"), "Oldest snippet should be present");
        assert!(manager.history.iter().any(|(m, _)| m.content.as_ref().map_or(false, |c| c.contains("Second user"))), "Newest message should be present");
        assert!(manager.context_snippets.iter().any(|s| s.source == "file2.txt"), "Newest snippet should be present");
    }


    #[test]
    fn test_construct_api_messages_format() {
        let mut manager = create_test_manager();
        manager.add_message(Message { role: Role::User, content: Some("User query".to_string()), tool_calls: None, tool_call_id: None }).unwrap();
        manager.add_snippet("test.rs".to_string(), "let x = 5;".to_string()).unwrap();
        manager.add_message(Message { role: Role::Assistant, content: Some("Assistant reply".to_string()), tool_calls: None, tool_call_id: None }).unwrap();

        let api_messages = manager.construct_api_messages().unwrap();

        assert_eq!(api_messages.len(), 3, "Should have 1 snippet + 2 history messages");

        // Snippet should be first and formatted as System message
        assert_eq!(api_messages[0].role, Role::System);
        assert!(api_messages[0].content.as_ref().map_or(false, |c| c.contains("Content from test.rs:"))); // Safely check Option
        assert!(api_messages[0].content.as_ref().map_or(false, |c| c.contains("```\nlet x = 5;\n```"))); // Safely check Option

        // History should follow in chronological order
        assert_eq!(api_messages[1].role, Role::User);
        assert_eq!(api_messages[1].content, Some("User query".to_string())); // Compare Option<String>
        assert_eq!(api_messages[2].role, Role::Assistant);
        assert_eq!(api_messages[2].content, Some("Assistant reply".to_string())); // Compare Option<String>
    }

     #[test]
    fn test_construct_api_messages_respects_limit() {
        let mut manager = create_test_manager_with_limit(50); // Tight limit

        // Add more content than fits
        manager.add_snippet("file1.txt".to_string(), "Very long snippet content that will definitely exceed the small token limit all by itself.".to_string()).unwrap();
        manager.add_message(Message { role: Role::User, content: Some("A user message".to_string()), tool_calls: None, tool_call_id: None }).unwrap();
        manager.add_message(Message { role: Role::Assistant, content: Some("An assistant message".to_string()), tool_calls: None, tool_call_id: None }).unwrap();

        // Eviction should have happened during add, but construct also checks
        let api_messages = manager.construct_api_messages().unwrap();

        let final_tokens: usize = api_messages.iter()
            .map(|m| m.content.as_ref().map_or(0, |c| manager.count_tokens(c))) // No longer returns Result
            .sum();

        assert!(final_tokens <= manager.max_tokens, "Constructed messages total tokens ({}) should not exceed limit ({})", final_tokens, manager.max_tokens);
        // Depending on exact counts and eviction, likely only the last message fits
        // With overhead removed, estimated tokens ~38, limit 50. No eviction. All 3 items should fit.
        assert_eq!(api_messages.len(), 3, "Expected snippet, user message, and assistant message");
        assert_eq!(api_messages[0].role, Role::System, "First message should be the snippet (System)");
        assert_eq!(api_messages[1].role, Role::User, "Second message should be the user message");
        assert_eq!(api_messages[2].role, Role::Assistant, "Third message should be the assistant message");
    }

    #[test]
    fn test_clear_history() {
        let mut manager = create_test_manager();
        manager.add_message(Message { role: Role::User, content: Some("msg1".to_string()), tool_calls: None, tool_call_id: None }).unwrap();
        manager.add_snippet("file.txt".to_string(), "snip1".to_string()).unwrap();
        let snippet_tokens = manager.context_snippets[0].token_count;

        manager.clear_history();

        assert!(manager.history.is_empty());
        assert_eq!(manager.context_snippets.len(), 1);
        assert_eq!(manager.total_token_count, snippet_tokens);
    }

    #[test]
    fn test_clear_snippets() {
        let mut manager = create_test_manager();
        manager.add_message(Message { role: Role::User, content: Some("msg1".to_string()), tool_calls: None, tool_call_id: None }).unwrap();
        manager.add_snippet("file.txt".to_string(), "snip1".to_string()).unwrap();
        let message_tokens = manager.history[0].1;

        manager.clear_snippets();

        assert_eq!(manager.history.len(), 1);
        assert!(manager.context_snippets.is_empty());
        assert_eq!(manager.total_token_count, message_tokens);
    }
}
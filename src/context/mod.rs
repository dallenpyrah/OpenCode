use crate::api::models::{Message, Role};
use crate::config::Config;
use anyhow::{anyhow, Context, Result};
use tiktoken_rs::{get_bpe_from_model, CoreBPE};
use tracing::{debug, info, warn};


const DEFAULT_TOKENIZER_MODEL: &str = "gpt-4"; 
const MAX_CONTEXT_TOKENS: usize = 4000; 

#[derive(Debug, Clone)]
pub struct ContextSnippet {
    pub source: String, 
    pub content: String,
    token_count: usize, 
}


pub struct ContextManager {
    #[allow(dead_code)]
    config: Config,
    history: Vec<(Message, usize)>, 
    context_snippets: Vec<ContextSnippet>,
    tokenizer: CoreBPE,
    total_token_count: usize,
    max_tokens: usize, 
}

impl ContextManager {
    
    pub fn new(config: Config) -> Result<Self> {
        let tokenizer = get_bpe_from_model(DEFAULT_TOKENIZER_MODEL)
            .map_err(|e| anyhow!("Failed to load tokenizer: {}", e))?;
        let max_tokens = MAX_CONTEXT_TOKENS; 
        Ok(ContextManager {
            config,
            history: Vec::new(),
            context_snippets: Vec::new(),
            tokenizer,
            total_token_count: 0,
            max_tokens,
        })
    }

    
    
    
    fn count_tokens(&self, text: &str) -> usize {
        self.tokenizer.encode_with_special_tokens(text).len()
    }

    
    pub fn add_message(&mut self, message: Message) -> Result<()> {
        
        let tokens = match &message.content {
            Some(content_str) => self.count_tokens(content_str), 
            None => 0, 
        };
        debug!(role = ?message.role, tokens = tokens, "Adding message to history");
        self.history.push((message, tokens));
        self.total_token_count += tokens;
        self.ensure_token_limit()
            .context("Failed to ensure token limit after adding message")?;
        Ok(())
    }

    
    pub fn clear_history(&mut self) {
        info!("Clearing conversation history");
        self.total_token_count = self
            .context_snippets
            .iter()
            .map(|s| s.token_count)
            .sum();
        self.history.clear();
    }

    
    pub fn clear_snippets(&mut self) {
        info!("Clearing context snippets");
        self.total_token_count = self.history.iter().map(|(_, tokens)| tokens).sum();
        self.context_snippets.clear();
    }

    
    fn format_snippet_content(source: &str, content: &str) -> String {
        
        format!("Content from {}:\n```\n{}\n```", source, content)
    }

    
    
    fn ensure_token_limit(&mut self) -> Result<()> {
        while self.total_token_count > self.max_tokens {
            
            
            if !self.history.is_empty() {
                let (removed_message, removed_tokens) = self.history.remove(0);
                self.total_token_count -= removed_tokens;
                debug!(tokens = removed_tokens, role = ?removed_message.role, "Evicted oldest message");
            } else if !self.context_snippets.is_empty() {
                let removed_snippet = self.context_snippets.remove(0);
                self.total_token_count -= removed_snippet.token_count;
                debug!(tokens = removed_snippet.token_count, source = %removed_snippet.source, "Evicted oldest snippet");
            } else {
                
                warn!("Token limit exceeded but nothing to evict. Total tokens: {}", self.total_token_count);
                return Err(anyhow!("Cannot reduce tokens below limit, history and snippets are empty, but total_token_count ({}) > max_tokens ({})", self.total_token_count, self.max_tokens));
            }
        }
        Ok(())
    }

    
    
    
    pub fn construct_api_messages(&mut self) -> Result<Vec<Message>> {
        
        self.ensure_token_limit()
            .context("Failed to ensure token limit before constructing API messages")?;

        let mut api_messages = Vec::new();
        let mut current_tokens = 0;

        
        
        
        for snippet in self.context_snippets.iter().rev() {
             let formatted_content = Self::format_snippet_content(&snippet.source, &snippet.content);
             
             let snippet_tokens = self.count_tokens(&formatted_content); 
             if current_tokens + snippet_tokens <= self.max_tokens {
                 api_messages.push(Message {
                     role: Role::System, 
                     content: Some(formatted_content), 
                     tool_calls: None, 
                     tool_call_id: None, 
                 });
                 current_tokens += snippet_tokens;
             } else {
                 warn!(source = %snippet.source, "Skipping snippet during construction due to token limit");
             }
        }
        
        api_messages.reverse();


        
        
        for (message, message_tokens) in self.history.iter().rev() {
            if current_tokens + message_tokens <= self.max_tokens {
                api_messages.push(message.clone());
                current_tokens += message_tokens;
            } else {
                 warn!(role = ?message.role, "Skipping history message during construction due to token limit");
                 
                 break;
            }
        }

        
        
        let history_start_index = self.context_snippets.len(); 
        if api_messages.len() > history_start_index {
             api_messages[history_start_index..].reverse();
        }


        debug!(messages_count = api_messages.len(), final_tokens = current_tokens, "Constructed API messages");
        Ok(api_messages)
    }

    
    #[allow(dead_code)]
    pub fn config(&self) -> &Config {
        &self.config
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::Role;
    use crate::config::Config;

    fn create_test_manager() -> ContextManager {
        let config = Config::default(); 
        ContextManager::new(config).expect("Failed to create test ContextManager")
    }

     fn create_test_manager_with_limit(limit: usize) -> ContextManager {
        let config = Config::default();
        
        let mut manager = ContextManager::new(config).expect("Failed to create test ContextManager");
        manager.max_tokens = limit; 
        manager
    }


    #[test]
    fn test_token_counting() {
        let manager = create_test_manager();
        let tokens = manager.count_tokens("Hello world"); 
        assert!(tokens > 0, "Token count should be positive");
        
        assert_eq!(tokens, 2);

        let tokens_complex = manager.count_tokens("複雑なテキスト"); 
         assert!(tokens_complex > 0, "Token count for complex text should be positive");
    }

    #[test]
    fn test_add_message() {
        let mut manager = create_test_manager();
        let msg = Message {
            role: Role::User,
            content: Some("Test message".to_string()), 
            tool_calls: None, 
            tool_call_id: None, 
        };
        let initial_tokens = manager.total_token_count;

        manager.add_message(msg.clone()).unwrap();

        assert_eq!(manager.history.len(), 1);
        assert_eq!(manager.history[0].0.content, msg.content); 
        assert!(manager.total_token_count > initial_tokens);
        
        let expected_tokens = msg.content.as_ref().map_or(0, |c| manager.count_tokens(c)); 
        assert_eq!(manager.history[0].1, expected_tokens);
    }

    #[test]
    fn test_basic_eviction_history() {
        
        
        let mut manager = create_test_manager_with_limit(20);

        
        for i in 0..10 {
            let msg = Message {
                role: Role::User,
                content: Some(format!("Message {}", i)), 
                tool_calls: None, 
                tool_call_id: None, 
             };
            manager.add_message(msg).unwrap();
        }

        assert!(manager.total_token_count <= manager.max_tokens, "Total tokens should be within limit after eviction");
        assert!(!manager.history.is_empty(), "History should not be empty after eviction (unless limit is tiny)");
        
        assert!(manager.history.iter().any(|(m, _)| m.content == Some("Message 9".to_string()))); 
         
        assert!(!manager.history.iter().any(|(m, _)| m.content == Some("Message 0".to_string()))); 
    }

    // Removed tests relying on add_snippet:
    // - test_basic_eviction_snippets
    // - test_eviction_mixed
    // - test_construct_api_messages_format
    // - test_construct_api_messages_respects_limit
    // - test_clear_history (modified version might be possible if needed)
    // - test_clear_snippets (modified version might be possible if needed)
}
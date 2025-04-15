use crate::config::Config;
use crate::tools::{ToolRegistry, ToolError};
use anyhow::{Context, Result};
use reqwest::{Client, header::{HeaderMap, HeaderValue, USER_AGENT}}; // Removed AUTHORIZATION
use serde::{Deserialize, Serialize};
use serde_json::Value; // For tool arguments and results
use jsonschema::{validator_for, validate, is_valid, ValidationError};
// Removed unused HashMap import
use std::time::Duration; // For request timeout

const OPENROUTER_API_BASE_URL: &str = "https://openrouter.ai/api/v1";
const REQUEST_TIMEOUT_SECONDS: u64 = 120; // Timeout for API requests

// Placeholder for app URL and name - replace with actual values or make configurable
const HTTP_REFERER: &str = "http://localhost:3000"; // Example value
const X_TITLE: &str = "OpenCode CLI"; // Example value

#[derive(Debug)]
pub struct ApiClient {
    client: Client,
    config: Config,
    api_key: String, // Store the retrieved key
}

// --- Request Structures ---

#[derive(Serialize, Debug, Clone)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    // Add other parameters like top_p, stop sequences if needed
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] // Added PartialEq
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: Option<String>, // Optional for assistant messages with tool calls
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>, // Assistant requests tool use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>, // ID for Tool role message when returning result
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String, // Typically "function"
    pub function: FunctionDefinition,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema object
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)] // Can be "none", "auto", or specific tool
pub enum ToolChoice {
    None,
    Auto,
    Tool {
        #[serde(rename = "type")]
        tool_type: String, // "function"
        function: ToolChoiceFunction,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolChoiceFunction {
    pub name: String,
}

// --- Response Structures ---

#[derive(Deserialize, Debug, Clone)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String, // e.g., "chat.completion"
    pub created: u64,   // Unix timestamp
    pub model: String,
    pub choices: Vec<Choice>,
    #[serde(default)] // Usage might not be present in streaming chunks final message
    pub usage: Option<UsageStats>,
    // Anthropic specific stop reason for tool use might appear here
    #[serde(default)]
    pub stop_reason: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Choice {
    pub index: u32,
    pub message: Message, // Contains the response content and/or tool calls
    pub finish_reason: Option<String>, // e.g., "stop", "length", "tool_calls"
    // logprobs field omitted for simplicity
}

#[derive(Serialize, Deserialize, Debug, Clone)] // Added Serialize
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String, // "function"
    pub function: ToolCallFunction,
}

#[derive(Serialize, Deserialize, Debug, Clone)] // Added Serialize
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String, // Arguments are a JSON string
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct UsageStats {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// --- Streaming Chunk Structure (Simplified) ---
// Note: Real SSE handling is more complex, parsing `data:` lines.
// This structure represents the typical JSON payload within a data line.
#[derive(Deserialize, Debug, Clone)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String, // e.g., "chat.completion.chunk"
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
    #[serde(default)]
    pub usage: Option<UsageStats>, // Usually null until the final chunk
}

#[derive(Deserialize, Debug, Clone)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: Delta, // The changed content/tool call
    pub finish_reason: Option<String>, // Null until the final chunk
    // logprobs omitted
}

#[derive(Deserialize, Debug, Clone)]
pub struct Delta {
    #[serde(default)]
    pub role: Option<Role>, // Usually only present in the first chunk
    #[serde(default)]
    pub content: Option<String>, // Text delta
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallChunk>>, // Tool call delta
}

// Represents a tool call within a streaming chunk delta
#[derive(Deserialize, Debug, Clone)]
pub struct ToolCallChunk {
    pub index: u32, // Index of the tool call if multiple are streamed
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "type")]
    pub tool_type: Option<String>, // "function"
    #[serde(default)]
    pub function: Option<ToolCallFunctionChunk>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ToolCallFunctionChunk {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>, // Argument string delta
}


#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

// --- ApiClient Implementation ---

impl ApiClient {
    /// Creates a new API client instance.
    /// Requires loaded config and retrieves the API key.
    pub fn new(config: Config) -> Result<Self> {
        let api_key = config.get_api_key()?
            .context("OpenRouter API key not found in keyring. Please set it using 'opencode configure --set-api-key'.")?;

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_str(&format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")))?);
        headers.insert("HTTP-Referer", HeaderValue::from_static(HTTP_REFERER)); // Add Referer
        headers.insert("X-Title", HeaderValue::from_static(X_TITLE)); // Add Title

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .build()
            .context("Failed to build reqwest client")?;

        Ok(ApiClient {
            client,
            config,
            api_key,
        })
    }

    /// Makes an authenticated POST request to the specified OpenRouter endpoint.
    async fn post_request<T: Serialize + std::fmt::Debug, R: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}/{}", OPENROUTER_API_BASE_URL, endpoint.trim_start_matches('/'));
        tracing::debug!(url = %url, "Making POST request");
        // Avoid logging the full body in production if it contains sensitive data
        // tracing::trace!(body = ?body, "Request body");

        let response = self.client.post(&url)
            .bearer_auth(&self.api_key)
            .json(body)
            .send()
            .await
            .with_context(|| format!("Failed to send request to {}", url))?;

        // Check for HTTP errors (4xx, 5xx)
        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_else(|_| "Could not read error body".to_string());
            tracing::error!(status = %status, body = %error_body, "API request failed");
            anyhow::bail!("API request failed with status {}: {}", status, error_body);
        }

        let response_body = response
            .json::<R>()
            .await
            .with_context(|| format!("Failed to deserialize response from {}", url))?;

        tracing::debug!("Successfully received and deserialized response");
        Ok(response_body)
    }

    /// Performs a non-streaming chat completion request.
    pub async fn chat_completion(
        &self,
        mut request: ChatCompletionRequest, // Take the full request struct
    ) -> Result<ChatCompletionResponse> {
        if request.stream == Some(true) {
             anyhow::bail!("Streaming chat completion is not yet implemented in this function.");
        }
        // Ensure stream is not set or false for non-streaming request
        request.stream = None;

        tracing::info!(model = %request.model, "Requesting non-streaming chat completion");
        self.post_request("/chat/completions", &request).await
    }

    /// Parses and validates tool calls from a chat completion response.
    pub fn parse_and_validate_tool_calls(
        &self,
        response: &ChatCompletionResponse,
        tool_registry: &ToolRegistry,
    ) -> Result<Vec<ValidatedToolCall>, ToolError> {
        let mut validated_calls = Vec::new();

        // Task 4.3.1: Detect Tool Calls
        if let Some(choice) = response.choices.first() {
            if choice.finish_reason == Some("tool_calls".to_string()) {
                if let Some(tool_calls) = &choice.message.tool_calls {
                    // Task 4.3.2: Extract Tool Call Details & Tasks 4.3.3/4.3.4: Parse/Validate
                    for tool_call in tool_calls {
                        let tool_name = &tool_call.function.name;
                        let arguments_str = &tool_call.function.arguments;

                        // Task 4.3.3: Parse arguments string
                        let arguments_value: Value = serde_json::from_str(arguments_str)
                            .map_err(|e| ToolError::InvalidArguments {
                                tool_name: tool_name.clone(),
                                details: format!("Failed to parse JSON arguments: {}. Raw: '{}'", e, arguments_str),
                            })?;

                        // Get schema from registry
                        let tool = tool_registry.get_tool(tool_name)
                            .ok_or_else(|| ToolError::Other { message: format!("Tool '{}' requested by model not found in registry.", tool_name) })?;
                        
                        let schema_value = tool.parameters_schema()
                            .map_err(|e| ToolError::Other { message: format!("Failed to get schema for tool '{}': {}", tool_name, e) })?;

                        // Task 4.3.4: Compile and Validate schema
                        let validator = jsonschema::validator_for(&schema_value)
                            .map_err(|e| ToolError::Other { message: format!("Failed to compile schema for tool '{}': {}", tool_name, e) })?;

                        let validation_result = validator.validate(&arguments_value);

                        if let Err(error) = validation_result {
                            let error_details = format!("[{:?}]: {:?}", error.instance_path, error.kind);
                            return Err(ToolError::InvalidArguments {
                                tool_name: tool_name.clone(),
                                details: format!("Schema validation failed: {}", error_details),
                            });
                        }

                        // If validation passes
                        validated_calls.push(ValidatedToolCall {
                            id: tool_call.id.clone(),
                            name: tool_name.clone(),
                            arguments: arguments_value,
                        });
                    }
                }
            }
        }

        Ok(validated_calls)
    }

    // Placeholder for streaming chat completion (Task 2.1 continued)
    // This would likely return a stream or use a callback/channel
    // pub async fn chat_completion_stream(...) -> Result<impl Stream<Item = Result<ChatCompletionChunk>>> { ... }

}

// --- Tests ---
#[cfg(test)]
mod tests {
    use super::*;
    // Removed unused CliTool import
    use crate::tools::ToolRegistry;
    use crate::tools::tests::DummyTool; // Import from the test module
    use serde_json::json;
    // Removed unused anyhow::Result import
    // Removed unused async_trait::async_trait import

    // MockTool definition removed, using DummyTool from tools::tests

    fn create_mock_tool_registry() -> ToolRegistry {
        let mut registry = ToolRegistry::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "param1": { "type": "string" },
                "param2": { "type": "integer" }
            },
            "required": ["param1"]
        });
        let tool = Box::new(DummyTool::new("mock_tool", "Mock tool description", schema));
        registry.register(tool);
        registry
    }

    fn create_mock_response(finish_reason: Option<&str>, tool_calls: Option<Vec<ToolCall>>) -> ChatCompletionResponse {
        ChatCompletionResponse {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion".to_string(),
            created: 1677652288,
            model: "gpt-4".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: Role::Assistant,
                    content: None,
                    tool_calls,
                    tool_call_id: None,
                },
                finish_reason: finish_reason.map(String::from),
            }],
            usage: None,
            stop_reason: None,
        }
    }

    // Helper to create ApiClient without needing actual config/keyring for these tests
    fn create_test_api_client() -> ApiClient {
        // We don't actually make network calls in these tests, so dummy values are fine.
        let client = Client::builder().build().unwrap();
        let config = Config::default(); // Or a mock config if needed
        ApiClient {
            client,
            config,
            api_key: "dummy_key".to_string(),
        }
    }

    #[test]
    fn test_parse_validate_successful() {
        let client = create_test_api_client();
        let registry = create_mock_tool_registry();
        let tool_calls = vec![ToolCall {
            id: "call_123".to_string(),
            tool_type: "function".to_string(),
            function: ToolCallFunction {
                name: "mock_tool".to_string(),
                arguments: json!({ "param1": "value1", "param2": 100 }).to_string(),
            },
        }];
        let response = create_mock_response(Some("tool_calls"), Some(tool_calls));

        let result = client.parse_and_validate_tool_calls(&response, &registry);
        assert!(result.is_ok());
        let validated = result.unwrap();
        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0], ValidatedToolCall {
            id: "call_123".to_string(),
            name: "mock_tool".to_string(),
            arguments: json!({ "param1": "value1", "param2": 100 }),
        });
    }

    #[test]
    fn test_parse_validate_no_tool_calls_finish_reason() {
        let client = create_test_api_client();
        let registry = create_mock_tool_registry();
        let response = create_mock_response(Some("stop"), None);
        let result = client.parse_and_validate_tool_calls(&response, &registry);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_validate_no_tool_calls_in_message() {
        let client = create_test_api_client();
        let registry = create_mock_tool_registry();
        let response = create_mock_response(Some("tool_calls"), None);
        let result = client.parse_and_validate_tool_calls(&response, &registry);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_validate_invalid_json_arguments() {
        let client = create_test_api_client();
        let registry = create_mock_tool_registry();
        let tool_calls = vec![ToolCall {
            id: "call_err".to_string(),
            tool_type: "function".to_string(),
            function: ToolCallFunction {
                name: "mock_tool".to_string(),
                arguments: "{ invalid json ".to_string(),
            },
        }];
        let response = create_mock_response(Some("tool_calls"), Some(tool_calls));

        let result = client.parse_and_validate_tool_calls(&response, &registry);
        assert!(result.is_err());
        match result.err().unwrap() {
            ToolError::InvalidArguments { tool_name, details } => {
                assert_eq!(tool_name, "mock_tool");
                assert!(details.contains("Failed to parse JSON arguments"));
            }
            _ => panic!("Expected InvalidArguments error"),
        }
    }

    #[test]
    fn test_parse_validate_schema_validation_failed_missing_required() {
        let client = create_test_api_client();
        let registry = create_mock_tool_registry();
        let tool_calls = vec![ToolCall {
            id: "call_val_err".to_string(),
            tool_type: "function".to_string(),
            function: ToolCallFunction {
                name: "mock_tool".to_string(),
                arguments: json!({ "param2": 123 }).to_string(), // Missing required 'param1'
            },
        }];
        let response = create_mock_response(Some("tool_calls"), Some(tool_calls));

        let result = client.parse_and_validate_tool_calls(&response, &registry);
        assert!(result.is_err());
        match result.err().unwrap() {
            ToolError::InvalidArguments { tool_name, details } => {
                assert_eq!(tool_name, "mock_tool");
                assert!(details.contains("Schema validation failed"));
                assert!(details.contains("'param1' is a required property")); // Specific jsonschema error
            }
            _ => panic!("Expected InvalidArguments error"),
        }
    }

    #[test]
    fn test_parse_validate_schema_validation_failed_wrong_type() {
        let client = create_test_api_client();
        let registry = create_mock_tool_registry();
        let tool_calls = vec![ToolCall {
            id: "call_type_err".to_string(),
            tool_type: "function".to_string(),
            function: ToolCallFunction {
                name: "mock_tool".to_string(),
                arguments: json!({ "param1": 123 }).to_string(), // param1 should be string
            },
        }];
        let response = create_mock_response(Some("tool_calls"), Some(tool_calls));

        let result = client.parse_and_validate_tool_calls(&response, &registry);
        assert!(result.is_err());
        match result.err().unwrap() {
            ToolError::InvalidArguments { tool_name, details } => {
                assert_eq!(tool_name, "mock_tool");
                assert!(details.contains("Schema validation failed"));
                assert!(details.contains("123 is not of type 'string'")); // Specific jsonschema error
            }
            _ => panic!("Expected InvalidArguments error"),
        }
    }

    #[test]
    fn test_parse_validate_tool_not_found() {
        let client = create_test_api_client();
        let registry = ToolRegistry::new(); // Empty registry
        let tool_calls = vec![ToolCall {
            id: "call_notfound".to_string(),
            tool_type: "function".to_string(),
            function: ToolCallFunction {
                name: "unknown_tool".to_string(),
                arguments: json!({}).to_string(),
            },
        }];
        let response = create_mock_response(Some("tool_calls"), Some(tool_calls));

        let result = client.parse_and_validate_tool_calls(&response, &registry);
        assert!(result.is_err());
        match result.err().unwrap() {
            ToolError::Other { message } => {
                assert!(message.contains("Tool 'unknown_tool' requested by model not found"));
            }
            _ => panic!("Expected Other error for tool not found"),
        }
    }
}
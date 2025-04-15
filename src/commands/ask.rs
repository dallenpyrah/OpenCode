use anyhow::{Context, Result}; // Removed anyhow
use serde_json;

use crate::api::client::ApiClient;
use crate::api::models::{ChatCompletionRequest, Message, Role, ToolChoice};
use crate::config::Config;
use crate::context::ContextManager;
use crate::tools::execution::ToolExecutionEngine;
use crate::tools::registry::ToolRegistry;
use crate::tools::ToolError;
use crate::tui::{print_error, print_result, print_warning, start_spinner}; // Removed print_info

pub async fn handle_ask(
    config: Config,
    mut context_manager: ContextManager,
    tool_registry: &ToolRegistry,
    tool_engine: &ToolExecutionEngine<'_>,
    prompt: String,
) -> Result<()> {
    let api_client = ApiClient::new(config.clone())
        .context("Failed to create API client (check API key configuration)")?;
    tracing::debug!("Processing 'ask' command with prompt: '{}'", prompt);
    let user_message = Message {
        role: Role::User,
        content: Some(prompt),
        tool_calls: None,
        tool_call_id: None,
    };
    context_manager.add_message(user_message.clone())?;
    let messages_for_api = context_manager.construct_api_messages()?;
    if messages_for_api.is_empty() {
        anyhow::bail!("Cannot send empty message list to API.");
    }

    let tool_definitions = tool_registry.get_tool_definitions()
        .context("Failed to get tool definitions from registry")?;

    let request = ChatCompletionRequest {
        model: config.api.default_model.clone(),
        messages: messages_for_api,
        stream: None,
        temperature: None,
        max_tokens: None,
        tools: Some(tool_definitions),
        tool_choice: Some(ToolChoice::Auto),
        source_map: None,
    };
    tracing::debug!("Sending request to API: {:?}", request);
    let spinner = start_spinner("Waiting for API response...");
    let result = api_client.chat_completion(request).await;
    spinner.finish_and_clear();
    match result {
        Ok(response) => {
            tracing::debug!("Received response from API: {:?}", response);
            if let Some(choice) = response.choices.first() {
                context_manager.add_message(choice.message.clone())?;
                tracing::debug!("Added assistant message (potentially with tool calls) to context.");

                let mut tool_results_with_ids: Vec<(String, Result<serde_json::Value, ToolError>)> = Vec::new();

                if let Some(tool_calls) = &choice.message.tool_calls {
                    for tool_call in tool_calls {
                        let tool_call_id = tool_call.id.clone();
                        let tool_name = &tool_call.function.name;
                        let arguments_str = &tool_call.function.arguments;

                        let arguments_value = match serde_json::from_str(arguments_str) {
                            Ok(val) => val,
                            Err(e) => {
                                let error_result = Err(ToolError::InvalidArguments {
                                    tool_name: tool_name.clone(),
                                    details: format!("Failed to parse JSON arguments: {}", e),
                                });
                                tool_results_with_ids.push((tool_call_id, error_result));
                                continue;
                            }
                        };

                        let tool_result = tool_engine.execute_tool_call(tool_name, arguments_value).await;

                        print_result(&format!("Tool Call ID: {}, Result: {:?}", tool_call_id, tool_result));
                        tool_results_with_ids.push((tool_call_id, tool_result));
                    }
                }

                for (id, result) in tool_results_with_ids {
                    let content_string = match result {
                        Ok(value) => serde_json::to_string(&value)
                            .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize tool result: {}\"}}", e)),
                        Err(e) => serde_json::to_string(&serde_json::json!({ "error": e.to_string() }))
                            .unwrap_or_else(|_| format!("{{\"error\": \"Failed to serialize tool error: {}\"}}", e)),
                    };

                    let tool_message = Message {
                        role: Role::Tool,
                        content: Some(content_string),
                        tool_calls: None,
                        tool_call_id: Some(id),
                    };
                    context_manager.add_message(tool_message)?;
                    tracing::debug!("Added tool result message to context.");
                }

                if let Some(content) = &choice.message.content {
                     if !content.is_empty() {
                        print_result(content);
                     }
                } else if choice.message.tool_calls.is_none() {
                     print_warning("Assistant response content was empty and no tool calls were made.");
                     tracing::warn!("Assistant response content was None and no tool calls were made.");
                }

            } else {
                print_warning("No choices received from API.");
                tracing::warn!("No choices received in API response.");
            }
        }
        Err(e) => {
            print_error(&format!("Error interacting with the AI: {}", e));
        }
    }
    Ok(())
}
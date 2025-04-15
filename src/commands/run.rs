use anyhow::{anyhow, Context, Result};
use serde_json;

use crate::api::client::ApiClient;
use crate::api::models::{ChatCompletionRequest, Message, Role, ToolChoice};
use crate::cli::commands::RunArgs;
use crate::config::Config;
use crate::context::ContextManager;
use crate::tools; // For tool_result_format
use crate::tools::execution::ToolExecutionEngine;
use crate::tools::registry::ToolRegistry;
use crate::tui::{print_error, print_info, print_result, print_warning, start_spinner};
use crate::app::generate_source_map;
use std::env;

const MAX_ITERATIONS: usize = 5;

pub async fn handle_run(
    config: Config,
    mut context_manager: ContextManager,
    tool_registry: &ToolRegistry,
    tool_engine: &ToolExecutionEngine<'_>,
    args: RunArgs,
) -> Result<()> {
    let api_client = ApiClient::new(config.clone())
        .context("Failed to create API client (check API key configuration)")?;
    tracing::info!("Processing 'run' command with task: '{}'", args.task_description);
    print_info(&format!("Starting agentic task: {}", args.task_description));

    context_manager.clear_history();
    context_manager.clear_snippets();
    let initial_prompt = format!(
        "You are an AI assistant tasked with completing the following objective: '{}'. \
        Break down the task into steps and use the available tools to execute those steps. \
        Respond with the next single tool call required, or indicate if the task is complete.",
        args.task_description
    );
    let system_message = Message {
        role: Role::System,
        content: Some(initial_prompt),
        tool_calls: None,
        tool_call_id: None,
    };
    context_manager.add_message(system_message)?;

    let mut task_complete = false;

    for i in 0..MAX_ITERATIONS {
        print_info(&format!("Iteration {}/{}", i + 1, MAX_ITERATIONS));
        tracing::debug!("Agentic loop iteration {} starting.", i + 1);

        let messages_for_api = context_manager.construct_api_messages()?;
        if messages_for_api.is_empty() {
            print_error("Cannot send empty message list to API.");
            break;
        }

        let tool_definitions = tool_registry.get_tool_definitions()
            .context("Failed to get tool definitions from registry")?;

        let current_dir = env::current_dir().context("Failed to get current directory for source map generation")?;
        let source_map = match generate_source_map(&current_dir) {
            Ok(map) => Some(map),
            Err(e) => {
                tracing::error!("Failed to generate source map: {}", e);
                print_error(&format!("Failed to generate source map: {}", e));
                None
            }
        };

        let request = ChatCompletionRequest {
            model: config.api.default_model.clone(),
            messages: messages_for_api,
            stream: None,
            temperature: None,
            max_tokens: None,
            tools: Some(tool_definitions),
            tool_choice: Some(ToolChoice::Auto),
            source_map: source_map,
        };

        tracing::debug!("Sending agent request to API: {:?}", request);
        let spinner = start_spinner("Waiting for AI step...");
        let result = api_client.chat_completion(request).await;
        spinner.finish_and_clear();

        match result {
            Ok(response) => {
                tracing::debug!("Received agent response from API: {:?}", response);
                if let Some(choice) = response.choices.first() {
                    context_manager.add_message(choice.message.clone())?;
                    tracing::debug!("Added assistant message to context.");

                    let mut tool_results_with_ids: Vec<(String, serde_json::Value)> = Vec::new();
                    let mut tool_execution_occurred = false;
                    let mut tool_execution_failed = false;

                    if let Some(tool_calls) = &choice.message.tool_calls {
                        tool_execution_occurred = true;
                        for tool_call in tool_calls {
                            let tool_call_id = tool_call.id.clone();
                            let tool_name = &tool_call.function.name;
                            let arguments_str = &tool_call.function.arguments;
                            print_info(&format!("Attempting tool call: {} with ID: {}", tool_name, tool_call_id));
                            tracing::info!("Attempting tool call: {} (ID: {})", tool_name, tool_call_id);

                            let arguments_value = match serde_json::from_str(arguments_str) {
                                Ok(val) => val,
                                Err(e) => {
                                    let error_msg = format!("Failed to parse JSON arguments for tool '{}': {}", tool_name, e);
                                    print_error(&error_msg);
                                    tracing::error!("{}", error_msg);

                                    let error_value = tools::tool_result_format::format_tool_result(
                                        tool_name,
                                        &serde_json::Value::Null,
                                        Some(&error_msg),
                                    );
                                    tool_results_with_ids.push((tool_call_id, error_value));
                                    tool_execution_failed = true;
                                    continue;
                                }
                            };

                            let tool_result = tool_engine.execute_tool_call(tool_name, arguments_value).await;

                            // The match block below handles both Ok and Err for storing the result.
                            // This first match block for logging/checking is removed to potentially fix E0282.
                             match tool_result { // This match now starts at the original line 134
                                Ok(value) => tool_results_with_ids.push((tool_call_id, value)),
                                Err(e) => {
                                     let error_value = tools::tool_result_format::format_tool_result(
                                        tool_name,
                                        &serde_json::Value::Null,
                                        Some(&e.to_string()),
                                    );
                                    tool_results_with_ids.push((tool_call_id, error_value));
                                }
                            }
                        }
                    }

                    for (id, value) in tool_results_with_ids {
                        let content_string = serde_json::to_string(&value)
                            .map_err(|e| anyhow!("Failed to serialize tool result value: {}", e))?;

                        let tool_message = Message {
                            role: Role::Tool,
                            content: Some(content_string),
                            tool_calls: None,
                            tool_call_id: Some(id),
                        };

                        tracing::debug!("Adding tool result message to context for tool_call_id: {}", tool_message.tool_call_id.as_deref().unwrap_or("unknown"));
                        context_manager.add_message(tool_message)?;
                    }

                    if tool_execution_failed {
                        print_error("Agentic task failed due to tool execution error.");
                        tracing::error!("Agentic task failed due to tool execution error.");
                        break;
                    } else if !tool_execution_occurred {
                        if let Some(content) = &choice.message.content {
                            if !content.is_empty() {
                                print_result(&format!("AI Response: {}", content));
                                if content.to_lowercase().contains("task complete") || content.to_lowercase().contains("task finished") {
                                    print_info("Task marked as complete by AI.");
                                    task_complete = true;
                                    break;
                                }
                            } else {
                                 print_warning("AI responded with empty content and no tool calls.");
                                 tracing::warn!("AI responded with empty content and no tool calls in agentic loop.");
                                 print_error("Agentic task stalled: AI provided no action or completion signal.");
                                 break;
                            }
                        } else {
                             print_warning("AI responded with no content and no tool calls.");
                             tracing::warn!("AI responded with None content and no tool calls in agentic loop.");
                             print_error("Agentic task stalled: AI provided no action or completion signal.");
                             break;
                        }
                    }
                } else {
                    print_warning("No choices received from API in agentic loop.");
                    tracing::warn!("No choices received in API response during agentic loop.");
                    break;
                }
            }
            Err(e) => {
                print_error(&format!("Error interacting with the AI during agentic loop: {}", e));
                tracing::error!("API error during agentic loop: {}", e);
                break;
            }
        }
    }

    if task_complete {
         print_info("Agentic task finished successfully.");
         tracing::info!("Agentic task finished successfully.");
    } else {
         print_warning(&format!("Agentic task stopped after {} iterations.", MAX_ITERATIONS));
         tracing::warn!("Agentic task stopped after max iterations.");
    }
    Ok(())
}
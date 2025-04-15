use anyhow::{Context, Result};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::fs;
use std::env;
use dirs;
use std::path::Path;

use crate::api::client::ApiClient;
use crate::api::models::{ChatCompletionRequest, Message, Role, ToolChoice};
use crate::config::{Config, GLOBAL_CONFIG_DIR};
use crate::context::ContextManager;
use crate::tui::{print_error, print_info, print_warning};
use crate::tools::execution::ToolExecutionEngine;
use crate::tools::registry::ToolRegistry;
use crate::app::generate_source_map;
use crate::tools::ToolError;

use futures_util::StreamExt;
use std::io::Write;

pub async fn run_interactive_mode<'a>(
    config: Config,
    api_client: ApiClient,
    mut context_manager: ContextManager,
    tool_registry: &'a ToolRegistry,
    tool_execution_engine: &'a ToolExecutionEngine<'a>,
) -> Result<()> {
    tracing::info!("Checking codebase access...");
    let current_dir = std::env::current_dir()?;
    tracing::info!("Current directory: {:?}", current_dir);

    let can_read_workspace = fs::read_dir(&current_dir).is_ok();
    tracing::info!("Can read workspace: {}", can_read_workspace);

    if !can_read_workspace {
        tracing::error!("I don't have access to your specific codebase!");
        print_error("I don't have access to your specific codebase!");
        return Ok(());
    }
    tracing::info!("Starting interactive mode...");
    print_info("Welcome to OpenCode Interactive Mode! Type /help for commands, /exit to quit.");

    let mut rl = DefaultEditor::new().context("Failed to create readline editor")?;

    let history_path_opt = match dirs::config_dir() {
        Some(mut path) => {
            path.push(GLOBAL_CONFIG_DIR);

            if !path.exists() {
                if let Err(e) = fs::create_dir_all(&path) {
                    tracing::warn!("Failed to create config directory {:?}: {}. History will not be saved.", path, e);
                    print_warning(&format!("Could not create config directory for history: {}", e));
                } else {
                    tracing::debug!("Created config directory for history: {:?}", path);
                }
            }
            path.push("repl_history.txt");
            Some(path)
        }
        None => {
            tracing::warn!("Could not determine user config directory for REPL history.");
            print_warning("Could not determine config directory to load/save history.");
            None
        }
    };

    if let Some(ref history_path) = history_path_opt {
        if history_path.exists() {
            if let Err(e) = rl.load_history(history_path) {
                tracing::warn!("Failed to load REPL history from {:?}: {}", history_path, e);
                print_warning(&format!("Could not load history: {}", e));
            } else {
                tracing::debug!("Loaded REPL history from {:?}", history_path);
            }
        }
    }

    let tool_definitions = match tool_registry.get_tool_definitions() {
        Ok(defs) => {
            tracing::info!("Loaded {} tool definitions.", defs.len());
            Some(defs)
        },
        Err(e) => {
            tracing::error!("Failed to load tool definitions: {}", e);
            print_error(&format!("Failed to load tool definitions: {}", e));
            None
        }
    };

    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                let trimmed_line = line.trim();
                if trimmed_line.is_empty() {
                    continue;
                }

                if let Err(e) = rl.add_history_entry(trimmed_line) {
                     tracing::warn!("Failed to add line to history: {}", e);
                }

                match trimmed_line {
                    "/exit" => {
                        tracing::info!("Exiting interactive mode via /exit command.");
                        break;
                    }
                    "/help" => {
                        print_info("Available commands:");
                        print_info("  /exit    - Quit the interactive session.");
                        print_info("  /help    - Show this help message.");
                        print_info("  /clear   - Clear the conversation history.");
                    }
                    "/clear" => {
                        context_manager.clear_history();
                        print_info("Conversation history cleared.");
                        tracing::debug!("Cleared conversation history via /clear command.");
                    }
                    _ => {
                        let user_message = Message {
                            role: Role::User,
                            content: Some(trimmed_line.to_string()),
                            tool_calls: None,
                            tool_call_id: None,
                        };
                        context_manager.add_message(user_message)?;

                        let messages_for_api = context_manager.construct_api_messages()?;
                        if messages_for_api.is_empty() {
                            print_warning("Cannot send empty message list to API.");
                            continue;
                        }

                        let current_dir = env::current_dir()?;
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
                            stream: Some(true),
                            temperature: None,
                            max_tokens: None,
                            tools: tool_definitions.clone(), // Include tool definitions
                            tool_choice: if tool_definitions.is_some() { Some(ToolChoice::Auto) } else { None }, // Set tool_choice to auto if tools exist
                            source_map: source_map.clone(), // Clone source_map here
                        };

                        tracing::debug!("Sending interactive request to API (streaming): {:?}", request);
                        match api_client.chat_completion_stream(request).await {
                            Ok(mut stream) => {
                                tracing::debug!("Received interactive stream from API.");
                                let mut accumulated_content = String::new();
                                let mut accumulated_tool_calls: Vec<crate::api::models::ToolCall> = Vec::new();
                                let mut current_tool_calls: Option<Vec<crate::api::models::ToolCall>> = None; // To handle incremental tool calls

                                print_info("Assistant: "); // Indicate AI is responding

                                while let Some(chunk_result) = stream.next().await {
                                    match chunk_result {
                                        Ok(chunk) => {
                                            if let Some(choice) = chunk.choices.first() {
                                                if let Some(content_text) = &choice.delta.content {
                                                    if !content_text.is_empty() {
                                                        print!("{}", content_text); // Print content as it arrives
                                                        std::io::stdout().flush().ok();
                                                        accumulated_content.push_str(content_text);
                                                    }
                                                }
                                                // Handle potential tool calls in delta
                                                if let Some(delta_tool_calls) = &choice.delta.tool_calls {
                                                    // This part needs refinement based on how streaming tool calls are structured.
                                                    // Assuming for now they might come in full or partial chunks.
                                                    // A simple approach: collect all tool calls received.
                                                    // More complex logic might be needed to merge partial tool call chunks.
                                                    if current_tool_calls.is_none() {
                                                        current_tool_calls = Some(Vec::new());
                                                    }
                                                    // For simplicity, let's assume tool calls arrive fully formed in deltas for now.
                                                    // A robust implementation would handle partial updates.
                                                    current_tool_calls.as_mut().unwrap().extend(delta_tool_calls.iter().cloned());
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            print_error(&format!("\nError processing stream chunk: {}", e));
                                            tracing::error!("Error processing stream chunk: {}", e);
                                            // Decide how to handle stream errors, maybe break or return error
                                            break; // Stop processing on error
                                        }
                                    }
                                }
                                println!(); // Newline after streaming is complete

                                // Consolidate accumulated tool calls if any were received
                                if let Some(calls) = current_tool_calls {
                                    accumulated_tool_calls = calls;
                                }

                                // Add the initial assistant message (potentially with tool calls) to context
                                let assistant_message_response = Message {
                                    role: Role::Assistant,
                                    content: if accumulated_content.is_empty() { None } else { Some(accumulated_content.clone()) },
                                    tool_calls: if accumulated_tool_calls.is_empty() { None } else { Some(accumulated_tool_calls.clone()) },
                                    tool_call_id: None,
                                };
                                context_manager.add_message(assistant_message_response)?;
                                tracing::debug!("Added initial assistant response message to context.");


                                // --- Iterative Tool Calling Logic ---
                                let mut current_tool_calls = accumulated_tool_calls;

                                while !current_tool_calls.is_empty() {
                                    tracing::info!("Processing {} tool calls.", current_tool_calls.len());
                                    // We'll process one tool call at a time from the list received
                                    // In the future, the API might return multiple parallel calls,
                                    // but for sequential logic, we handle the first one.
                                    let tool_call = current_tool_calls.remove(0); // Take the first tool call

                                    print_info(&format!("\nExecuting tool: {} (ID: {})", tool_call.function.name, tool_call.id));
                                    let tool_name = &tool_call.function.name;
                                    let tool_args_str = &tool_call.function.arguments;

                                    let arguments_value: serde_json::Value = match serde_json::from_str(tool_args_str) {
                                        Ok(val) => val,
                                        Err(e) => {
                                            let error_msg = format!("Failed to parse arguments for tool '{}': {}. Arguments: '{}'", tool_name, e, tool_args_str);
                                            tracing::error!("{}", error_msg);
                                            print_error(&error_msg);
                                            serde_json::json!({ "error": error_msg })
                                        }
                                    };

                                    // Execute the single tool call
                                    let tool_result_content = match tool_execution_engine.execute_tool_call(tool_name, arguments_value).await {
                                        Ok(result) => {
                                            tracing::info!("Tool '{}' executed successfully. Result: {:?}", tool_name, result);
                                            print_info(&format!("  - Success: {}", serde_json::to_string(&result).unwrap_or_else(|_| "Result not serializable".to_string())));
                                            result
                                        },
                                        Err(ToolError::FileNotFound { path }) => {
                                            let path_obj = Path::new(&path);
                                            let filename = path_obj.file_name().map(|os| os.to_string_lossy().into_owned()).unwrap_or_else(|| path.clone());
                                            let extension = path_obj.extension().map(|os| os.to_string_lossy().into_owned());
                                            let error_msg = format!("Tool '{}' failed for '{}'. File not found.", tool_name, path);
                                            tracing::error!("{}", error_msg);
                                            print_error(&error_msg);
                                            let mut arguments = serde_json::json!({ "query": filename, "case_sensitive": false, "include_hidden": false });
                                            if let Some(ext) = extension {
                                                arguments.as_object_mut().unwrap().insert("extension".to_string(), serde_json::json!(ext));
                                            }
                                            serde_json::json!({
                                                "error": "FileNotFound",
                                                "failed_path": path,
                                                "message": error_msg,
                                                "next_action_suggestion": { "tool_name": "FileSearchTool", "arguments": arguments }
                                            })
                                        },
                                        Err(ToolError::PermissionDenied { resource }) => {
                                            let error_msg = format!("Permission denied when trying to access resource: {}", resource);
                                            tracing::error!("{}", error_msg);
                                            print_error(&error_msg);
                                            serde_json::json!({ "error": error_msg })
                                        },
                                        Err(e) => {
                                            let error_msg = format!("Error executing tool '{}': {}", tool_name, e);
                                            tracing::error!("{}", error_msg);
                                            print_error(&error_msg);
                                            serde_json::json!({ "error": error_msg })
                                        }
                                    };

                                    // Serialize tool result content first
                                    let tool_result_content_str = serde_json::to_string(&tool_result_content)
                                        .unwrap_or_else(|_| "{\"error\": \"Failed to serialize tool result\"}".to_string());
                                    tracing::debug!("Tool result content to send: {}", tool_result_content_str); // Log before sending

                                    // Add the tool result message to context
                                    let tool_result_message = Message {
                                        role: Role::Tool,
                                        tool_call_id: Some(tool_call.id.clone()),
                                        content: Some(tool_result_content_str.clone()), // Use the stored string
                                        tool_calls: None,
                                    };
                                    context_manager.add_message(tool_result_message)?;
                                    tracing::debug!("Added tool result message for call ID '{}' to context.", tool_call.id);

                                    // Send the context *with the single tool result* back to the API
                                    let messages_for_next_step = context_manager.construct_api_messages()?;
                                    if messages_for_next_step.is_empty() {
                                        print_warning("Cannot send empty message list after tool execution.");
                                        break; // Exit the tool loop if context is empty
                                    }

                                    let next_request = ChatCompletionRequest {
                                        model: config.api.default_model.clone(),
                                        messages: messages_for_next_step,
                                        stream: Some(true), // Continue streaming
                                        temperature: None,
                                        max_tokens: None,
                                        tools: tool_definitions.clone(), // Send tool definitions again, API might call another tool
                                        tool_choice: if tool_definitions.is_some() { Some(ToolChoice::Auto) } else { None },
                                        source_map: source_map.clone(),
                                    };

                                    tracing::debug!("Sending request back to API after tool execution: {:?}", next_request);
                                    print_info("\nSending tool result back to Assistant...");

                                    // Get the next response from the API (could be content or another tool call)
                                    match api_client.chat_completion_stream(next_request).await {
                                        Ok(mut next_stream) => {
                                            tracing::debug!("Received next stream from API.");
                                            let mut next_accumulated_content = String::new();
                                            let mut next_accumulated_tool_calls: Vec<crate::api::models::ToolCall> = Vec::new();
                                            let mut next_current_tool_calls: Option<Vec<crate::api::models::ToolCall>> = None;

                                            print_info("Assistant: ");

                                            while let Some(next_chunk_result) = next_stream.next().await {
                                                match next_chunk_result {
                                                    Ok(chunk) => {
                                                        if let Some(choice) = chunk.choices.first() {
                                                            if let Some(content_text) = &choice.delta.content {
                                                                if !content_text.is_empty() {
                                                                    print!("{}", content_text);
                                                                    std::io::stdout().flush().ok();
                                                                    next_accumulated_content.push_str(content_text);
                                                                }
                                                            }
                                                            if let Some(delta_tool_calls) = &choice.delta.tool_calls {
                                                                if next_current_tool_calls.is_none() {
                                                                    next_current_tool_calls = Some(Vec::new());
                                                                }
                                                                // Simple accumulation, assumes full tool calls in delta
                                                                next_current_tool_calls.as_mut().unwrap().extend(delta_tool_calls.iter().cloned());
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        print_error(&format!("\nError processing next stream chunk: {}", e));
                                                        tracing::error!("Error processing next stream chunk: {}", e);
                                                        break; // Stop processing on error
                                                    }
                                                }
                                            }
                                            println!(); // Newline after streaming

                                            if let Some(calls) = next_current_tool_calls {
                                                next_accumulated_tool_calls = calls;
                                            }

                                            tracing::debug!(
                                                "Received response after tool execution. Content: '{}', Tool Calls: {:?}",
                                                next_accumulated_content,
                                                next_accumulated_tool_calls
                                            ); // Log received content

                                            // Defensive check: Did the LLM just echo the tool result?
                                            if next_accumulated_content == tool_result_content_str && next_accumulated_tool_calls.is_empty() {
                                                let warning_msg = "Warning: Assistant failed to process the previous tool result correctly and echoed it back.";
                                                tracing::warn!("{}", warning_msg);
                                                print_warning(warning_msg);
                                                // Do NOT add this echoed message to context.
                                                // Clear remaining tool calls as the flow is broken for this turn.
                                                current_tool_calls.clear();
                                                break; // Exit the tool processing loop for this user turn
                                            } else {
                                                // Normal processing: Add the assistant's response (content or tool call) to context
                                                let next_assistant_message = Message {
                                                    role: Role::Assistant,
                                                    content: if next_accumulated_content.is_empty() { None } else { Some(next_accumulated_content.clone()) },
                                                    tool_calls: if next_accumulated_tool_calls.is_empty() { None } else { Some(next_accumulated_tool_calls.clone()) },
                                                    tool_call_id: None,
                                                };
                                                context_manager.add_message(next_assistant_message)?;
                                                tracing::debug!("Added next assistant message to context.");

                                                // Update the tool calls for the next iteration of the while loop
                                                current_tool_calls = next_accumulated_tool_calls;

                                                // If the response was content (no tool calls), break the loop
                                                if current_tool_calls.is_empty() {
                                                    if next_accumulated_content.is_empty() {
                                                        // API returned no text and no further tools after processing the last tool result.
                                                        let warn_msg = "Assistant processed the tool result but provided no further response.";
                                                        tracing::warn!("{}", warn_msg);
                                                        print_warning(warn_msg); // Inform the user directly
                                                    }
                                                    // Break regardless of content, as there are no more tools to call in this chain.
                                                    break; // Exit the tool processing loop
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            print_error(&format!("Error getting next chat stream after tool execution: {}", e));
                                            tracing::error!("Error getting next chat stream after tool execution: {}", e);
                                            current_tool_calls.clear(); // Stop processing tools on error
                                            break; // Exit the tool loop
                                        }
                                    }
                                } // End of while !current_tool_calls.is_empty() loop

                                // --- End Iterative Tool Calling Logic ---

                            }
                            Err(e) => {
                                print_error(&format!("Error getting chat stream: {}", e));
                                tracing::error!("Error getting chat stream: {}", e);
                            }
                        }

                    } // Closes _ =>
                } // Closes match input.trim()
            } // Closes Ok(input) case
            Err(ReadlineError::Interrupted) => {
                tracing::info!("Received Ctrl-C (Interrupt), exiting interactive mode.");
                print_info("Received Interrupt (Ctrl+C). Exiting.");
                break;
            }
            Err(ReadlineError::Eof) => {
                tracing::info!("Received Ctrl-D (EOF), exiting interactive mode.");
                print_info("Received EOF (Ctrl+D). Exiting.");
                break;
            }
            Err(err) => {
                print_error(&format!("Readline error: {}", err));
                tracing::error!("Readline error: {}", err);
                break;
            }
        }
    } // Closes loop

    if let Some(ref history_path) = history_path_opt {
        if let Err(e) = rl.save_history(history_path) {
            tracing::error!("Failed to save REPL history to {:?}: {}", history_path, e);
            print_error(&format!("Could not save history: {}", e));
        } else {
            tracing::debug!("Saved REPL history to {:?}", history_path);
        }
    }

    tracing::info!("Exited interactive mode.");
    Ok(())
}
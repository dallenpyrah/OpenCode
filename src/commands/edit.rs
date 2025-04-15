use anyhow::{Context, Result}; // Removed anyhow
use std::fs;
use serde_json;

use crate::api::client::ApiClient;
use crate::api::models::{ChatCompletionRequest, Message, Role, ToolChoice};
use crate::cli::commands::EditArgs;
use crate::config::Config;
use crate::tools::execution::ToolExecutionEngine;
use crate::tools::registry::ToolRegistry;
use crate::tui::{print_error, print_info, print_result, print_warning, start_spinner};

pub async fn handle_edit(
    config: Config,
    tool_registry: &ToolRegistry,
    tool_engine: &ToolExecutionEngine<'_>,
    args: EditArgs,
) -> Result<()> {
    let api_client = ApiClient::new(config.clone())
        .context("Failed to create API client (check API key configuration)")?;
    tracing::debug!(
        "Processing 'edit' command for file: '{}' with instruction: '{}'",
        args.file,
        args.instruction
    );

    let file_content = match fs::read_to_string(&args.file) {
        Ok(content) => {
            tracing::debug!("Successfully read file for editing: {}", args.file);
            content
        }
        Err(e) => {
            print_error(&format!("Could not read file '{}': {}", args.file, e));
            tracing::error!("Failed to read file for editing '{}': {}", args.file, e);
            return Err(anyhow::anyhow!("Failed to read file for editing: {}", e));
        }
    };

    let prompt = format!(
        "Apply the following edit instruction to the provided file content. \
        You MUST call the appropriate file modification tool (e.g., 'file_write', 'apply_diff') \
        to apply the changes. Output ONLY the tool call.\n\n\
        Instruction: {}\n\n\
        File Path: {}\n\n\
        File Content:\n```\n{}\n```",
        args.instruction, args.file, file_content
    );

    let user_message = Message {
        role: Role::User,
        content: Some(prompt),
        tool_calls: None,
        tool_call_id: None,
    };

    let tool_definitions = tool_registry.get_tool_definitions()
        .context("Failed to get tool definitions from registry")?;

    let request = ChatCompletionRequest {
        model: config.api.edit_model.clone(),
        messages: vec![user_message],
        stream: None,
        temperature: None,
        max_tokens: None,
        tools: if tool_definitions.is_empty() { None } else { Some(tool_definitions) },
        tool_choice: Some(ToolChoice::Auto),
        source_map: None,
    };

    tracing::debug!("Sending edit request to API: {:?}", request);
    let spinner = start_spinner("Requesting edit from AI...");
    let result = api_client.chat_completion(request).await;
    spinner.finish_and_clear();

    match result {
        Ok(response) => {
            tracing::debug!("Received edit response from API: {:?}", response);
            if let Some(choice) = response.choices.first() {
                if let Some(tool_calls) = &choice.message.tool_calls {
                    if let Some(tool_call) = tool_calls.first() {
                        let tool_name = &tool_call.function.name;
                        let arguments_str = &tool_call.function.arguments;
                        match serde_json::from_str(arguments_str) {
                            Ok(arguments_value) => {
                                let tool_result = tool_engine.execute_tool_call(tool_name, arguments_value).await;
                                print_result(&format!("Tool '{}' execution result: {:?}", tool_name, tool_result));
                            }
                            Err(e) => {
                                print_error(&format!("Failed to parse tool arguments: {}", e));
                                tracing::error!("Failed to parse tool arguments for '{}': {}", tool_name, e);
                            }
                        }
                    } else {
                        print_warning("LLM response contained an empty tool calls array.");
                        tracing::warn!("LLM response contained an empty tool calls array for edit.");
                    }
                } else {
                    print_warning("LLM did not request an edit via tool call.");
                    tracing::warn!("LLM did not request an edit via tool call.");
                    if let Some(content) = &choice.message.content {
                        print_info(&format!("LLM Response Text: {}", content));
                    }
                }
            } else {
                print_warning("No choices received from API for edit.");
                tracing::warn!("No choices received in API response for edit.");
            }
        }
        Err(e) => {
            print_error(&format!("Error requesting edit from AI: {}", e));
        }
    }
    Ok(())
}
use anyhow::{Context, Result};
use std::fs;

use crate::api::client::ApiClient;
use crate::api::models::{ChatCompletionRequest, Message, Role};
use crate::cli::commands::DebugArgs;
use crate::config::Config;
use crate::streaming::handle_streamed_response;
use crate::tui::{print_error, print_warning};

pub async fn handle_debug(
    config: Config,
    args: DebugArgs,
) -> Result<()> {
    let api_client = ApiClient::new(config.clone())
        .context("Failed to create API client (check API key configuration)")?;
    tracing::debug!(
        "Processing 'debug' command with error: '{}', file: {:?}",
        args.error,
        args.file
    );

    let file_path_str = args.file.clone().unwrap_or_else(|| "None".to_string());

    let code_context = match args.file {
        Some(path) => match fs::read_to_string(&path) {
            Ok(content) => {
                tracing::debug!("Successfully read context file: {}", path);
                Some(content)
            }
            Err(e) => {
                print_warning(&format!(
                    "Could not read context file '{}': {}. Proceeding without file context.",
                    path, e
                ));
                tracing::warn!("Failed to read context file '{}': {}", path, e);
                None
            }
        },
        None => None,
    };

    let prompt = if let Some(context) = code_context {
        format!(
            "Help me debug the following error:\n\n```\n{}\n```\n\nHere is the relevant code context from the file '{}':\n\n```rust\n{}\n```\n\nWhat could be the cause and how can I fix it?",
            args.error, file_path_str, context
        )
    } else {
        format!(
            "Help me debug the following error:\n\n```\n{}\n```\n\nWhat could be the cause and how can I fix it?",
            args.error
        )
    };

    let user_message = Message {
        role: Role::User,
        content: Some(prompt),
        tool_calls: None,
        tool_call_id: None,
    };

    let request = ChatCompletionRequest {
        model: config.api.big_model.clone(),
        messages: vec![user_message],
        stream: None,
        temperature: None,
        max_tokens: None,
        tools: None,
        tool_choice: None,
        source_map: None,
    };

    tracing::debug!("Sending debug request to API (streaming): {:?}", request);

    match api_client.chat_completion_stream(request).await {
        Ok(stream) => {
            tracing::debug!("Received debug stream from API.");
            handle_streamed_response(stream).await?;
        }
        Err(e) => {
            print_error(&format!("Error getting debugging assistance stream: {}", e));
        }
    }
    Ok(())
}
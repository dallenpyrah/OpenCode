use anyhow::{Context, Result};
use std::fs;

use crate::api::client::ApiClient;
use crate::api::models::{ChatCompletionRequest, Message, Role};
use crate::cli::commands::GenerateArgs;
use crate::config::Config;
use crate::streaming::handle_streamed_response;
use crate::tui::{print_error, print_warning};

pub async fn handle_generate(
    config: Config,
    args: GenerateArgs,
) -> Result<()> {
    let api_client = ApiClient::new(config.clone())
        .context("Failed to create API client (check API key configuration)")?;
    tracing::debug!(
        "Processing 'generate' command with description: '{}', file: {:?}",
        args.description,
        args.file
    );

    let file_content = match args.file {
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

    let prompt = if let Some(content) = file_content {
        format!(
            "Generate code based on the following description:\n{}\n\nUse this file content as context:\n```\n{}\n```",
            args.description, content
        )
    } else {
        format!(
            "Generate code based on the following description:\n{}",
            args.description
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
        stream: Some(true),
        temperature: None,
        max_tokens: None,
        tools: None,
        tool_choice: None,
        source_map: None,
    };

    tracing::debug!("Sending generation request to API (streaming): {:?}", request);

    match api_client.chat_completion_stream(request).await {
        Ok(stream) => {
            tracing::debug!("Received generation stream from API.");
            handle_streamed_response(stream).await?;
        }
        Err(e) => {
            print_error(&format!("Error generating code stream: {}", e));
        }
    }
    Ok(())
}
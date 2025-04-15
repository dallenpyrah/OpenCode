use anyhow::{Context, Result}; // Removed anyhow
use std::fs;

use crate::api::client::ApiClient;
use crate::api::models::{ChatCompletionRequest, Message, Role};
use crate::cli::commands::TestArgs;
use crate::config::Config;
use crate::streaming::handle_streamed_response;
use crate::tui::{print_error};

pub async fn handle_test(
    config: Config,
    args: TestArgs,
) -> Result<()> {
    let api_client = ApiClient::new(config.clone())
        .context("Failed to create API client (check API key configuration)")?;
    tracing::debug!(
        "Processing 'test' command for file: '{}'",
        args.file
    );

    let file_content = match fs::read_to_string(&args.file) {
        Ok(content) => {
            tracing::debug!("Successfully read file for test generation: {}", args.file);
            content
        }
        Err(e) => {
            print_error(&format!("Could not read file '{}': {}", args.file, e));
            tracing::error!("Failed to read file for test generation '{}': {}", args.file, e);
            return Err(anyhow::anyhow!("Failed to read file for test generation: {}", e));
        }
    };

    let prompt = format!(
        "Generate unit tests for the following code, using the appropriate testing framework for the language:\n\n```\n{}\n```",
        file_content
    );

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

    tracing::debug!("Sending test generation request to API (streaming): {:?}", request);

    match api_client.chat_completion_stream(request).await {
        Ok(stream) => {
            tracing::debug!("Received test generation stream from API.");
            handle_streamed_response(stream).await?;
        }
        Err(e) => {
            print_error(&format!("Error generating tests stream: {}", e));
        }
    }
    Ok(())
}
use anyhow::{Context, Result}; // Removed anyhow
use std::fs;

use crate::api::client::ApiClient;
use crate::api::models::{ChatCompletionRequest, Message, Role};
use crate::cli::commands::DocArgs;
use crate::config::Config;
use crate::streaming::handle_streamed_response;
use crate::tui::{print_error};

pub async fn handle_doc(
    config: Config,
    args: DocArgs,
) -> Result<()> {
    let api_client = ApiClient::new(config.clone())
        .context("Failed to create API client (check API key configuration)")?;
    tracing::debug!(
        "Processing 'doc' command for file: '{}'",
        args.file
    );

    let file_content = match fs::read_to_string(&args.file) {
        Ok(content) => {
            tracing::debug!("Successfully read file for doc generation: {}", args.file);
            content
        }
        Err(e) => {
            print_error(&format!("Could not read file '{}': {}", args.file, e));
            tracing::error!("Failed to read file for doc generation '{}': {}", args.file, e);
            return Err(anyhow::anyhow!("Failed to read file for doc generation: {}", e));
        }
    };

    let prompt = format!(
        "Generate documentation comments (e.g., Javadoc, Docstrings, Rustdoc) for the following code, following the conventions of the detected language:\n\n```\n{}\n```",
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

    tracing::debug!("Sending doc generation request to API (streaming): {:?}", request);

    match api_client.chat_completion_stream(request).await {
        Ok(stream) => {
            tracing::debug!("Received doc generation stream from API.");
            handle_streamed_response(stream).await?;
        }
        Err(e) => {
            print_error(&format!("Error generating documentation stream: {}", e));
        }
    }
    Ok(())
}
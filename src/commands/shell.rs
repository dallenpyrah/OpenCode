use anyhow::{Context, Result};

use crate::api::client::ApiClient;
use crate::api::models::{ChatCompletionRequest, Message, Role};
use crate::cli::commands::{ShellArgs, ShellCommands};
use crate::config::Config;
use crate::streaming::handle_streamed_response;
use crate::tui::{print_error};

pub async fn handle_shell(
    config: Config,
    args: ShellArgs,
) -> Result<()> {
    let api_client = ApiClient::new(config.clone())
        .context("Failed to create API client (check API key configuration)")?;
    match args.command {
        ShellCommands::Explain(explain_args) => {
            tracing::debug!(
                "Processing 'shell explain' command for command: '{}'",
                explain_args.command_string
            );

            let prompt = format!(
                "Explain the following shell command:\n\n```sh\n{}\n```",
                explain_args.command_string
            );

            let user_message = Message {
                role: Role::User,
                content: Some(prompt),
                tool_calls: None,
                tool_call_id: None,
            };

            let request = ChatCompletionRequest {
                model: config.api.default_model.clone(),
                messages: vec![user_message],
                stream: Some(true),
                temperature: None,
                max_tokens: None,
                tools: None,
                tool_choice: None,
                source_map: None,
            };

            tracing::debug!("Sending shell explanation request to API (streaming): {:?}", request);

            match api_client.chat_completion_stream(request).await {
                Ok(stream) => {
                    tracing::debug!("Received shell explanation stream from API.");
                    handle_streamed_response(stream).await?;
                }
                Err(e) => {
                    print_error(&format!("Error getting shell explanation stream: {}", e));
                }
            }
        }
        ShellCommands::Suggest(suggest_args) => {
            tracing::debug!(
                "Processing 'shell suggest' command for description: '{}'",
                suggest_args.description
            );

            let prompt = format!(
                "Suggest a shell command that does the following:\n\n{}",
                suggest_args.description
            );

            let user_message = Message {
                role: Role::User,
                content: Some(prompt),
                tool_calls: None,
                tool_call_id: None,
            };

            let request = ChatCompletionRequest {
                model: config.api.default_model.clone(),
                messages: vec![user_message],
                stream: Some(true),
                temperature: None,
                max_tokens: None,
                tools: None,
                tool_choice: None,
                source_map: None,
            };

            tracing::debug!("Sending shell suggestion request to API (streaming): {:?}", request);

            match api_client.chat_completion_stream(request).await {
                Ok(stream) => {
                    tracing::debug!("Received shell suggestion stream from API.");
                    handle_streamed_response(stream).await?;
                }
                Err(e) => {
                    print_error(&format!("Error getting shell suggestion stream: {}", e));
                }
            }
        }
    }
    Ok(())
}
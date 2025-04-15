use anyhow::{Context, Result}; // Removed anyhow
use std::fs;

use crate::api::client::ApiClient;
use crate::api::models::{ChatCompletionRequest, Message, Role};
use crate::cli::commands::ExplainArgs;
use crate::config::Config;
use crate::parsing::find_symbol_context;
use crate::streaming::handle_streamed_response;
use crate::tui::{print_error};

pub async fn handle_explain(
    config: Config,
    args: ExplainArgs,
) -> Result<()> {
    let api_client = ApiClient::new(config.clone())
        .context("Failed to create API client (check API key configuration)")?;
    tracing::debug!(
        "Processing 'explain' command for file: '{}', lines: {:?}, symbol: {:?}",
        args.file,
        args.lines,
        args.symbol
    );

    let code_context = if let Some(symbol_name) = &args.symbol {
        match find_symbol_context(&args.file, symbol_name) {
            Ok(context) => {
                tracing::debug!("Successfully found context for symbol '{}' in file '{}'", symbol_name, args.file);
                context
            }
            Err(e) => {
                print_error(&format!("Error finding symbol '{}': {}", symbol_name, e));
                tracing::error!("Error finding symbol '{}' in {}: {}", symbol_name, args.file, e);
                return Err(anyhow::anyhow!("Failed to find symbol context: {}", e));
            }
        }
    } else {
        let full_content = match fs::read_to_string(&args.file) {
            Ok(content) => {
                tracing::debug!("Successfully read file: {}", args.file);
                content
            }
            Err(e) => {
                print_error(&format!("Could not read file '{}': {}", args.file, e));
                tracing::error!("Failed to read file '{}': {}", args.file, e);
                return Err(anyhow::anyhow!("Failed to read file: {}", e));
            }
        };

        if let Some(lines_str) = &args.lines {
            match parse_lines(lines_str) {
                Ok((start_line, end_line)) => {
                    match extract_lines(&full_content, start_line, end_line) {
                        Ok(extracted) => extracted,
                        Err(e) => {
                            print_error(&format!("Error extracting lines: {}", e));
                            tracing::error!("Failed extracting lines '{}' from {}: {}", lines_str, args.file, e);
                            return Err(anyhow::anyhow!("Failed to extract lines: {}", e));
                        }
                    }
                }
                Err(e) => {
                    print_error(&format!("Invalid lines format '{}': {}", lines_str, e));
                    tracing::error!("Invalid lines format '{}': {}", lines_str, e);
                    return Err(anyhow::anyhow!("Invalid lines format: {}", e));
                }
            }
        } else {
            full_content
        }
    };

    let prompt = format!(
        "Explain the following code. Identify the programming language if possible:\n\n```\n{}\n```",
        code_context
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

    tracing::debug!("Sending explanation request to API (streaming): {:?}", request);

    match api_client.chat_completion_stream(request).await {
        Ok(stream) => {
            tracing::debug!("Received explanation stream from API.");
            handle_streamed_response(stream).await?;
        }
        Err(e) => {
            print_error(&format!("Error getting explanation stream: {}", e));
        }
    }
    Ok(())
}

fn parse_lines(lines_str: &str) -> Result<(usize, Option<usize>), String> {
    if lines_str.contains('-') {
        let parts: Vec<&str> = lines_str.splitn(2, '-').collect();
        if parts.len() == 2 {
            let start = parts[0].trim().parse::<usize>().map_err(|_| "Invalid start line number".to_string())?;
            let end = parts[1].trim().parse::<usize>().map_err(|_| "Invalid end line number".to_string())?;
            if start == 0 || end == 0 {
                Err("Line numbers must be 1 or greater".to_string())
            } else if start > end {
                Err("Start line cannot be greater than end line".to_string())
            } else {
                Ok((start, Some(end)))
            }
        } else {
            Err("Invalid range format".to_string())
        }
    } else {
        let start = lines_str.trim().parse::<usize>().map_err(|_| "Invalid line number".to_string())?;
         if start == 0 {
            Err("Line number must be 1 or greater".to_string())
        } else {
            Ok((start, None))
        }
    }
}

fn extract_lines(content: &str, start_line: usize, end_line: Option<usize>) -> Result<String, String> {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    if start_line == 0 {
        return Err("Start line cannot be 0 (lines are 1-based)".to_string());
    }

    if start_line > total_lines {
        return Err(format!("Start line {} is out of bounds (total lines: {})", start_line, total_lines));
    }

    let end = match end_line {
        Some(e) => {
            if e > total_lines {
                return Err(format!("End line {} is out of bounds (total lines: {})", e, total_lines));
            }
            e
        }
        None => start_line,
    };

    let start_index = start_line - 1;
    let end_index = end;

    if start_index >= end_index {
         return Err("Start index cannot be greater than or equal to end index after adjustment".to_string());
    }

    Ok(lines[start_index..end_index].join("\n"))
}
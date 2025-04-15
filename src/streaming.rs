use anyhow::Result;
use futures_util::stream::Stream;
use futures_util::StreamExt;
use std::io::{self, Write};
use std::pin::Pin;

use crate::api::models::ChatCompletionChunk;

pub async fn handle_streamed_response(
    mut stream: Pin<Box<dyn Stream<Item = Result<ChatCompletionChunk>> + Send>>,
) -> Result<String> {
    let mut buffer = String::new();
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                for choice in chunk.choices {
                    let mut printed_in_chunk = false;

                    if let Some(content_text) = choice.delta.content {
                        if !content_text.is_empty() {
                            print!("{}", content_text);
                            buffer.push_str(&content_text);
                            printed_in_chunk = true;
                        }
                    }

                    if printed_in_chunk {
                        io::stdout().flush()?;
                    }
                }
            }
            Err(e) => {
                tracing::error!("Error receiving stream chunk: {}", e);
                eprintln!("\nError during streaming: {}", e);
                return Err(e);
            }
        }
    }
    println!();
    io::stdout().flush()?;

    Ok(buffer)
}
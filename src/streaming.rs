use anyhow::Result;
use futures_util::stream::Stream;
use futures_util::StreamExt;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use iocraft::prelude::*;

use crate::api::models::ChatCompletionChunk;
use crate::tui::StreamingOutput;

pub async fn handle_streamed_response(
    mut stream: Pin<Box<dyn Stream<Item = Result<ChatCompletionChunk>> + Send>>,
) -> Result<()> {
    let (tx, rx) = mpsc::unbounded_channel::<Result<String, String>>();

    let stream_processor = tokio::spawn(async move {
        let mut accumulated_content = String::new();
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    let mut chunk_text = String::new();
                    for choice in chunk.choices {
                        if let Some(content_text) = choice.delta.content {
                            chunk_text.push_str(&content_text);
                        }
                    }
                    if !chunk_text.is_empty() {
                        accumulated_content.push_str(&chunk_text);
                        if tx.send(Ok(chunk_text)).is_err() {
                            tracing::warn!("Stream receiver dropped, stopping stream processing.");
                            return Err(anyhow::anyhow!("Stream receiver dropped"));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Error receiving stream chunk: {}", e);
                    let _ = tx.send(Err(e.to_string()));
                    return Err(e);
                }
            }
        }
        Ok(accumulated_content)
    });

    let wrapped_rx = Arc::new(Mutex::new(Some(rx)));
    
    element! { StreamingOutput(stream_rx: wrapped_rx) }
        .render_loop()
        .await
        .map_err(|e| anyhow::anyhow!("iocraft render loop failed: {}", e))?;

    match stream_processor.await {
        Ok(Ok(_content)) => {
            Ok(())
        }
        Ok(Err(e)) => {
            Err(e)
        }
        Err(e) => {
            Err(anyhow::anyhow!("Stream processing task failed: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;
    use crate::api::models::{ChatCompletionChunk, ChoiceDelta, Choice};
    use std::pin::Pin;
    use std::time::Duration;

    #[tokio::test]
    async fn test_handle_streamed_response_sends_data() {
        let chunk1 = ChatCompletionChunk {
            id: "1".to_string(), object: "chunk".to_string(), created: 0, model: "test".to_string(),
            choices: vec![Choice { index: 0, delta: ChoiceDelta { content: Some("Hello ".to_string()) }, finish_reason: None }],
        };
         let chunk2 = ChatCompletionChunk {
            id: "2".to_string(), object: "chunk".to_string(), created: 1, model: "test".to_string(),
            choices: vec![Choice { index: 0, delta: ChoiceDelta { content: Some("World!".to_string()) }, finish_reason: None }],
        };
        let s = stream::iter(vec![Ok(chunk1), Ok(chunk2)]);
        let stream: Pin<Box<dyn Stream<Item = Result<ChatCompletionChunk>> + Send>> = Box::pin(s);

        let (tx, mut rx) = mpsc::unbounded_channel::<Result<String, String>>();

        let processor_handle = tokio::spawn(async move {
            while let Some(chunk_result) = stream.next().await {
                 match chunk_result {
                    Ok(chunk) => {
                        let mut chunk_text = String::new();
                        for choice in chunk.choices {
                            if let Some(content_text) = choice.delta.content {
                                chunk_text.push_str(&content_text);
                            }
                        }
                         if !chunk_text.is_empty() {
                             if tx.send(Ok(chunk_text)).is_err() {
                                return Err(anyhow::anyhow!("Send failed"));
                             }
                         }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                        return Err(e);
                    }
                 }
            }
            Ok(())
        });

        let first_recv = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(first_recv.is_ok(), "Did not receive first chunk");
        assert_eq!(first_recv.unwrap().unwrap().unwrap(), "Hello ");

        let second_recv = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(second_recv.is_ok(), "Did not receive second chunk");
        assert_eq!(second_recv.unwrap().unwrap().unwrap(), "World!");

        let processor_result = tokio::time::timeout(Duration::from_millis(100), processor_handle).await;
        assert!(processor_result.is_ok(), "Processor task timed out");
        assert!(processor_result.unwrap().unwrap().is_ok(), "Processor task failed");
    }
}
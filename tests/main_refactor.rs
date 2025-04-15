use anyhow::{anyhow, Result};
use futures_util::stream::Stream;
use std::pin::Pin;
use tokio_stream::iter;

// Import types and functions from their new locations using crate paths
use opencode::api::models::{ChatCompletionChunk, ChunkChoice, Delta, Role};
use opencode::streaming::handle_streamed_response;
use opencode::commands::explain::{parse_lines, extract_lines};

fn create_test_chunk(content: Option<&str>, reasoning: Option<&str>, role: Option<Role>, finish_reason: Option<&str>) -> ChatCompletionChunk {
    ChatCompletionChunk {
        id: "test-id".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 0,
        model: "test-model".to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: Delta {
                role,
                content: content.map(String::from),
                reasoning: reasoning.map(String::from),
                tool_calls: None,
            },
            finish_reason: finish_reason.map(String::from),
        }],
        usage: None,
    }
}

#[tokio::test]
async fn test_handle_streamed_response() {
    let chunks = vec![
        Ok(create_test_chunk(None, None, Some(Role::Assistant), None)),
        Ok(create_test_chunk(Some("Hello"), None, None, None)),
        Ok(create_test_chunk(None, Some("Thinking..."), None, None)),
        Ok(create_test_chunk(Some(" world"), None, None, None)),
        Ok(create_test_chunk(Some("!"), Some("Done thinking"), None, None)),
        Ok(create_test_chunk(None, None, None, Some("stop"))),
    ];

    let stream = iter(chunks);
    // Ensure the stream item type matches the function signature
    let pinned_stream: Pin<Box<dyn Stream<Item = Result<ChatCompletionChunk>> + Send>> = Box::pin(stream);

    let result = handle_streamed_response(pinned_stream).await;

    assert!(result.is_ok());
    let accumulated_content = result.unwrap();
    assert_eq!(accumulated_content, "Hello world!");
}

#[tokio::test]
async fn test_handle_streamed_response_with_error() {
    let chunks = vec![
        Ok(create_test_chunk(Some("Part 1"), None, None, None)),
        Err(anyhow!("Simulated stream error")),
        Ok(create_test_chunk(Some("Part 2"), None, None, None)),
    ];

    let stream = iter(chunks);
    // Ensure the stream item type matches the function signature
    let pinned_stream: Pin<Box<dyn Stream<Item = Result<ChatCompletionChunk>> + Send>> = Box::pin(stream);

    let result = handle_streamed_response(pinned_stream).await;

    assert!(result.is_err());
    let error = result.err().unwrap();
    assert!(error.to_string().contains("Simulated stream error"));
}

#[test]
fn test_parse_lines_valid() {
    assert_eq!(parse_lines("10"), Ok((10, None)));
    assert_eq!(parse_lines(" 5 "), Ok((5, None)));
    assert_eq!(parse_lines("10-20"), Ok((10, Some(20))));
    assert_eq!(parse_lines(" 5 - 15 "), Ok((5, Some(15))));
}

#[test]
fn test_parse_lines_invalid() {
    assert!(parse_lines("abc").is_err());
    assert!(parse_lines("10-").is_err());
    assert!(parse_lines("-20").is_err());
    assert!(parse_lines("10-abc").is_err());
    assert!(parse_lines("abc-20").is_err());
    assert!(parse_lines("20-10").is_err());
    assert!(parse_lines("0").is_err());
    assert!(parse_lines("0-5").is_err());
    assert!(parse_lines("5-0").is_err());
}

#[test]
fn test_extract_lines_valid() {
    let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
    assert_eq!(extract_lines(content, 2, None), Ok("Line 2".to_string()));
    assert_eq!(extract_lines(content, 3, Some(3)), Ok("Line 3".to_string()));
    assert_eq!(extract_lines(content, 2, Some(4)), Ok("Line 2\nLine 3\nLine 4".to_string()));
    assert_eq!(extract_lines(content, 1, Some(5)), Ok(content.to_string()));
}

#[test]
fn test_extract_lines_invalid() {
    let content = "Line 1\nLine 2\nLine 3";
    assert!(extract_lines(content, 4, None).is_err());
    assert!(extract_lines(content, 1, Some(4)).is_err());
    assert!(extract_lines(content, 0, Some(1)).is_err());
}
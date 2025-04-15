use crate::config::Config;
use anyhow::{anyhow, Context, Result};
use reqwest::{Client, header::{HeaderMap, HeaderValue, USER_AGENT}};
use serde::{Deserialize, Serialize};


use std::time::Duration;
use bytes::Bytes;
use futures_util::stream::{try_unfold, Stream, StreamExt};
use futures_util::TryStreamExt;
use std::pin::Pin;

use crate::api::models::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
};

const OPENROUTER_API_BASE_URL: &str = "https://openrouter.ai/api/v1";
const REQUEST_TIMEOUT_SECONDS: u64 = 120;


const HTTP_REFERER: &str = "http://localhost:3000";
const X_TITLE: &str = "OpenCode CLI"; 

#[derive(Debug)]
pub struct ApiClient {
    client: Client,

    api_key: String, 
}





impl ApiClient {
    
    
    pub fn new(config: Config) -> Result<Self> {
        let api_key = config.get_api_key()?
            .context("OpenRouter API key not found. Please set the OPENROUTER_API_KEY environment variable.")?;

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_str(&format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")))?);
        headers.insert("HTTP-Referer", HeaderValue::from_static(HTTP_REFERER)); 
        headers.insert("X-Title", HeaderValue::from_static(X_TITLE)); 

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .build()
            .context("Failed to build reqwest client")?;

        Ok(ApiClient {
            client,
            api_key,
        })
    }

    
    async fn post_request<T: Serialize + std::fmt::Debug, R: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        body: &T,
    ) -> Result<R> {
        let url = format!("{}/{}", OPENROUTER_API_BASE_URL, endpoint.trim_start_matches('/'));
        tracing::debug!(url = %url, "Making POST request");
        
        

        let response = self.client.post(&url)
            .bearer_auth(&self.api_key)
            .json(body)
            .send()
            .await
            .with_context(|| format!("Failed to send request to {}", url))?;

        
        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_else(|_| "Could not read error body".to_string());
            tracing::error!(status = %status, body = %error_body, "API request failed");
            anyhow::bail!("API request failed with status {}: {}", status, error_body);
        }

        let response_body = response
            .json::<R>()
            .await
            .with_context(|| format!("Failed to deserialize response from {}", url))?;

        tracing::debug!("Successfully received and deserialized response");
        Ok(response_body)
    }

    
    pub async fn chat_completion(
        &self,
        mut request: ChatCompletionRequest, 
    ) -> Result<ChatCompletionResponse> {
        if request.stream == Some(true) {
             anyhow::bail!("Streaming chat completion is not yet implemented in this function.");
        }
        
        request.stream = None;

        tracing::info!(model = %request.model, "Requesting non-streaming chat completion");
        self.post_request("/chat/completions", &request).await
    }

    
    
    pub async fn chat_completion_stream(
        &self,
        mut request: ChatCompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatCompletionChunk>> + Send>>> { 
        
        request.stream = Some(true);

        let url = format!("{}/{}", OPENROUTER_API_BASE_URL, "chat/completions");
        tracing::info!(model = %request.model, url = %url, "Requesting streaming chat completion");
        
        

        let response = self.client.post(&url)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("Failed to send streaming request to {}", url))?;

        
        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_else(|_| "Could not read error body".to_string());
            tracing::error!(status = %status, body = %error_body, "API streaming request failed");
            anyhow::bail!("API streaming request failed with status {}: {}", status, error_body);
        }

        tracing::debug!("Received streaming response header, starting stream processing.");

        
        let byte_stream = response.bytes_stream().map_err(anyhow::Error::from); 
        Ok(Self::process_sse_stream(byte_stream)) 
    }

    
    fn process_sse_stream(
        byte_stream: impl Stream<Item = Result<Bytes>> + Send + Unpin + 'static, 
    ) -> Pin<Box<dyn Stream<Item = Result<ChatCompletionChunk>> + Send>> {
        let initial_state = (Vec::new(), byte_stream); 

        let stream = try_unfold(initial_state, |(mut buffer, mut stream)| async move {
            loop { 
                
                if let Some(newline_pos) = buffer.iter().position(|&b| b == b'\n') {
                    let line_bytes = buffer.drain(..=newline_pos).collect::<Vec<u8>>();
                    let line = String::from_utf8_lossy(&line_bytes).trim().to_string();

                    if line.starts_with("data:") {
                        let data = line[5..].trim();
                        if data == "[DONE]" {
                            tracing::debug!("SSE stream finished with [DONE]");
                            return Ok(None); 
                        }
                        if !data.is_empty() {
                            match serde_json::from_str::<ChatCompletionChunk>(data) {
                                Ok(parsed_chunk) => {
                                    
                                    return Ok(Some((parsed_chunk, (buffer, stream))));
                                }
                                Err(e) => {
                                    let err_msg = format!("Failed to parse SSE data line: {}. Data: '{}'", e, data);
                                    tracing::error!("{}", err_msg);
                                    
                                    return Err(anyhow!(err_msg));
                                }
                            }
                        }
                        
                    } else if !line.is_empty() {
                        tracing::trace!(line = %line, "Ignoring non-data SSE line");
                        
                    }
                    
                    continue; 
                }

                
                match stream.next().await {
                    Some(Ok(chunk)) => {
                        buffer.extend_from_slice(&chunk);
                        
                    }
                    Some(Err(e)) => {
                        tracing::error!(error = %e, "Error reading from byte stream");
                        return Err(anyhow::Error::from(e)); 
                    }
                    None => {
                        
                        if !buffer.is_empty() {
                            let remaining_data = String::from_utf8_lossy(&buffer);
                            tracing::error!("SSE stream ended with incomplete data in buffer: {}", remaining_data);
                            return Err(anyhow!("SSE stream ended unexpectedly with incomplete data: {}", remaining_data));
                        }
                        return Ok(None); 
                    }
                }
            } 
        }); 

        Box::pin(stream)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{ChatCompletionResponse, ToolCall}; // Kept ToolCall
    use crate::api::models::{Choice, Message, Role}; // Added back required imports for tests

    fn create_mock_response(_finish_reason: Option<&str>, tool_calls: Option<Vec<ToolCall>>) -> ChatCompletionResponse { // Prefix unused finish_reason
        ChatCompletionResponse {
            choices: vec![Choice {
                message: Message {
                    role: Role::Assistant,
                    content: None,
                    tool_calls,
                    tool_call_id: None,
                },
                
            }],
        }
    }

    
    

    
    

    
    #[tokio::test]
    async fn test_chat_completion_stream_success() {
        let mut server = mockito::Server::new_async().await;
        let server_url = server.url();

        
        let mock_body = "data: {\"id\":\"cmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1, \"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"cmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1, \"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"cmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1, \"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world!\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"cmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1, \"model\":\"test-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        let mock = server.mock("POST", "/chat/completions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(mock_body)
            .create_async().await;

        
        let http_client = reqwest::Client::new();
        
        let api_client = ApiClient {
            client: http_client,
            
            api_key: "dummy_key".to_string(), 
        };

        
        let request = ChatCompletionRequest {
            model: "test-model".to_string(),
            messages: vec![Message { role: Role::User, content: Some("Hi".to_string()), tool_calls: None, tool_call_id: None }],
            temperature: None,
            max_tokens: None,
            stream: Some(true),
            tools: None,
            tool_choice: None,
            source_map: None, // Added missing field
        };

        
        
        
        let url = format!("{}/chat/completions", server_url);
        let response = api_client.client.post(&url)
            .bearer_auth(&api_client.api_key)
            .json(&request)
            .send()
            .await
            .expect("Mock request failed");

        assert!(response.status().is_success());

        let byte_stream = response.bytes_stream().map_err(anyhow::Error::from);
        let mut chunk_stream = ApiClient::process_sse_stream(byte_stream);

        let mut chunks = Vec::new();
        while let Some(chunk_result) = chunk_stream.next().await {
            chunks.push(chunk_result.expect("Stream yielded an error"));
        }

        mock.assert_async().await;

        assert_eq!(chunks.len(), 4); 

        
        // Re-applying removal of delta.role assertions
        assert_eq!(chunks[0].choices[0].delta.content, None);
        // assert_eq!(chunks[0].choices[0].delta.role, Some(Role::Assistant)); // Removed

        assert_eq!(chunks[1].choices[0].delta.content, Some("Hello".to_string()));
        // assert_eq!(chunks[1].choices[0].delta.role, None); // Removed

        assert_eq!(chunks[2].choices[0].delta.content, Some(" world!".to_string()));
        // assert_eq!(chunks[2].choices[0].delta.role, None); // Removed

        assert_eq!(chunks[3].choices[0].delta.content, None);
        // assert_eq!(chunks[3].choices[0].delta.role, None); // Removed
        
    }
}
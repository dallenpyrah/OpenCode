use crate::config::Config;
use anyhow::{anyhow, Context, Result};
use reqwest::{Client, header::{HeaderMap, HeaderValue, USER_AGENT}};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use std::time::Duration;
use bytes::Bytes;
use futures_util::stream::{try_unfold, Stream, StreamExt};
use futures_util::TryStreamExt; 
use std::pin::Pin;

const OPENROUTER_API_BASE_URL: &str = "https:
const REQUEST_TIMEOUT_SECONDS: u64 = 120; 


const HTTP_REFERER: &str = "http:
const X_TITLE: &str = "OpenCode CLI"; 

#[derive(Debug)]
pub struct ApiClient {
    client: Client,

    api_key: String, 
}



#[derive(Serialize, Debug, Clone)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
#[serde(rename_all = "lowercase")]
pub enum Role {
    System, 
    User,
    Assistant,
    Tool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: Option<String>, 
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>, 
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>, 
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String, 
    pub function: FunctionDefinition,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value, 
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)] 
pub enum ToolChoice {
    None,
    Auto,
    Tool {
        #[serde(rename = "type")]
        tool_type: String, 
        function: ToolChoiceFunction,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolChoiceFunction {
    pub name: String,
}



#[derive(Deserialize, Debug, Clone)] 
pub struct ChatCompletionResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Choice {
    pub message: Message, 
    
}

#[derive(Serialize, Deserialize, Debug, Clone)] 
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String, 
    pub function: ToolCallFunction,
}

#[derive(Serialize, Deserialize, Debug, Clone)] 
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String, 
}

#[derive(Deserialize, Debug, Clone, Default)] 
pub struct UsageStats {
}




#[derive(Deserialize, Debug, Clone)] 
pub struct ChatCompletionChunk {
    pub choices: Vec<ChunkChoice>,
}

#[derive(Deserialize, Debug, Clone)] 
pub struct ChunkChoice {
    pub delta: Delta, 
    
}

#[derive(Deserialize, Debug, Clone)] 
pub struct Delta {
    #[serde(default)]
    pub content: Option<String>, 
    
}


#[derive(Deserialize, Debug, Clone)] 
pub struct ToolCallChunk {
}

#[derive(Deserialize, Debug, Clone)] 
pub struct ToolCallFunctionChunk {
}





impl ApiClient {
    
    
    pub fn new(config: Config) -> Result<Self> {
        let api_key = config.get_api_key()?
            .context("OpenRouter API key not found in keyring. Please set it using 'opencode configure --set-api-key'.")?;

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

    
    use crate::tools::ToolRegistry;
    use crate::tools::tests::DummyTool; 
    use serde_json::json;
    
    

    

    fn create_mock_response(finish_reason: Option<&str>, tool_calls: Option<Vec<ToolCall>>) -> ChatCompletionResponse {
        ChatCompletionResponse {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion".to_string(),
            created: 1677652288,
            model: "gpt-4".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: Role::Assistant,
                    content: None,
                    tool_calls,
                    tool_call_id: None,
                },
                finish_reason: finish_reason.map(String::from),
            }],
            usage: None,
            stop_reason: None,
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

        
        assert_eq!(chunks[0].choices[0].delta.role, Some(Role::Assistant));
        assert_eq!(chunks[0].choices[0].delta.content, None);

        assert_eq!(chunks[1].choices[0].delta.role, None);
        assert_eq!(chunks[1].choices[0].delta.content, Some("Hello".to_string()));

        assert_eq!(chunks[2].choices[0].delta.role, None);
        assert_eq!(chunks[2].choices[0].delta.content, Some(" world!".to_string()));

        assert_eq!(chunks[3].choices[0].delta.role, None);
        assert_eq!(chunks[3].choices[0].delta.content, None);
        assert_eq!(chunks[3].choices[0].finish_reason, Some("stop".to_string()));
    }
}

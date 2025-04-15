use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use serde_json::Value; // Needed for CliTool trait
use std::env; // Needed for reading environment variable

use super::{CliTool, ToolError}; // Correct trait and error type

#[derive(Debug, Serialize, Deserialize)]
pub struct WebSearchInput {
    pub query: String,
    pub num_results: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub link: String,
    pub snippet: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebSearchOutput {
    pub results: Vec<SearchResult>,
}

#[derive(Error, Debug)]
pub enum WebSearchError {
    #[error("Missing API key for Brave Search. Please set BRAVE_SEARCH_API_KEY environment variable.")]
    MissingApiKey,
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Failed to parse API response: {0}")]
    ParseError(#[from] serde_json::Error),
    // ConfigError variant removed as it's unused
}

impl From<WebSearchError> for ToolError {
    fn from(error: WebSearchError) -> Self {
        // Convert specific WebSearchError to generic ToolError
        match error {
            WebSearchError::MissingApiKey => ToolError::Other { message: error.to_string() },
            WebSearchError::NetworkError(e) => ToolError::NetworkError { source: anyhow::anyhow!(e) },
            WebSearchError::ApiError(msg) => ToolError::Other { message: format!("API Error: {}", msg) },
            WebSearchError::ParseError(e) => ToolError::Other { message: format!("Response Parse Error: {}", e) },
            // ConfigError match arm removed
        }
    }
}

#[derive(Debug)] // Added Debug derive
pub struct WebSearchTool;

#[async_trait]
impl CliTool for WebSearchTool {
    fn name(&self) -> String {
        "web_search".to_string()
    }

    fn description(&self) -> String {
        "Searches the web for a given query using the Brave Search API. \
         Requires BRAVE_SEARCH_API_KEY environment variable to be set. \
         Args: {\"query\": string, \"num_results\": integer (optional, default 5)}"
            .to_string()
    }

    fn parameters_schema(&self) -> anyhow::Result<Value> { // Use anyhow::Result
        Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query."
                },
                "num_results": {
                    "type": "integer",
                    "description": "The maximum number of results to return (default: 5)."
                }
            },
            "required": ["query"]
        }))
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: WebSearchInput = serde_json::from_value(args).map_err(|e| {
            ToolError::InvalidArguments {
                tool_name: self.name(),
                details: format!("Failed to parse arguments: {}", e),
            }
        })?;

        let api_key = env::var("BRAVE_SEARCH_API_KEY")
            .map_err(|_| WebSearchError::MissingApiKey)?;

        if api_key.is_empty() {
             return Err(WebSearchError::MissingApiKey.into());
        }

        let client = reqwest::Client::new();
        let num_results = input.num_results.unwrap_or(5);

        // Brave Search API response structure (kept internal to execute)
        #[derive(Deserialize)]
        struct BraveApiResponse {
            web: Option<BraveWebResults>,
        }
        #[derive(Deserialize)]
        struct BraveWebResults {
            results: Option<Vec<BraveSearchResult>>,
        }
        #[derive(Deserialize)]
        struct BraveSearchResult {
            title: Option<String>,
            url: Option<String>,
            description: Option<String>,
        }

        let response = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("Accept", "application/json")
            .header("X-Subscription-Token", &api_key) // Pass reference
            .query(&[("q", &input.query), ("count", &num_results.to_string())])
            .send()
            .await
            .map_err(WebSearchError::NetworkError)?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_else(|_| "Failed to read error body".to_string());
            return Err(WebSearchError::ApiError(format!(
                "API request failed with status {}: {}",
                status, text
            )).into());
        }

        let api_response: BraveApiResponse = response
            .json()
            .await
            .map_err(WebSearchError::NetworkError)?; // Use NetworkError for reqwest errors

        let results = api_response
            .web
            .and_then(|w| w.results)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| {
                Some(SearchResult {
                    title: r.title?,
                    link: r.url?,
                    snippet: r.description?,
                })
            })
            .collect();

        let output = WebSearchOutput { results };
        serde_json::to_value(output).map_err(|e| ToolError::Other {
            message: format!("Failed to serialize output: {}", e),
        })
    }
}
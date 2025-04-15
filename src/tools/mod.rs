pub mod registry;
pub mod tool_result_format;
use crate::config::UserToolConfig;
pub mod execution;
use async_trait::async_trait;
use anyhow::{Context, Result}; 
use rust_search::SearchBuilder;
use thiserror::Error;
use serde_json::Value;
use tracing;
use std::process::Command;
use std::env;
use std::path::PathBuf;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("Invalid arguments for tool '{tool_name}': {details}")]
    InvalidArguments { tool_name: String, details: String },

    #[error("Execution failed for command '{command}': {stderr}")]
    ExecutionFailed { command: String, stderr: String },

    #[error("File not found at path: {path}")]
    FileNotFound { path: String },

    #[error("Permission denied for resource: {resource}")]
    PermissionDenied { resource: String },

    #[error("Network error: {source}")]
    NetworkError {
        #[from]
        source: anyhow::Error,
    },

    #[error("An unexpected error occurred: {message}")]
    Other { message: String },
}

#[derive(Debug)]
pub struct FileReadTool;

#[derive(Debug)]
pub struct FileWriteTool;

#[derive(Debug)]
pub struct ShellCommandTool;

#[derive(Debug)]
pub struct GitTool;

#[derive(Debug)]
pub struct WebSearchTool;

#[derive(Debug)]
pub struct CodeSearchTool;

#[derive(Debug)]
pub struct FileSearchTool;

#[derive(Debug)]
pub struct UserDefinedTool {
    name: String,
    description: String,
    input_schema_val: Value, 
    compiled_schema: jsonschema::Validator, 
    command_template: String,
}

impl UserDefinedTool {
    
    
    pub fn new(config: &UserToolConfig) -> Result<Self> {
        let input_schema_val: Value = serde_json::from_str(&config.input_schema)
            .with_context(|| format!("Failed to parse input_schema JSON for tool '{}'", config.name))?;

        
        let compiled_schema = jsonschema::validator_for(&input_schema_val)
            .with_context(|| format!("Failed to compile input_schema for tool '{}'", config.name))?;

        Ok(UserDefinedTool {
            name: config.name.clone(),
            description: config.description.clone(),
            input_schema_val,
            compiled_schema,
            command_template: config.command_template.clone(),
        })
    }
}

#[async_trait]
impl CliTool for UserDefinedTool {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn description(&self) -> String {
        self.description.clone()
    }

    fn parameters_schema(&self) -> Result<Value> {
        
        Ok(self.input_schema_val.clone())
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        
        let errors: Vec<String> = self.compiled_schema
            .iter_errors(&args)
            .map(|e| format!("{}", e)) 
            .collect();

        if !errors.is_empty() {
            let error_details = errors.join("; ");
            return Err(ToolError::InvalidArguments {
                tool_name: self.name(),
                details: format!("Schema validation failed: {}", error_details),
            });
        }

        
        let mut command_string = self.command_template.clone();
        if let Value::Object(map) = args {
            for (key, value) in map {
                let placeholder = format!("{{{}}}", key);
                
                
                
                let value_str = match value {
                    Value::String(s) => s,
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    
                    _ => return Err(ToolError::InvalidArguments {
                        tool_name: self.name(),
                        details: format!("Unsupported argument type for key '{}'", key),
                    }),
                };
                command_string = command_string.replace(&placeholder, &value_str);
            }
        } else if !args.is_null() {
             return Err(ToolError::InvalidArguments {
                tool_name: self.name(),
                details: "Expected arguments to be a JSON object".to_string(),
            });
        }

        
        
        
        
        tracing::info!("Executing user tool '{}' command: {}", self.name, command_string);
        let output = Command::new("sh")
            .arg("-c")
            .arg(&command_string) 
            .output()
            .map_err(|e| ToolError::Other {
                message: format!("Failed to execute command for tool '{}': {}", self.name, e),
            })?;

        
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(Value::String(stdout)) 
        } else {
            tracing::error!("User tool '{}' failed. Stderr: {}", self.name, stderr);
            Err(ToolError::ExecutionFailed {
                command: command_string, 
                stderr,
            })
        }
    }
}

#[async_trait]
impl CliTool for CodeSearchTool {
    fn name(&self) -> String {
        "CodeSearchTool".to_string()
    }
    fn description(&self) -> String {
        "Searches for a pattern in code using ripgrep (rg). Args: {\"pattern\": string, \"path\": string (optional)}".to_string()
    }
    fn parameters_schema(&self) -> Result<Value> {
        Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string" }
            },
            "required": ["pattern"]
        }))
    }
    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let pattern = args.get("pattern").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidArguments {
            tool_name: self.name(),
            details: "Missing or invalid 'pattern' argument".to_string(),
        })?;
        let search_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let output = std::process::Command::new("rg")
            .arg(pattern)
            .arg(search_path)
            .output()
            .map_err(|e| ToolError::Other { message: format!("Failed to run ripgrep: {}", e) })?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let code = output.status.code().unwrap_or(-1);
        if !output.status.success() && !stdout.is_empty() {
            return Err(ToolError::ExecutionFailed { command: format!("rg {} {}", pattern, search_path), stderr });
        }
        Ok(serde_json::json!({ "stdout": stdout, "exit_code": code }))
    }
}

#[async_trait]
impl CliTool for WebSearchTool {
    fn name(&self) -> String {
        "WebSearchTool".to_string()
    }
    fn description(&self) -> String {
        "Fetches the contents of a URL. Args: {\"url\": string}".to_string()
    }
    fn parameters_schema(&self) -> Result<Value> {
        Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" }
            },
            "required": ["url"]
        }))
    }
    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let url = args.get("url").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidArguments {
            tool_name: self.name(),
            details: "Missing or invalid 'url' argument".to_string(),
        })?;
        let resp = reqwest::get(url).await.map_err(|e| ToolError::NetworkError { source: anyhow::anyhow!(e) })?;
        let status = resp.status().as_u16();
        let content = resp.text().await.map_err(|e| ToolError::NetworkError { source: anyhow::anyhow!(e) })?;
        Ok(serde_json::json!({ "status": status, "content": content }))
    }
}

#[async_trait]
impl CliTool for GitTool {
    fn name(&self) -> String {
        "GitTool".to_string()
    }
    fn description(&self) -> String {
        "Runs git operations. Args: {\"operation\": string, \"args\": object (optional)}".to_string()
    }
    fn parameters_schema(&self) -> Result<Value> {
        Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "operation": { "type": "string", "enum": ["status", "commit", "add", "push"] },
                "args": { "type": "object" }
            },
            "required": ["operation"]
        }))
    }
    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let operation = args.get("operation").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidArguments {
            tool_name: self.name(),
            details: "Missing or invalid 'operation' argument".to_string(),
        })?;
        match operation {
            "status" => {
                let output = std::process::Command::new("git")
                    .arg("status")
                    .output()
                    .map_err(|e| ToolError::Other { message: format!("Failed to run git status: {}", e) })?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let code = output.status.code().unwrap_or(-1);
                if !output.status.success() {
                    return Err(ToolError::ExecutionFailed { command: "git status".to_string(), stderr });
                }
                Ok(serde_json::json!({ "stdout": stdout, "exit_code": code }))
            },
            _ => Err(ToolError::InvalidArguments {
                tool_name: self.name(),
                details: format!("Unsupported git operation: {}", operation)
            })
        }
    }
}

#[async_trait]
impl CliTool for ShellCommandTool {
    fn name(&self) -> String {
        "ShellCommandTool".to_string()
    }
    fn description(&self) -> String {
        "Executes a shell command. Args: {\"command\": string, \"args\": [string] (optional)}".to_string()
    }
    fn parameters_schema(&self) -> Result<Value> {
        Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" },
                "args": { "type": "array", "items": { "type": "string" }, "default": [] }
            },
            "required": ["command"]
        }))
    }
    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let command = args.get("command").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidArguments {
            tool_name: self.name(),
            details: "Missing or invalid 'command' argument".to_string(),
        })?;
        let arg_list: Vec<String> = args.get("args")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_else(Vec::new);
        let output = std::process::Command::new(command)
            .args(&arg_list)
            .output()
            .map_err(|e| ToolError::Other { message: format!("Failed to execute command: {}", e) })?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let code = output.status.code().unwrap_or(-1);
        if !output.status.success() {
            return Err(ToolError::ExecutionFailed { command: command.to_string(), stderr });
        }
        Ok(serde_json::json!({ "stdout": stdout, "exit_code": code }))
    }
}

#[async_trait]
impl CliTool for FileWriteTool {
    fn name(&self) -> String {
        "FileWriteTool".to_string()
    }
    fn description(&self) -> String {
        "Writes content to a file. Args: {\"path\": string, \"content\": string}".to_string()
    }
    fn parameters_schema(&self) -> Result<Value> {
        Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        }))
    }
    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidArguments {
            tool_name: self.name(),
            details: "Missing or invalid 'path' argument".to_string(),
        })?;
        let content = args.get("content").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidArguments {
            tool_name: self.name(),
            details: "Missing or invalid 'content' argument".to_string(),
        })?;
        std::fs::write(path, content).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                ToolError::PermissionDenied { resource: path.to_string() }
            } else {
                ToolError::Other { message: format!("Failed to write file: {}", e) }
            }
        })?;
        Ok(serde_json::json!({ "status": "success" }))
    }
}

#[async_trait]
impl CliTool for FileReadTool {
    fn name(&self) -> String {
        "FileReadTool".to_string()
    }
    fn description(&self) -> String {
        "Reads a file from the file system. Args: {\"path\": string}".to_string()
    }
    fn parameters_schema(&self) -> Result<Value> {
        Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        }))
    }
    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidArguments {
            tool_name: self.name(),
            details: "Missing or invalid 'path' argument".to_string(),
        })?;
        let content = std::fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ToolError::FileNotFound { path: path.to_string() }
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                ToolError::PermissionDenied { resource: path.to_string() }
            } else {
                ToolError::Other { message: format!("Failed to read file: {}", e) }
            }
        })?;
        Ok(serde_json::json!({ "content": content }))
    }
}

#[async_trait]
impl CliTool for FileSearchTool {
    fn name(&self) -> String {
        "FileSearchTool".to_string()
    }

    fn description(&self) -> String {
        "Searches the project workspace for files with advanced filtering options. Args: {\"query\": string, \"extension\": string (optional), \"case_sensitive\": boolean (optional), \"include_hidden\": boolean (optional), \"max_results\": number (optional)}".to_string()
    }

    fn parameters_schema(&self) -> Result<Value> {
        Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "query": { 
                    "type": "string", 
                    "description": "Filename or partial path to search for."
                },
                "extension": { 
                    "type": "string", 
                    "description": "Filter by file extension (e.g., 'rs', 'json', 'md')."
                },
                "case_sensitive": { 
                    "type": "boolean", 
                    "description": "Whether the search should be case sensitive (default: false)."
                },
                "include_hidden": { 
                    "type": "boolean", 
                    "description": "Whether to include hidden files/directories in the search (default: false)."
                },
                "max_results": { 
                    "type": "integer", 
                    "description": "Maximum number of results to return (default: 20)."
                }
            },
            "required": ["query"]
        }))
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| ToolError::InvalidArguments {
            tool_name: self.name(),
            details: "Missing or invalid 'query' argument".to_string(),
        })?;
        
        // Optional parameters with defaults
        let extension = args.get("extension").and_then(|v| v.as_str()).map(|s| s.trim_start_matches('.').to_string());
        let case_sensitive = args.get("case_sensitive").and_then(|v| v.as_bool()).unwrap_or(false);
        let include_hidden = args.get("include_hidden").and_then(|v| v.as_bool()).unwrap_or(false);
        let max_results = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
        
        // Clone extension for later use in JSON response
        let extension_for_response = extension.clone();

        tracing::debug!(
            tool_name = self.name(), 
            query = query, 
            extension = ?extension, 
            case_sensitive = case_sensitive, 
            include_hidden = include_hidden, 
            max_results = max_results,
            "Executing enhanced FileSearchTool"
        );

        // Get current working directory
        let current_dir = env::current_dir().map_err(|e| ToolError::Other { 
            message: format!("Failed to get current directory: {}", e) 
        })?;

        let current_dir_str = current_dir.to_string_lossy().to_string();
        tracing::debug!(tool_name = self.name(), location = current_dir_str, "Search location");

        // Build search with all options
        let mut builder = SearchBuilder::default()
            .location(&current_dir_str)
            .search_input(query)
            .limit(max_results);

        // Apply optional filters
        if !case_sensitive {
            builder = builder.ignore_case();
        }
        
        if let Some(ref ext) = extension {
            builder = builder.ext(ext);
        }
        
        // Execute search and collect results
        let search_results: Vec<String> = builder.build().collect();
        
        // Process results to get relative paths
        let mut found_files = Vec::new();
        for result_path in search_results {
            let path = PathBuf::from(&result_path);
            if let Ok(relative) = path.strip_prefix(&current_dir) {
                if let Some(path_str) = relative.to_str() {
                    // Skip hidden files/folders if requested
                    if !include_hidden && path_str.split('/').any(|part| part.starts_with('.')) {
                        tracing::debug!(tool_name = self.name(), path = path_str, "Skipping hidden file/directory");
                        continue;
                    }
                    
                    found_files.push(path_str.to_string());
                    tracing::debug!(tool_name = self.name(), matched_file = path_str, "Adding matched file");
                }
            }
        }

        tracing::debug!(
            tool_name = self.name(), 
            query = query, 
            found_count = found_files.len(), 
            "Search complete"
        );

        // Additional context for empty results
        if found_files.is_empty() {
            tracing::debug!(tool_name = self.name(), "No files found, adding search suggestions");
            
            Ok(serde_json::json!({
                "found_files": found_files,
                "search_info": {
                    "query": query,
                    "extension": extension_for_response,
                    "case_sensitive": case_sensitive,
                    "include_hidden": include_hidden,
                    "max_results": max_results
                },
                "suggestions": [
                    "Try a more general search term",
                    "Try with include_hidden: true to search hidden directories",
                    "Try removing the extension filter",
                    "Try with case_sensitive: false (if not already)"
                ]
            }))
        } else {
            Ok(serde_json::json!({
                "found_files": found_files,
                "search_info": {
                    "query": query,
                    "extension": extension_for_response,
                    "case_sensitive": case_sensitive,
                    "include_hidden": include_hidden,
                    "max_results": max_results
                }
            }))
        }
    }
}

#[async_trait]
pub trait CliTool: Send + Sync + std::fmt::Debug {
    
    fn name(&self) -> String;

    
    fn description(&self) -> String;

    
    fn parameters_schema(&self) -> Result<Value>;

    
    
    async fn execute(&self, args: Value) -> Result<Value, ToolError>;
}
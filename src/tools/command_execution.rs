use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value; // Needed for CliTool trait
use std::process::Command;
use std::path::PathBuf;

use super::{CliTool, ToolError}; // Correct trait and error type

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteCommandInput {
    pub command: String,
    pub working_directory: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecuteCommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub struct ExecuteCommandTool;

#[async_trait]
impl CliTool for ExecuteCommandTool {
    fn name(&self) -> String {
        "execute_command".to_string()
    }

    fn description(&self) -> String {
        "Executes a shell command and captures its output. \
         Args: {\"command\": string, \"working_directory\": string (optional)}"
            .to_string()
    }

    fn parameters_schema(&self) -> anyhow::Result<Value> { // Use anyhow::Result
        Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The command line to execute."
                },
                "working_directory": {
                    "type": "string",
                    "description": "The directory to execute the command in. Defaults to the current workspace directory."
                }
            },
            "required": ["command"]
        }))
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: ExecuteCommandInput = serde_json::from_value(args).map_err(|e| {
            ToolError::InvalidArguments {
                tool_name: self.name(),
                details: format!("Failed to parse arguments: {}", e),
            }
        })?;

        let (shell, shell_arg) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        let mut command_builder = Command::new(shell);
        command_builder.arg(shell_arg).arg(&input.command);

        let current_dir = match &input.working_directory {
            Some(dir) => PathBuf::from(dir),
            None => std::env::current_dir().map_err(|e| ToolError::Other {
                message: format!("Failed to get current directory: {}", e),
            })?,
        };

        command_builder.current_dir(&current_dir);

        let output = command_builder.output().map_err(|e| ToolError::Other {
            message: format!("Failed to spawn command '{}': {}", input.command, e),
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code();

        let result = ExecuteCommandOutput {
            exit_code,
            stdout,
            stderr,
        };

        // Even if the command fails (non-zero exit code), we return the output
        serde_json::to_value(result).map_err(|e| ToolError::Other {
            message: format!("Failed to serialize output: {}", e),
        })
    }
}

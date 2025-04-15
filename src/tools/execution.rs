use crate::tools::ToolError;
use serde_json::Value;
use anyhow::Result;

#[derive(Debug)]
pub enum SecurityPolicy {
    #[allow(dead_code)]
    AllowAll,
    ConfirmWrites,
}

#[derive(Debug)]
pub struct ToolExecutionEngine<'a> {
    tool_registry: &'a crate::tools::registry::ToolRegistry,
    security_policy: SecurityPolicy,
}

impl<'a> ToolExecutionEngine<'a> {
    pub fn new(tool_registry: &'a crate::tools::registry::ToolRegistry, security_policy: SecurityPolicy) -> Self {
        ToolExecutionEngine {
            tool_registry,
            security_policy,
        }
    }

    pub async fn execute_tool_call(&self, tool_name: &str, arguments: Value) -> Result<Value, ToolError> {
        tracing::info!("Attempting to execute tool '{}' with arguments: {:?}", tool_name, arguments);
        if let Some(tool) = self.tool_registry.get_tool(tool_name) {
            match self.security_policy {
                SecurityPolicy::AllowAll => {
                    tracing::debug!("Executing tool '{}' under AllowAll security policy.", tool_name);
                    tool.execute(arguments).await
                }
                SecurityPolicy::ConfirmWrites => {
                    
                    if tool_name == "FileWriteTool" {
                        tracing::warn!("FileWriteTool execution requires confirmation but is currently auto-approved.");
                    }
                    tracing::debug!("Executing tool '{}' under ConfirmWrites security policy (auto-approved).", tool_name);
                    tool.execute(arguments).await
                }
            }
        } else {
            tracing::warn!("Tool '{}' not found in registry.", tool_name);
            Err(ToolError::Other { message: format!("Tool '{}' not found", tool_name) })
        }
    }
}
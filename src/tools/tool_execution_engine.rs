use crate::tools::ToolRegistry; 
use serde_json::Value;

pub enum SecurityPolicy {
    ConfirmWrites,
}

pub struct ToolExecutionEngine<'a> {
    pub registry: &'a ToolRegistry,
    pub policy: SecurityPolicy,
}

impl<'a> ToolExecutionEngine<'a> {
    pub fn new(registry: &'a ToolRegistry, policy: SecurityPolicy) -> Self {
        Self { registry, policy }
    }

    pub fn needs_confirmation(&self, tool_name: &str) -> bool {
        match self.policy {
            SecurityPolicy::ConfirmWrites => {
                
                tool_name == "FileWriteTool" || tool_name == "ShellCommandTool" || tool_name == "GitTool"
            }
        }
    }

    pub async fn execute_tool_call(&self, tool_name: &str, args: Value) -> Value {
        let tool = match self.registry.get_tool(tool_name) {
            Some(t) => t,
            None => {
                return tool_result_format::format_tool_result(tool_name, &serde_json::Value::Null, Some(&format!("Tool '{}' not found", tool_name)));
            }
        };
        if self.needs_confirmation(tool_name) {
            let prompt = format!("Allow {} with args: {}?", tool_name, args);
            if let Ok(false) = crate::tui::prompt_confirmation(&prompt) {
                return tool_result_format::format_tool_result(tool_name, &serde_json::Value::Null, Some("Execution denied by user"));
            }
        }
        match tool.execute(args).await {
            Ok(result) => tool_result_format::format_tool_result(tool_name, &result, None),
            Err(e) => tool_result_format::format_tool_result(tool_name, &serde_json::Value::Null, Some(&e.to_string())),
        }
    }
}

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

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

#[async_trait]
pub trait CliTool: Send + Sync + std::fmt::Debug {
    /// The unique name of the tool (used in API calls).
    fn name(&self) -> String;

    /// A description of what the tool does (for the LLM).
    fn description(&self) -> String;

    /// Returns the JSON schema for the tool's input parameters.
    fn parameters_schema(&self) -> Result<Value>;

    /// Executes the tool with the given arguments (parsed JSON).
    /// Returns the result as a JSON value.
    async fn execute(&self, args: Value) -> Result<Value, ToolError>;
}

use std::collections::HashMap;

/// Manages the collection of available CLI tools.
#[derive(Debug, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn CliTool + Send + Sync>>,
}

impl ToolRegistry {
    /// Creates a new, empty tool registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new tool with the registry.
    ///
    /// The tool is stored using its name as the key.
    pub fn register(&mut self, tool: Box<dyn CliTool + Send + Sync>) {
        self.tools.insert(tool.name(), tool);
    }

    /// Retrieves the JSON schemas for all registered tools.
    pub fn get_tool_schemas(&self) -> Result<Vec<Value>> {
        self.tools
            .values()
            .map(|tool| tool.parameters_schema())
            .collect()
    }

    /// Retrieves a reference to a tool by its name.
    ///
    /// Returns `None` if no tool with the given name is registered.
    pub fn get_tool(&self, name: &str) -> Option<&(dyn CliTool + Send + Sync)> {
        self.tools.get(name).map(|boxed_tool| boxed_tool.as_ref())
    }

}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug)]
    pub struct DummyTool {
        name: String,
        description: String,
        schema: Value,
    }

    impl DummyTool {
        pub fn new(name: &str, description: &str, schema: Value) -> Self {
            Self {
                name: name.to_string(),
                description: description.to_string(),
                schema,
            }
        }
    }

    #[async_trait]
    impl CliTool for DummyTool {
        fn name(&self) -> String {
            self.name.clone()
        }

        fn description(&self) -> String {
            self.description.clone()
        }

        fn parameters_schema(&self) -> Result<Value> {
            Ok(self.schema.clone())
        }

        async fn execute(&self, _args: Value) -> Result<Value, ToolError> {
            Ok(json!({ "status": "dummy execution successful" }))
        }
    }

    #[test]
    fn test_tool_registry_new() {
        let registry = ToolRegistry::new();
        assert!(registry.tools.is_empty());
    }

    #[test]
    fn test_tool_registry_register_and_get() {
        let mut registry = ToolRegistry::new();
        let dummy_tool = Box::new(DummyTool::new("dummy", "A test tool", json!({ "type": "object" })));
        let tool_name = dummy_tool.name();

        registry.register(dummy_tool);

        assert_eq!(registry.tools.len(), 1);
        let retrieved_tool = registry.get_tool(&tool_name);
        assert!(retrieved_tool.is_some());
        assert_eq!(retrieved_tool.unwrap().name(), tool_name);
        assert_eq!(retrieved_tool.unwrap().description(), "A test tool");

        let non_existent_tool = registry.get_tool("non_existent");
        assert!(non_existent_tool.is_none());
    }

    #[test]
    fn test_tool_registry_get_tool_schemas() {
        let mut registry = ToolRegistry::new();
        let schema1 = json!({ "type": "object", "properties": { "arg1": { "type": "string" } } });
        let schema2 = json!({ "type": "object", "properties": { "arg2": { "type": "number" } } });

        let tool1 = Box::new(DummyTool::new("tool1", "First tool", schema1.clone()));
        let tool2 = Box::new(DummyTool::new("tool2", "Second tool", schema2.clone()));

        registry.register(tool1);
        registry.register(tool2);

        let schemas_result = registry.get_tool_schemas();
        assert!(schemas_result.is_ok());
        let schemas = schemas_result.unwrap();

        assert_eq!(schemas.len(), 2);
        // HashMap iteration order is not guaranteed, so check for presence
        assert!(schemas.contains(&schema1));
        assert!(schemas.contains(&schema2));
    }

    #[test]
    fn test_tool_registry_get_tool_schemas_empty() {
        let registry = ToolRegistry::new();
        let schemas_result = registry.get_tool_schemas();
        assert!(schemas_result.is_ok());
        assert!(schemas_result.unwrap().is_empty());
    }

    // Test case for when parameters_schema returns an error
    #[derive(Debug)]
    struct FailingSchemaTool;

    #[async_trait]
    impl CliTool for FailingSchemaTool {
        fn name(&self) -> String { "failing_schema".to_string() }
        fn description(&self) -> String { "Tool that fails schema generation".to_string() }
        fn parameters_schema(&self) -> Result<Value> {
            Err(anyhow::anyhow!("Schema generation failed"))
        }
        async fn execute(&self, _args: Value) -> Result<Value, ToolError> {
            Ok(json!({}))
        }
    }

    #[test]
    fn test_tool_registry_get_tool_schemas_error() {
        let mut registry = ToolRegistry::new();
        let failing_tool = Box::new(FailingSchemaTool);
        registry.register(failing_tool);

        let schemas_result = registry.get_tool_schemas();
        assert!(schemas_result.is_err());
    }
}
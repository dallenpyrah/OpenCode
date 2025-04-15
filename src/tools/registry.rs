use std::collections::HashMap;
use crate::config::Config; 
use crate::tools::CliTool;
use anyhow::Result;
use crate::api::models::{ToolDefinition, FunctionDefinition};
use crate::tools::code_intelligence::ListCodeDefinitionsTool;
use crate::tools::command_execution::ExecuteCommandTool;

use crate::tools::web_search::WebSearchTool;

#[derive(Debug, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn CliTool>>,
}

impl ToolRegistry {
    
    
    
    
    pub fn new(config: &Config) -> Self { 
        let mut registry = Self::default();

        registry.register(Box::new(crate::tools::FileReadTool));
        registry.register(Box::new(crate::tools::FileWriteTool));
        registry.register(Box::new(crate::tools::ShellCommandTool));
        registry.register(Box::new(crate::tools::GitTool));
        registry.register(Box::new(WebSearchTool));
        registry.register(Box::new(crate::tools::CodeSearchTool));
        registry.register(Box::new(crate::tools::FileSearchTool));
        registry.register(Box::new(crate::tools::CreateDirectoryTool));
        registry.register(Box::new(crate::tools::DeleteTool));
        registry.register(Box::new(crate::tools::ListFilesTool));

        registry.register(Box::new(ListCodeDefinitionsTool));
        registry.register(Box::new(ExecuteCommandTool));

        if let Some(user_tool_configs) = &config.usertools {
            for tool_config in user_tool_configs {
                match crate::tools::UserDefinedTool::new(tool_config) {
                    Ok(user_tool) => registry.register(Box::new(user_tool)),
                    Err(e) => {
                        tracing::error!("Failed to load user tool '{}': {}", tool_config.name, e);
                        
                    }
                }
            }
        }

        registry
    }

    
    
    
    
    pub fn register(&mut self, tool: Box<dyn CliTool>) { 
        let name = tool.name();
        tracing::debug!("Registering tool: {}", name);
        self.tools.insert(name, tool);
    }

    
    pub fn get_tool_definitions(&self) -> Result<Vec<ToolDefinition>> {
        self.tools
            .values()
            .map(|tool| {
                let schema = tool.parameters_schema()?;
                Ok(ToolDefinition {
                    tool_type: "function".to_string(),
                    function: FunctionDefinition {
                        name: tool.name(),
                        description: tool.description(),
                        parameters: schema,
                    },
                })
            })
            .collect()
    }

    
    
    
    #[allow(clippy::borrowed_box)] 
    pub fn get_tool(&self, name: &str) -> Option<&Box<dyn CliTool>> { 
        self.tools.get(name)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;
    use async_trait::async_trait;
    use crate::tools::ToolError;
    use serde_json::Value;

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
        let config = Config::default(); 
        let registry = ToolRegistry::new(&config); 
        assert_eq!(registry.tools.len(), 12);
    }

    #[test]
    fn test_tool_registry_register_and_get() {
        let config = Config::default(); 
        let mut registry = ToolRegistry::new(&config); 
        let dummy_tool = Box::new(DummyTool::new("dummy", "A test tool", json!({ "type": "object" })));
        let tool_name = dummy_tool.name();

        registry.register(dummy_tool);

        assert_eq!(registry.tools.len(), 13);
        let retrieved_tool = registry.get_tool(&tool_name);
        assert!(retrieved_tool.is_some());
        assert_eq!(retrieved_tool.unwrap().name(), tool_name);
        assert_eq!(retrieved_tool.unwrap().description(), "A test tool");

        let non_existent_tool = registry.get_tool("non_existent");
        assert!(non_existent_tool.is_none());
    }

    #[test]
    fn test_tool_registry_get_tool_schemas() {
        let config = Config::default(); 
        let mut registry = ToolRegistry::new(&config); 
        let schema1 = json!({ "type": "object", "properties": { "arg1": { "type": "string" } } });
        let schema2 = json!({ "type": "object", "properties": { "arg2": { "type": "number" } } });

        let tool1 = Box::new(DummyTool::new("tool1", "First tool", schema1.clone()));
        let tool2 = Box::new(DummyTool::new("tool2", "Second tool", schema2.clone()));

        registry.register(tool1);
        registry.register(tool2);

        let schemas_result = registry.get_tool_definitions();
        assert!(schemas_result.is_ok());
        let schemas = schemas_result.unwrap();

        assert_eq!(schemas.len(), 14);
    }

    #[test]
    fn test_tool_registry_get_tool_schemas_empty() {
        let config = Config::default(); 
        let registry = ToolRegistry::new(&config); 
        let schemas_result = registry.get_tool_definitions();
        assert!(schemas_result.is_ok());
        assert_eq!(schemas_result.unwrap().len(), 12);
    }

    
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

    #[tokio::test]
    async fn test_tool_registry_get_tool_schemas_error() {
        let config = Config::default(); 
        let mut registry = ToolRegistry::new(&config); 
        let failing_tool = Box::new(FailingSchemaTool);
        registry.register(failing_tool);

        let schemas_result = registry.get_tool_definitions();
        assert!(schemas_result.is_err());
    }
}
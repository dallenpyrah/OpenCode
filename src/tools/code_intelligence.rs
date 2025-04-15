use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf}; // Path is used in parse_definitions, PathBuf in execute
use tokio::fs; // Used for async file reading
use tree_sitter::{Parser, Query, QueryCursor};
use serde_json::Value; // Needed for CliTool trait

use crate::tools::{CliTool, ToolError}; // Correct trait and error type

#[derive(Debug, Serialize, Deserialize)]
pub struct ListCodeDefinitionsInput {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CodeDefinition {
    pub name: String,
    pub r#type: String, // Using r# to allow "type" as a field name
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ListCodeDefinitionsOutput {
    pub definitions: Vec<CodeDefinition>,
}

#[derive(Debug)]
pub struct ListCodeDefinitionsTool;

#[async_trait]
impl CliTool for ListCodeDefinitionsTool {
    fn name(&self) -> String {
        "list_code_definition_names".to_string()
    }

    fn description(&self) -> String {
        "Request to list definition names (classes, functions, methods, etc.) from source code. \
         Analyzes a single file specified by path. \
         Provides insights into the codebase structure and important constructs."
            .to_string()
    }

    fn parameters_schema(&self) -> Result<Value> {
        Ok(serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path of the file or directory to analyze."
                }
            },
            "required": ["path"]
        }))
    }

    async fn execute(&self, args: Value) -> Result<Value, ToolError> {
        let input: ListCodeDefinitionsInput = serde_json::from_value(args).map_err(|e| {
            ToolError::InvalidArguments {
                tool_name: self.name(),
                details: format!("Failed to parse arguments: {}", e),
            }
        })?;

        let file_path = PathBuf::from(&input.path);

        if !file_path.is_file() {
             return Err(ToolError::FileNotFound { path: input.path });
        }

        let content = fs::read_to_string(&file_path)
            .await
            .map_err(|e| ToolError::Other {
                 message: format!("Failed to read file {}: {}", input.path, e),
            })?;

        let definitions = parse_definitions(&file_path, &content)
            .map_err(|e| ToolError::Other {
                 message: format!("Failed to parse definitions in {}: {}", input.path, e),
            })?;

        let output = ListCodeDefinitionsOutput { definitions };
        serde_json::to_value(output).map_err(|e| ToolError::Other {
            message: format!("Failed to serialize output: {}", e),
        })
    }
}

fn parse_definitions(path: &Path, source_code: &str) -> Result<Vec<CodeDefinition>> {
    let extension = path.extension().and_then(|ext| ext.to_str());

    // TODO: Support more languages
    let language = match extension {
        Some("rs") => tree_sitter_rust::language(),
        _ => return Err(anyhow!("Unsupported language for file: {:?}", path)),
    };

    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .context("Failed to set language for parser")?;

    let tree = parser
        .parse(source_code, None)
        .ok_or_else(|| anyhow!("Failed to parse file: {:?}", path))?;

    // Query to find top-level functions, structs, enums, traits, and impl blocks
    // Captures the name identifier for each definition type.
    let query_str = r#"
        (function_item name: (identifier) @function.name) @function.definition
        (struct_item name: (type_identifier) @struct.name) @struct.definition
        (enum_item name: (type_identifier) @enum.name) @enum.definition
        (trait_item name: (type_identifier) @trait.name) @trait.definition
        (impl_item trait: (type_identifier) @impl.trait type: (type_identifier) @impl.type) @impl.definition
        (impl_item type: (type_identifier) @impl.type) @impl.definition_no_trait
    "#;

    let query = Query::new(&language, query_str).context("Failed to create query")?;

    let mut query_cursor = QueryCursor::new();
    let matches = query_cursor.matches(&query, tree.root_node(), source_code.as_bytes());

    let mut definitions = Vec::new();
    let capture_names = query.capture_names();

    for match_result in matches {
        let mut definition_name = None;
        let mut definition_type = None;

        for capture in match_result.captures {
            let capture_name = &capture_names[capture.index as usize];
            let node_text = capture
                .node
                .utf8_text(source_code.as_bytes())
                .context("Failed to get text for capture")?
                .to_string();

            match *capture_name {
                "function.name" => {
                    definition_name = Some(node_text);
                    definition_type = Some("function".to_string());
                }
                "struct.name" => {
                    definition_name = Some(node_text);
                    definition_type = Some("struct".to_string());
                }
                "enum.name" => {
                    definition_name = Some(node_text);
                    definition_type = Some("enum".to_string());
                }
                "trait.name" => {
                    definition_name = Some(node_text);
                    definition_type = Some("trait".to_string());
                }
                "impl.type" if definition_type.is_none() => {
                    // Handle impl blocks (try to capture trait if present)
                    let mut impl_trait_name = None;
                    for c in match_result.captures {
                        if capture_names[c.index as usize] == "impl.trait" { // Removed extra &
                             impl_trait_name = Some(c.node.utf8_text(source_code.as_bytes())?.to_string());
                             break;
                        }
                    }
                    if let Some(trait_name) = impl_trait_name {
                         definition_name = Some(format!("impl {} for {}", trait_name, node_text));
                    } else {
                         definition_name = Some(format!("impl {}", node_text));
                    }
                    definition_type = Some("impl".to_string());
                }
                _ => {} // Ignore other captures like the full definition block
            }
        }

        if let (Some(name), Some(r#type)) = (definition_name, definition_type) {
            // Avoid adding duplicates if multiple captures match (e.g., impl block)
            // Access fields directly on the borrowed item 'd'
            if !definitions.iter().any(|d: &CodeDefinition| d.name == name && d.r#type == r#type) { // Explicitly type 'd'
                 definitions.push(CodeDefinition { name, r#type });
            }
        }
    }

    Ok(definitions)
}
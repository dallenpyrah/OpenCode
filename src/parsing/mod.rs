use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::Path;
use tree_sitter::{Parser, Query, QueryCursor};

pub fn find_symbol_context(file_path: &str, symbol_name: &str) -> Result<String> {
    let path = Path::new(file_path);
    let extension = path.extension().and_then(|ext| ext.to_str());

    let language = match extension {
        Some("rs") => tree_sitter_rust::language(),
        _ => return Err(anyhow!("Unsupported language for file: {}", file_path)),
    };

    let source_code = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", file_path))?;

    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .with_context(|| format!("Failed to set language for parser"))?;

    let tree = parser
        .parse(&source_code, None)
        .ok_or_else(|| anyhow!("Failed to parse file: {}", file_path))?;

    
    let query_str = format!(
        r#"
        (function_item
          name: (identifier) @function.name
          (#eq? @function.name "{}")
        ) @function.definition
        "#,
        symbol_name
    );

    let query = Query::new(&language, &query_str)
        .with_context(|| format!("Failed to create query for symbol: {}", symbol_name))?;

    let mut query_cursor = QueryCursor::new();
    let matches = query_cursor.matches(&query, tree.root_node(), source_code.as_bytes());

    for match_result in matches {
        for capture in match_result.captures {
            if query.capture_names()[capture.index as usize] == "function.definition" {
                let node = capture.node;
                let code_block = node
                    .utf8_text(source_code.as_bytes())
                    .with_context(|| "Failed to extract text from node")?;
                return Ok(code_block.to_string());
            }
        }
    }

    Err(anyhow!(
        "Symbol '{}' not found in file: {}",
        symbol_name,
        file_path
    ))
}
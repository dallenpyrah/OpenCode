use serde_json::Value;

pub fn format_tool_result(tool_name: &str, result: &Value, error: Option<&str>) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("tool_name".to_string(), Value::String(tool_name.to_string()));
    if let Some(err) = error {
        obj.insert("error".to_string(), Value::String(err.to_string()));
    } else {
        obj.insert("result".to_string(), result.clone());
    }
    Value::Object(obj)
}

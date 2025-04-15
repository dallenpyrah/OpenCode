use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Debug, Clone)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_map: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
#[serde(rename_all = "lowercase")]
pub enum Role {
    System, 
    User,
    Assistant,
    Tool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: Option<String>, 
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>, 
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>, 
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String, 
    pub function: FunctionDefinition,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value, 
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)] 
pub enum ToolChoice {
    None,
    Auto,
    Tool {
        #[serde(rename = "type")]
        tool_type: String, 
        function: ToolChoiceFunction,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ToolChoiceFunction {
    pub name: String,
}



#[derive(Deserialize, Debug, Clone)] 
pub struct ChatCompletionResponse {
    pub choices: Vec<Choice>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Choice {
    pub message: Message, 
    
}

#[derive(Serialize, Deserialize, Debug, Clone)] 
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String, 
    pub function: ToolCallFunction,
}

#[derive(Serialize, Deserialize, Debug, Clone)] 
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String, 
}

#[derive(Deserialize, Debug, Clone, Default)] 
pub struct UsageStats {
}




#[derive(Deserialize, Debug, Clone)] 
pub struct ChatCompletionChunk {
    pub choices: Vec<ChunkChoice>,
}

#[derive(Deserialize, Debug, Clone)] 
pub struct ChunkChoice {
    pub delta: Delta, 
    
}

#[derive(Deserialize, Debug, Clone)] 
pub struct Delta {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}


#[derive(Deserialize, Debug, Clone)] 
pub struct ToolCallChunk {
}

#[derive(Deserialize, Debug, Clone)] 
pub struct ToolCallFunctionChunk {
}
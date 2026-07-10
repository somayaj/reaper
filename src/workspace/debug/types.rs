use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DebugStatus {
    Idle,
    Starting,
    Running,
    Stopped,
    Terminated,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebugBreakpoint {
    pub path: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StackFrame {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebugVariable {
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    pub variables_reference: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebugState {
    pub status: DebugStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    pub frames: Vec<StackFrame>,
    pub variables: Vec<DebugVariable>,
    pub breakpoints: Vec<DebugBreakpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Default for DebugState {
    fn default() -> Self {
        Self {
            status: DebugStatus::Idle,
            language: None,
            adapter: None,
            thread_id: None,
            stop_reason: None,
            frames: Vec::new(),
            variables: Vec::new(),
            breakpoints: Vec::new(),
            message: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebugCapabilities {
    pub supported: bool,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum DebugEvent {
    State { state: DebugState },
    Output { category: String, text: String },
    Message { text: String },
}

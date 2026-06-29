use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickFix {
    pub title: String,
    pub edits: Vec<QuickFixEdit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickFixEdit {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuickFixDiagnostic {
    pub line: u32,
    pub column: u32,
    pub message: String,
    #[serde(default)]
    pub severity: String,
}

impl QuickFixEdit {
    pub fn clamp_to_document(&mut self, line_count: u32, line_len: impl Fn(u32) -> u32) {
        self.start_line = self.start_line.clamp(1, line_count);
        self.end_line = self.end_line.clamp(1, line_count);
        let start_max = line_len(self.start_line);
        let end_max = line_len(self.end_line);
        self.start_column = self.start_column.clamp(1, start_max.max(1));
        self.end_column = self.end_column.clamp(1, end_max.max(1));
        if self.end_line == self.start_line && self.end_column < self.start_column {
            self.end_column = self.start_column;
        }
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCheckItem {
    pub id: String,
    pub pass: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateReport {
    pub kind: String,
    pub phase: String,
    pub pass: bool,
    pub checks: Vec<GateCheckItem>,
}

pub fn gate_item(id: &str, pass: bool, message: String) -> GateCheckItem {
    GateCheckItem {
        id: id.to_string(),
        pass,
        message,
    }
}

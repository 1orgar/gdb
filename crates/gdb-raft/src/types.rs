use gdb_core::{DataValue, EdgeId, LabelId, VertexId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type NodeId = u64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RaftMutation {
    InsertVertex {
        label_id: LabelId,
        id: VertexId,
        properties: HashMap<String, DataValue>,
    },
    InsertEdge {
        edge: EdgeId,
    },
    DeleteEdge {
        edge: EdgeId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaftResponse {
    pub success: bool,
    pub commit_version: u64,
    pub message: String,
}

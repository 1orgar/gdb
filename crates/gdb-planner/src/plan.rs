use gdb_core::{EdgeType, LabelId, VertexId};
use gdb_parser::ast::{Expr, ReturnItem};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PhysicalOperator {
    /// Scans vertices of a label from storage.
    ScanVertices {
        var_name: String,
        label_id: LabelId,
        id_filter: Option<VertexId>,
    },
    /// Expands outgoing edges from previous operator's vertices.
    ExpandEdges {
        input: Box<PhysicalOperator>,
        src_var: String,
        edge_var: Option<String>,
        dst_var: String,
        edge_type: Option<EdgeType>,
    },
    /// Evaluates WHERE predicate.
    Filter {
        input: Box<PhysicalOperator>,
        predicate: Expr,
    },
    /// Projects RETURN expressions and creates final result columns.
    Project {
        input: Box<PhysicalOperator>,
        items: Vec<ReturnItem>,
    },
    /// Truncates results.
    Limit {
        input: Box<PhysicalOperator>,
        limit: usize,
    },
}

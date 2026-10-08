use gdb_core::{DataValue, Direction, PropertySpec, VertexId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    CreateVertexLabel {
        label: String,
        properties: Vec<PropertySpec>,
    },
    CreateEdgeType {
        edge_type: String,
        properties: Vec<PropertySpec>,
    },
    CreateIndex {
        label: String,
        property: String,
    },
    DropIndex {
        label: String,
        property: String,
    },
    InsertVertex {
        label: String,
        id: VertexId,
        properties: Vec<(String, DataValue)>,
    },
    InsertVertices {
        label: String,
        vertices: Vec<(VertexId, Vec<(String, DataValue)>)>,
    },
    InsertEdge {
        edge_type: String,
        src: VertexId,
        dst: VertexId,
        rank: i64,
        properties: Vec<(String, DataValue)>,
    },
    InsertEdges {
        edge_type: String,
        edges: Vec<(VertexId, VertexId, i64, Vec<(String, DataValue)>)>,
    },
    DeleteEdge {
        edge_type: String,
        src: VertexId,
        dst: VertexId,
        rank: i64,
    },
    MergeVertex {
        label: String,
        id: VertexId,
        properties: Vec<(String, DataValue)>,
    },
    Query(CypherQuery),
    CallAlgorithm {
        algorithm: String,
        args: std::collections::HashMap<String, DataValue>,
        yield_items: Vec<String>,
    },
    Explain(Box<Statement>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CypherQuery {
    pub pattern: PathPattern,
    pub where_clause: Option<Expr>,
    #[serde(default)]
    pub updates: Vec<UpdateClause>,
    #[serde(default)]
    pub distinct: bool,
    pub return_items: Vec<ReturnItem>,
    #[serde(default)]
    pub order_by: Vec<OrderByItem>,
    pub skip: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UpdateClause {
    Set {
        variable: String,
        property: String,
        expr: Expr,
    },
    Delete {
        variable: String,
        detach: bool,
    },
    MergeVertex {
        variable: Option<String>,
        label: String,
        id: Option<VertexId>,
        properties: Vec<(String, Expr)>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePattern {
    pub variable: Option<String>,
    pub label: Option<String>,
    pub id_filter: Option<VertexId>,
    #[serde(default)]
    pub properties: Vec<(String, DataValue)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgePattern {
    pub variable: Option<String>,
    pub edge_type: Option<String>,
    pub direction: Direction,
    #[serde(default = "default_min_hops")]
    pub min_hops: usize,
    #[serde(default = "default_max_hops")]
    pub max_hops: Option<usize>,
}

fn default_min_hops() -> usize {
    1
}

fn default_max_hops() -> Option<usize> {
    Some(1)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathPattern {
    pub start_node: NodePattern,
    /// List of (edge, target_node) steps
    pub hops: Vec<(EdgePattern, NodePattern)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnItem {
    pub expr: Expr,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderByItem {
    pub expr: Expr,
    pub ascending: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BinaryOperator {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    Plus,
    Minus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Literal(DataValue),
    Variable(String),
    Property {
        variable: String,
        property: String,
    },
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOperator,
        right: Box<Expr>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
    CountStar,
}

impl Expr {
    pub fn is_aggregate(&self) -> bool {
        match self {
            Expr::CountStar => true,
            Expr::FunctionCall { name, .. } => {
                let n = name.to_uppercase();
                matches!(n.as_str(), "COUNT" | "SUM" | "AVG" | "MIN" | "MAX")
            }
            _ => false,
        }
    }
}

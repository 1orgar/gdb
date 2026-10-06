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
    InsertVertex {
        label: String,
        id: VertexId,
        properties: Vec<(String, DataValue)>,
    },
    InsertEdge {
        edge_type: String,
        src: VertexId,
        dst: VertexId,
        rank: i64,
        properties: Vec<(String, DataValue)>,
    },
    DeleteEdge {
        edge_type: String,
        src: VertexId,
        dst: VertexId,
        rank: i64,
    },
    Query(CypherQuery),
    CallAlgorithm {
        algorithm: String,
        args: std::collections::HashMap<String, DataValue>,
        yield_items: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CypherQuery {
    pub pattern: PathPattern,
    pub where_clause: Option<Expr>,
    pub return_items: Vec<ReturnItem>,
    pub order_by: Vec<OrderByItem>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePattern {
    pub variable: Option<String>,
    pub label: Option<String>,
    pub id_filter: Option<VertexId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgePattern {
    pub variable: Option<String>,
    pub edge_type: Option<String>,
    pub direction: Direction,
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

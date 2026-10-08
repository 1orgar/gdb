use gdb_core::{DataValue, EdgeType, LabelId, VertexId};
use gdb_parser::ast::{Expr, OrderByItem, ReturnItem, UpdateClause};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PhysicalOperator {
    /// Scans vertices of a label from storage.
    ScanVertices {
        var_name: String,
        label_id: LabelId,
        id_filter: Option<VertexId>,
    },
    /// Fast secondary property index scan: looks up vertices where label.prop == val
    IndexScan {
        var_name: String,
        label_id: LabelId,
        property: String,
        value: DataValue,
    },
    /// Expands outgoing edges from previous operator's vertices.
    ExpandEdges {
        input: Box<PhysicalOperator>,
        src_var: String,
        edge_var: Option<String>,
        dst_var: String,
        edge_type: Option<EdgeType>,
    },
    /// Variable-length multi-hop path expansion (e.g., [:TYPE*1..3])
    VarLengthExpand {
        input: Box<PhysicalOperator>,
        src_var: String,
        edge_var: Option<String>,
        dst_var: String,
        edge_type: Option<EdgeType>,
        min_hops: usize,
        max_hops: Option<usize>,
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
    /// Sorts rows by OrderByItem expressions.
    Sort {
        input: Box<PhysicalOperator>,
        order_by: Vec<OrderByItem>,
    },
    /// Skips first N rows (OFFSET/SKIP).
    Skip {
        input: Box<PhysicalOperator>,
        skip: usize,
    },
    /// Deduplicates rows (DISTINCT).
    Distinct {
        input: Box<PhysicalOperator>,
    },
    /// Truncates results.
    Limit {
        input: Box<PhysicalOperator>,
        limit: usize,
    },
    /// Mutates graph data: SET properties or DELETE vertices.
    Mutate {
        input: Box<PhysicalOperator>,
        updates: Vec<UpdateClause>,
    },
}

impl PhysicalOperator {
    pub fn operator_name(&self) -> &'static str {
        match self {
            PhysicalOperator::ScanVertices { .. } => "ScanVertices",
            PhysicalOperator::IndexScan { .. } => "IndexScan",
            PhysicalOperator::ExpandEdges { .. } => "ExpandEdges",
            PhysicalOperator::VarLengthExpand { .. } => "VarLengthExpand",
            PhysicalOperator::Filter { .. } => "Filter",
            PhysicalOperator::Project { .. } => "Project",
            PhysicalOperator::Sort { .. } => "Sort",
            PhysicalOperator::Skip { .. } => "Skip",
            PhysicalOperator::Distinct { .. } => "Distinct",
            PhysicalOperator::Limit { .. } => "Limit",
            PhysicalOperator::Mutate { .. } => "Mutate",
        }
    }

    pub fn operator_details(&self) -> String {
        match self {
            PhysicalOperator::ScanVertices { var_name, label_id, id_filter } => {
                if let Some(id) = id_filter {
                    format!("var: {}, label_id: {:?}, id: {}", var_name, label_id, id)
                } else {
                    format!("var: {}, label_id: {:?}", var_name, label_id)
                }
            }
            PhysicalOperator::IndexScan { var_name, label_id, property, value } => {
                format!("var: {}, label_id: {:?}, {} = {}", var_name, label_id, property, value)
            }
            PhysicalOperator::ExpandEdges { src_var, edge_var, dst_var, edge_type, .. } => {
                format!(
                    "({})-[:{:?}{}]->({})",
                    src_var,
                    edge_type,
                    edge_var.as_ref().map(|v| format!(" as {}", v)).unwrap_or_default(),
                    dst_var
                )
            }
            PhysicalOperator::VarLengthExpand { src_var, dst_var, edge_type, min_hops, max_hops, .. } => {
                format!("({})-[:{:?}*{}..{:?}]->({})", src_var, edge_type, min_hops, max_hops, dst_var)
            }
            PhysicalOperator::Filter { predicate, .. } => {
                format!("predicate: {:?}", predicate)
            }
            PhysicalOperator::Project { items, .. } => {
                format!("items: {}", items.len())
            }
            PhysicalOperator::Sort { order_by, .. } => {
                let items: Vec<String> = order_by.iter().map(|o| format!("{:?} {}", o.expr, if o.ascending { "ASC" } else { "DESC" })).collect();
                format!("order: {}", items.join(", "))
            }
            PhysicalOperator::Skip { skip, .. } => format!("skip: {}", skip),
            PhysicalOperator::Distinct { .. } => "distinct: true".to_string(),
            PhysicalOperator::Limit { limit, .. } => format!("limit: {}", limit),
            PhysicalOperator::Mutate { updates, .. } => format!("updates: {}", updates.len()),
        }
    }

    /// Formats the physical execution plan as an indented tree diagram.
    pub fn format_ascii_tree(&self, indent: usize) -> String {
        let prefix = if indent == 0 {
            "".to_string()
        } else {
            format!("{}{}─ ", "  ".repeat(indent - 1), "└")
        };
        let line = format!("{}{}: {}\n", prefix, self.operator_name(), self.operator_details());

        let child_str = match self {
            PhysicalOperator::ScanVertices { .. } | PhysicalOperator::IndexScan { .. } => String::new(),
            PhysicalOperator::ExpandEdges { input, .. }
            | PhysicalOperator::VarLengthExpand { input, .. }
            | PhysicalOperator::Filter { input, .. }
            | PhysicalOperator::Project { input, .. }
            | PhysicalOperator::Sort { input, .. }
            | PhysicalOperator::Skip { input, .. }
            | PhysicalOperator::Distinct { input, .. }
            | PhysicalOperator::Limit { input, .. }
            | PhysicalOperator::Mutate { input, .. } => input.format_ascii_tree(indent + 1),
        };

        format!("{}{}", line, child_str)
    }
}

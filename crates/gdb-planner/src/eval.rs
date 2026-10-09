use gdb_core::{DataValue, EdgeId, GdbError, GdbResult, LabelId, VertexId};
use gdb_parser::ast::{BinaryOperator, Expr};
use gdb_storage::PartitionStorageEngine;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct PathRow {
    pub vertices: HashMap<String, (VertexId, LabelId)>,
    pub edges: HashMap<String, EdgeId>,
    pub custom_values: HashMap<String, DataValue>,
}

pub fn eval_expr(
    expr: &Expr,
    row: &PathRow,
    storage: &PartitionStorageEngine,
) -> GdbResult<DataValue> {
    match expr {
        Expr::Literal(val) => Ok(val.clone()),
        Expr::Variable(var) => {
            if let Some(val) = row.custom_values.get(var) {
                Ok(val.clone())
            } else if let Some(&(vid, _)) = row.vertices.get(var) {
                Ok(DataValue::Int64(vid.as_u64() as i64))
            } else {
                Err(GdbError::Execution(format!("Unbound variable: {}", var)))
            }
        }
        Expr::Property { variable, property } => {
            let compound = format!("{}.{}", variable, property);
            if let Some(val) = row.custom_values.get(&compound) {
                return Ok(val.clone());
            }
            if let Some(&(vid, label_id)) = row.vertices.get(variable) {
                if property == "id" || property == "_id" {
                    return Ok(DataValue::Int64(vid.as_u64() as i64));
                }
                let val = storage.get_vertex_property(vid, label_id, property);
                Ok(val.unwrap_or(DataValue::Null))
            } else {
                Err(GdbError::Execution(format!("Unbound vertex variable in property: {}", variable)))
            }
        }
        Expr::BinaryOp { left, op, right } => {
            let left_val = eval_expr(left, row, storage)?;
            let right_val = eval_expr(right, row, storage)?;
            eval_binary_op(&left_val, op, &right_val)
        }
        Expr::CountStar => Err(GdbError::Execution("COUNT(*) must be handled by aggregator".into())),
        Expr::FunctionCall { name, .. } => {
            Err(GdbError::Execution(format!("Function '{}' not implemented", name)))
        }
    }
}

pub fn eval_binary_op(
    left: &DataValue,
    op: &BinaryOperator,
    right: &DataValue,
) -> GdbResult<DataValue> {
    match op {
        BinaryOperator::Eq => Ok(DataValue::Boolean(left == right)),
        BinaryOperator::NotEq => Ok(DataValue::Boolean(left != right)),
        BinaryOperator::Lt => Ok(DataValue::Boolean(left < right)),
        BinaryOperator::LtEq => Ok(DataValue::Boolean(left <= right)),
        BinaryOperator::Gt => Ok(DataValue::Boolean(left > right)),
        BinaryOperator::GtEq => Ok(DataValue::Boolean(left >= right)),
        BinaryOperator::And => {
            match (left, right) {
                (DataValue::Boolean(a), DataValue::Boolean(b)) => Ok(DataValue::Boolean(*a && *b)),
                _ => Ok(DataValue::Boolean(false)),
            }
        }
        BinaryOperator::Or => {
            match (left, right) {
                (DataValue::Boolean(a), DataValue::Boolean(b)) => Ok(DataValue::Boolean(*a || *b)),
                _ => Ok(DataValue::Boolean(false)),
            }
        }
        BinaryOperator::Plus => {
            match (left, right) {
                (DataValue::Int64(a), DataValue::Int64(b)) => Ok(DataValue::Int64(a + b)),
                (DataValue::Float64(a), DataValue::Float64(b)) => Ok(DataValue::Float64(a + b)),
                (DataValue::String(a), DataValue::String(b)) => Ok(DataValue::String(format!("{}{}", a, b))),
                _ => Err(GdbError::Execution("Invalid operand types for Plus".into())),
            }
        }
        BinaryOperator::Minus => {
            match (left, right) {
                (DataValue::Int64(a), DataValue::Int64(b)) => Ok(DataValue::Int64(a - b)),
                (DataValue::Float64(a), DataValue::Float64(b)) => Ok(DataValue::Float64(a - b)),
                _ => Err(GdbError::Execution("Invalid operand types for Minus".into())),
            }
        }
    }
}

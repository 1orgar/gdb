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
            } else if let Some(&(vid, label_id)) = row.vertices.get(var) {
                let mut entity = serde_json::Map::new();
                entity.insert("id".to_string(), serde_json::json!(vid.as_u64()));
                if let Some(DataValue::String(lbl)) = row.custom_values.get(&format!("{}.__label", var)) {
                    entity.insert("label".to_string(), serde_json::json!(lbl));
                } else {
                    entity.insert("label".to_string(), serde_json::json!(format!("Label_{}", label_id.0)));
                }
                let mut props_map = serde_json::Map::new();
                if let Some(all_props) = storage.get_all_vertex_properties(vid, Some(label_id)) {
                    for (k, v) in all_props {
                        if let Ok(jv) = serde_json::to_value(&v) {
                            props_map.insert(k, jv);
                        }
                    }
                }
                entity.insert("properties".to_string(), serde_json::Value::Object(props_map));
                Ok(DataValue::String(serde_json::Value::Object(entity).to_string()))
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
                if property == "label" || property == "_label" {
                    if let Some(lbl_val) = row.custom_values.get(&format!("{}.__label", variable)) {
                        return Ok(lbl_val.clone());
                    }
                    return Ok(DataValue::String(format!("Label_{}", label_id.0)));
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
        Expr::FunctionCall { name, args } => {
            let fname = name.to_uppercase();
            match fname.as_str() {
                "ID" | "_ID" => {
                    if let Some(Expr::Variable(var)) = args.first() {
                        if let Some(&(vid, _)) = row.vertices.get(var) {
                            return Ok(DataValue::Int64(vid.as_u64() as i64));
                        }
                    }
                    if let Some(arg) = args.first() {
                        let val = eval_expr(arg, row, storage)?;
                        return Ok(val);
                    }
                    Err(GdbError::Execution("id() requires 1 argument".into()))
                }
                "LABEL" => {
                    if let Some(Expr::Variable(var)) = args.first() {
                        if let Some(lbl) = row.custom_values.get(&format!("{}.__label", var)) {
                            return Ok(lbl.clone());
                        }
                        if let Some(&(_, lid)) = row.vertices.get(var) {
                            return Ok(DataValue::String(format!("Label_{}", lid.0)));
                        }
                    }
                    Err(GdbError::Execution("label() requires a vertex variable argument".into()))
                }
                "LABELS" => {
                    if let Some(Expr::Variable(var)) = args.first() {
                        if let Some(lbl) = row.custom_values.get(&format!("{}.__label", var)) {
                            return Ok(DataValue::List(vec![lbl.clone()]));
                        }
                        if let Some(&(_, lid)) = row.vertices.get(var) {
                            return Ok(DataValue::List(vec![DataValue::String(format!("Label_{}", lid.0))]));
                        }
                    }
                    Err(GdbError::Execution("labels() requires a vertex variable argument".into()))
                }
                _ => Err(GdbError::Execution(format!("Function '{}' not implemented", name))),
            }
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

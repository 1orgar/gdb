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

#[cfg(test)]
mod tests {
    use super::*;
    use gdb_core::schema::GraphSchema;
    use parking_lot::RwLock;
    use std::sync::Arc;

    #[test]
    fn test_eval_binary_ops() {
        assert_eq!(
            eval_binary_op(&DataValue::Int64(5), &BinaryOperator::Eq, &DataValue::Int64(5)).unwrap(),
            DataValue::Boolean(true)
        );
        assert_eq!(
            eval_binary_op(&DataValue::Int64(5), &BinaryOperator::NotEq, &DataValue::Int64(6)).unwrap(),
            DataValue::Boolean(true)
        );
        assert_eq!(
            eval_binary_op(&DataValue::Int64(3), &BinaryOperator::Lt, &DataValue::Int64(5)).unwrap(),
            DataValue::Boolean(true)
        );
        assert_eq!(
            eval_binary_op(&DataValue::Int64(5), &BinaryOperator::LtEq, &DataValue::Int64(5)).unwrap(),
            DataValue::Boolean(true)
        );
        assert_eq!(
            eval_binary_op(&DataValue::Int64(7), &BinaryOperator::Gt, &DataValue::Int64(5)).unwrap(),
            DataValue::Boolean(true)
        );
        assert_eq!(
            eval_binary_op(&DataValue::Int64(7), &BinaryOperator::GtEq, &DataValue::Int64(7)).unwrap(),
            DataValue::Boolean(true)
        );

        // Boolean And / Or
        assert_eq!(
            eval_binary_op(&DataValue::Boolean(true), &BinaryOperator::And, &DataValue::Boolean(false)).unwrap(),
            DataValue::Boolean(false)
        );
        assert_eq!(
            eval_binary_op(&DataValue::Boolean(true), &BinaryOperator::Or, &DataValue::Boolean(false)).unwrap(),
            DataValue::Boolean(true)
        );
        assert_eq!(
            eval_binary_op(&DataValue::Int64(1), &BinaryOperator::And, &DataValue::Boolean(true)).unwrap(),
            DataValue::Boolean(false)
        );
        assert_eq!(
            eval_binary_op(&DataValue::Int64(1), &BinaryOperator::Or, &DataValue::Boolean(true)).unwrap(),
            DataValue::Boolean(false)
        );

        // Plus / Minus
        assert_eq!(
            eval_binary_op(&DataValue::Int64(10), &BinaryOperator::Plus, &DataValue::Int64(20)).unwrap(),
            DataValue::Int64(30)
        );
        assert_eq!(
            eval_binary_op(&DataValue::Float64(1.5), &BinaryOperator::Plus, &DataValue::Float64(2.5)).unwrap(),
            DataValue::Float64(4.0)
        );
        assert_eq!(
            eval_binary_op(&DataValue::String("hello ".into()), &BinaryOperator::Plus, &DataValue::String("world".into())).unwrap(),
            DataValue::String("hello world".into())
        );
        assert!(eval_binary_op(&DataValue::Boolean(true), &BinaryOperator::Plus, &DataValue::Int64(1)).is_err());

        assert_eq!(
            eval_binary_op(&DataValue::Int64(30), &BinaryOperator::Minus, &DataValue::Int64(10)).unwrap(),
            DataValue::Int64(20)
        );
        assert_eq!(
            eval_binary_op(&DataValue::Float64(4.5), &BinaryOperator::Minus, &DataValue::Float64(1.5)).unwrap(),
            DataValue::Float64(3.0)
        );
        assert!(eval_binary_op(&DataValue::String("a".into()), &BinaryOperator::Minus, &DataValue::String("b".into())).is_err());
    }

    #[test]
    fn test_eval_expressions_and_functions() {
        let schema = Arc::new(RwLock::new(GraphSchema::new("test")));
        let storage = PartitionStorageEngine::new(0, schema.clone());
        let mut row = PathRow::default();

        row.custom_values.insert("val".to_string(), DataValue::Int64(42));
        row.vertices.insert("n".to_string(), (VertexId(101), LabelId(1)));
        row.custom_values.insert("n.__label".to_string(), DataValue::String("User".to_string()));

        // Literal
        assert_eq!(
            eval_expr(&Expr::Literal(DataValue::Int64(99)), &row, &storage).unwrap(),
            DataValue::Int64(99)
        );

        // Variable from custom_values
        assert_eq!(
            eval_expr(&Expr::Variable("val".to_string()), &row, &storage).unwrap(),
            DataValue::Int64(42)
        );

        // Variable from vertices (serialized JSON entity)
        let ent = eval_expr(&Expr::Variable("n".to_string()), &row, &storage).unwrap();
        if let DataValue::String(s) = ent {
            assert!(s.contains("\"id\":101"));
            assert!(s.contains("\"label\":\"User\""));
        } else {
            panic!("Expected serialized JSON string entity");
        }

        // Unbound variable
        assert!(eval_expr(&Expr::Variable("unbound".to_string()), &row, &storage).is_err());

        // Property id / _id / label / _label
        assert_eq!(
            eval_expr(&Expr::Property { variable: "n".into(), property: "id".into() }, &row, &storage).unwrap(),
            DataValue::Int64(101)
        );
        assert_eq!(
            eval_expr(&Expr::Property { variable: "n".into(), property: "_id".into() }, &row, &storage).unwrap(),
            DataValue::Int64(101)
        );
        assert_eq!(
            eval_expr(&Expr::Property { variable: "n".into(), property: "label".into() }, &row, &storage).unwrap(),
            DataValue::String("User".into())
        );
        assert_eq!(
            eval_expr(&Expr::Property { variable: "n".into(), property: "_label".into() }, &row, &storage).unwrap(),
            DataValue::String("User".into())
        );

        // Compound property in custom_values
        row.custom_values.insert("n.custom".into(), DataValue::String("prop_val".into()));
        assert_eq!(
            eval_expr(&Expr::Property { variable: "n".into(), property: "custom".into() }, &row, &storage).unwrap(),
            DataValue::String("prop_val".into())
        );

        // Missing property in storage
        assert_eq!(
            eval_expr(&Expr::Property { variable: "n".into(), property: "non_existent".into() }, &row, &storage).unwrap(),
            DataValue::Null
        );

        // Unbound vertex property
        assert!(eval_expr(&Expr::Property { variable: "unknown".into(), property: "x".into() }, &row, &storage).is_err());

        // BinaryOp in expr
        let add_expr = Expr::BinaryOp {
            left: Box::new(Expr::Literal(DataValue::Int64(10))),
            op: BinaryOperator::Plus,
            right: Box::new(Expr::Literal(DataValue::Int64(20))),
        };
        assert_eq!(eval_expr(&add_expr, &row, &storage).unwrap(), DataValue::Int64(30));

        // CountStar returns error
        assert!(eval_expr(&Expr::CountStar, &row, &storage).is_err());

        // Functions: id, _id, label, labels
        assert_eq!(
            eval_expr(&Expr::FunctionCall { name: "id".into(), args: vec![Expr::Variable("n".into())] }, &row, &storage).unwrap(),
            DataValue::Int64(101)
        );
        assert_eq!(
            eval_expr(&Expr::FunctionCall { name: "_ID".into(), args: vec![Expr::Variable("n".into())] }, &row, &storage).unwrap(),
            DataValue::Int64(101)
        );
        assert_eq!(
            eval_expr(&Expr::FunctionCall { name: "label".into(), args: vec![Expr::Variable("n".into())] }, &row, &storage).unwrap(),
            DataValue::String("User".into())
        );
        assert_eq!(
            eval_expr(&Expr::FunctionCall { name: "labels".into(), args: vec![Expr::Variable("n".into())] }, &row, &storage).unwrap(),
            DataValue::List(vec![DataValue::String("User".into())])
        );

        // Unknown function & errors
        assert!(eval_expr(&Expr::FunctionCall { name: "id".into(), args: vec![] }, &row, &storage).is_err());
        assert!(eval_expr(&Expr::FunctionCall { name: "label".into(), args: vec![] }, &row, &storage).is_err());
        assert!(eval_expr(&Expr::FunctionCall { name: "labels".into(), args: vec![] }, &row, &storage).is_err());
        assert!(eval_expr(&Expr::FunctionCall { name: "unknown_fn".into(), args: vec![] }, &row, &storage).is_err());
    }
}

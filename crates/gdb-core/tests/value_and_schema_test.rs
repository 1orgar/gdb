use arrow::array::{
    ArrayRef, BooleanArray, Date32Array, Float64Array, Int64Array, StringArray,
    TimestampMicrosecondArray,
};
use arrow::datatypes::DataType as ArrowDataType;
use gdb_core::schema::{DataType, GraphSchema, PropertySpec};
use gdb_core::{DataValue, EdgeType, LabelId};
use std::collections::HashSet;
use std::sync::Arc;

#[test]
fn test_data_value_variants_and_display() {
    let null_val = DataValue::Null;
    assert_eq!(null_val.to_string(), "NULL");
    assert_eq!(format!("{:?}", null_val), "NULL");
    assert_eq!(null_val.data_type(), ArrowDataType::Null);

    let bool_val = DataValue::Boolean(true);
    assert_eq!(bool_val.to_string(), "true");
    assert_eq!(format!("{:?}", bool_val), "true");
    assert_eq!(bool_val.data_type(), ArrowDataType::Boolean);

    let int_val = DataValue::Int64(42);
    assert_eq!(int_val.to_string(), "42");
    assert_eq!(format!("{:?}", int_val), "42");
    assert_eq!(int_val.data_type(), ArrowDataType::Int64);

    let float_val = DataValue::Float64(3.1415);
    assert_eq!(float_val.to_string(), "3.1415");
    assert!(format!("{:?}", float_val).contains("3.1415"));
    assert_eq!(float_val.data_type(), ArrowDataType::Float64);

    let str_val = DataValue::String("hello world".into());
    assert_eq!(str_val.to_string(), "hello world");
    assert_eq!(format!("{:?}", str_val), "\"hello world\"");
    assert_eq!(str_val.data_type(), ArrowDataType::Utf8);

    let date_val = DataValue::Date(19000);
    assert_eq!(date_val.to_string(), "19000");
    assert_eq!(format!("{:?}", date_val), "Date(19000)");
    assert_eq!(date_val.data_type(), ArrowDataType::Date32);

    let ts_val = DataValue::Timestamp(1700000000000000);
    assert_eq!(ts_val.to_string(), "1700000000000000");
    assert_eq!(format!("{:?}", ts_val), "Timestamp(1700000000000000)");
    assert!(matches!(ts_val.data_type(), ArrowDataType::Timestamp(_, _)));

    let list_val = DataValue::List(vec![DataValue::Int64(1), DataValue::Int64(2)]);
    assert_eq!(list_val.to_string(), "[1, 2]");
    assert_eq!(format!("{:?}", list_val), "[1, 2]");
    assert!(matches!(list_val.data_type(), ArrowDataType::List(_)));

    assert_eq!(DataValue::from(true), DataValue::Boolean(true));
    assert_eq!(DataValue::from(10i64), DataValue::Int64(10));
    assert_eq!(DataValue::from(4.5f64), DataValue::Float64(4.5));
    assert_eq!(DataValue::from("foo"), DataValue::String("foo".into()));
    assert_eq!(DataValue::from("bar".to_string()), DataValue::String("bar".into()));
}

#[test]
fn test_data_value_hash_and_ord() {
    let mut set = HashSet::new();
    set.insert(DataValue::Int64(10));
    set.insert(DataValue::Int64(10));
    set.insert(DataValue::Float64(0.0));
    set.insert(DataValue::Float64(-0.0));
    set.insert(DataValue::Float64(f64::NAN));
    set.insert(DataValue::String("test".into()));
    set.insert(DataValue::Null);
    set.insert(DataValue::Date(100));
    set.insert(DataValue::Timestamp(200));
    set.insert(DataValue::List(vec![DataValue::Boolean(true)]));

    assert!(set.contains(&DataValue::Int64(10)));
    assert!(set.contains(&DataValue::Null));

    assert!(DataValue::Int64(5) < DataValue::Int64(10));
    assert!(DataValue::Float64(1.5) < DataValue::Float64(2.5));
}

#[test]
fn test_data_value_extract_from_array() {
    let bool_arr: ArrayRef = Arc::new(BooleanArray::from(vec![Some(true), None, Some(false)]));
    assert_eq!(DataValue::extract_from_array(&bool_arr, 0).unwrap(), DataValue::Boolean(true));
    assert_eq!(DataValue::extract_from_array(&bool_arr, 1).unwrap(), DataValue::Null);
    assert_eq!(DataValue::extract_from_array(&bool_arr, 2).unwrap(), DataValue::Boolean(false));

    let int_arr: ArrayRef = Arc::new(Int64Array::from(vec![Some(100), None]));
    assert_eq!(DataValue::extract_from_array(&int_arr, 0).unwrap(), DataValue::Int64(100));
    assert_eq!(DataValue::extract_from_array(&int_arr, 1).unwrap(), DataValue::Null);

    let float_arr: ArrayRef = Arc::new(Float64Array::from(vec![Some(2.5), None]));
    assert_eq!(DataValue::extract_from_array(&float_arr, 0).unwrap(), DataValue::Float64(2.5));

    let str_arr: ArrayRef = Arc::new(StringArray::from(vec![Some("abc"), None]));
    assert_eq!(DataValue::extract_from_array(&str_arr, 0).unwrap(), DataValue::String("abc".into()));

    let date_arr: ArrayRef = Arc::new(Date32Array::from(vec![Some(1234), None]));
    assert_eq!(DataValue::extract_from_array(&date_arr, 0).unwrap(), DataValue::Date(1234));

    let ts_arr: ArrayRef = Arc::new(TimestampMicrosecondArray::from(vec![Some(5678), None]));
    assert_eq!(DataValue::extract_from_array(&ts_arr, 0).unwrap(), DataValue::Timestamp(5678));
}

#[test]
fn test_graph_schema_operations() {
    let mut schema = GraphSchema::new("test_db");
    assert_eq!(schema.name, "test_db");

    // Register vertex
    let v_props = vec![
        PropertySpec::new("name", DataType::String, false),
        PropertySpec::new("age", DataType::Int64, true),
    ];
    let lid = schema.register_vertex_label("Person", v_props).unwrap();
    assert_eq!(lid, LabelId(1));
    assert_eq!(schema.get_vertex_schema_by_id(lid).unwrap().label, "Person");

    // Duplicate error
    assert!(schema.register_vertex_label("Person", vec![]).is_err());

    // Register edge
    let e_props = vec![PropertySpec::new("since", DataType::Int64, true)];
    let etype = schema.register_edge_type("KNOWS", e_props).unwrap();
    assert_eq!(etype, EdgeType(1));
    assert_eq!(schema.get_edge_schema_by_type(etype).unwrap().edge_type_name, "KNOWS");
    assert!(schema.register_edge_type("KNOWS", vec![]).is_err());

    // Arrow schema generation
    let vs = schema.get_vertex_schema("Person").unwrap();
    let v_arrow = vs.to_arrow_schema();
    assert_eq!(v_arrow.fields().len(), 3); // _vertex_id + name + age

    let es = schema.get_edge_schema("KNOWS").unwrap();
    let e_arrow = es.to_arrow_schema();
    assert_eq!(e_arrow.fields().len(), 5); // _src_id, _edge_type, _rank, _dst_id, since

    // Indexes
    schema.register_index("Person", "name").unwrap();
    assert!(schema.has_index("Person", "name"));
    assert!(!schema.has_index("Person", "age"));
    assert_eq!(schema.get_indexes("Person"), vec!["name"]);

    schema.drop_index("Person", "name").unwrap();
    assert!(!schema.has_index("Person", "name"));

    // Alter vertex
    let add_props = vec![PropertySpec::new("email", DataType::String, true)];
    let drop_props = vec!["age".to_string()];
    schema.alter_vertex_label("Person", add_props, drop_props).unwrap();
    let vs_updated = schema.get_vertex_schema("Person").unwrap();
    assert!(vs_updated.properties.iter().any(|p| p.name == "email"));
    assert!(!vs_updated.properties.iter().any(|p| p.name == "age"));

    // Alter edge
    let add_e_props = vec![PropertySpec::new("weight", DataType::Float64, true)];
    let drop_e_props = vec!["since".to_string()];
    schema.alter_edge_type("KNOWS", add_e_props, drop_e_props).unwrap();
    let es_updated = schema.get_edge_schema("KNOWS").unwrap();
    assert!(es_updated.properties.iter().any(|p| p.name == "weight"));
    assert!(!es_updated.properties.iter().any(|p| p.name == "since"));

    // Drop label & edge
    schema.drop_vertex_label("Person").unwrap();
    assert!(schema.get_vertex_schema("Person").is_none());

    schema.drop_edge_type("KNOWS").unwrap();
    assert!(schema.get_edge_schema("KNOWS").is_none());
}

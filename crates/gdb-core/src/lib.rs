pub mod error;
pub mod id;
pub mod schema;
pub mod value;

pub use error::{GdbError, GdbResult};
pub use id::{Direction, EdgeId, EdgeRank, EdgeType, LabelId, VertexId};
pub use schema::{DataType, EdgeSchema, GraphSchema, PropertySpec, VertexSchema};
pub use value::DataValue;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertex_and_edge_ids() {
        let v1 = VertexId::from_str_key("user_alice");
        let v2 = VertexId::from_str_key("user_bob");
        assert_ne!(v1, v2);

        let edge = EdgeId::simple(v1, EdgeType(1), v2);
        assert_eq!(edge.src, v1);
        assert_eq!(edge.dst, v2);
        assert_eq!(edge.rank, 0);

        let partition = v1.partition(8);
        assert!(partition < 8);
    }

    #[test]
    fn test_schema_arrow_generation() {
        let mut graph_schema = GraphSchema::new("test_graph");
        let label_id = graph_schema.register_vertex_label(
            "User",
            vec![
                PropertySpec::new("name", DataType::String, false),
                PropertySpec::new("age", DataType::Int64, true),
            ],
        ).unwrap();

        let v_schema = graph_schema.get_vertex_schema_by_id(label_id).unwrap();
        let arrow_schema = v_schema.to_arrow_schema();
        assert_eq!(arrow_schema.fields().len(), 3); // _vertex_id, name, age
    }
}

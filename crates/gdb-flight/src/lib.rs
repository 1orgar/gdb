pub mod exchange;
pub mod service;

pub use exchange::ShufflePartitioner;
pub use service::GdbFlightService;

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Int64Array, RecordBatch, UInt64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    #[test]
    fn test_mpp_shuffle_partitioning() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("target_vid", DataType::UInt64, false),
            Field::new("score", DataType::Int64, false),
        ]));

        // 4 rows: target_vids = [0, 1, 2, 3]
        let vid_arr = Arc::new(UInt64Array::from(vec![0u64, 1, 2, 3]));
        let score_arr = Arc::new(Int64Array::from(vec![100i64, 200, 300, 400]));
        let batch = RecordBatch::try_new(schema, vec![vid_arr, score_arr]).unwrap();

        let partitioner = ShufflePartitioner::new(2);
        let partitioned = partitioner.partition_batch(&batch, 0).unwrap();

        // Should be split into 2 partitions: 0 and 1
        assert_eq!(partitioned.len(), 2);
        assert_eq!(partitioned[&0].num_rows(), 2); // 0 and 2
        assert_eq!(partitioned[&1].num_rows(), 2); // 1 and 3
    }
}

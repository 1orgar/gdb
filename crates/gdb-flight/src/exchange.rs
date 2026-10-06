use arrow::array::{Array, RecordBatch, UInt64Array};
use gdb_core::{GdbError, GdbResult, VertexId};
use std::collections::HashMap;

/// Partitions a RecordBatch across N partitions by hashing the VertexId in the given column.
pub struct ShufflePartitioner {
    pub total_partitions: u32,
}

impl ShufflePartitioner {
    pub fn new(total_partitions: u32) -> Self {
        Self { total_partitions }
    }

    /// Splits `batch` into a map of `PartitionId -> RecordBatch`
    pub fn partition_batch(
        &self,
        batch: &RecordBatch,
        vid_col_idx: usize,
    ) -> GdbResult<HashMap<u32, RecordBatch>> {
        let col = batch.column(vid_col_idx);
        let u64_col = col.as_any().downcast_ref::<UInt64Array>()
            .ok_or_else(|| GdbError::Execution("Shuffle column must be UInt64 VertexId".into()))?;

        // Group row indices by partition
        let mut part_indices: HashMap<u32, Vec<usize>> = HashMap::new();
        for row in 0..batch.num_rows() {
            let vid = VertexId(u64_col.value(row));
            let part = vid.partition(self.total_partitions);
            part_indices.entry(part).or_default().push(row);
        }

        let mut partitioned_batches = HashMap::new();
        for (part, indices) in part_indices {
            let indices_array = arrow::array::UInt32Array::from(
                indices.into_iter().map(|i| i as u32).collect::<Vec<_>>()
            );
            let sub_batch = arrow::compute::take_record_batch(batch, &indices_array)?;
            partitioned_batches.insert(part, sub_batch);
        }

        Ok(partitioned_batches)
    }
}

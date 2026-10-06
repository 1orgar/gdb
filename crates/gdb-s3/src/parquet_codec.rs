use arrow::array::{ArrayRef, Int64Array, RecordBatch, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use gdb_core::{EdgeId, EdgeType, GdbError, GdbResult, VertexId};
use gdb_storage::ChunkedCsr;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use std::sync::Arc;

/// Serializes ChunkedCsr edges to an in-memory Parquet byte buffer.
pub fn csr_to_parquet(csr: &ChunkedCsr) -> GdbResult<Vec<u8>> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("src_id", DataType::UInt64, false),
        Field::new("edge_type", DataType::UInt32, false),
        Field::new("rank", DataType::Int64, false),
        Field::new("dst_id", DataType::UInt64, false),
    ]));

    let mut src_ids = Vec::with_capacity(csr.targets.len());
    let mut edge_types = Vec::with_capacity(csr.targets.len());
    let mut ranks = Vec::with_capacity(csr.targets.len());
    let mut dst_ids = Vec::with_capacity(csr.targets.len());

    for &src_raw in &csr.reverse_map {
        let src = VertexId(src_raw);
        let edges = csr.get_out_edges(src, None);
        for e in edges {
            src_ids.push(e.src.as_u64());
            edge_types.push(e.edge_type.0);
            ranks.push(e.rank);
            dst_ids.push(e.dst.as_u64());
        }
    }

    let columns: Vec<ArrayRef> = vec![
        Arc::new(UInt64Array::from(src_ids)),
        Arc::new(UInt32Array::from(edge_types)),
        Arc::new(Int64Array::from(ranks)),
        Arc::new(UInt64Array::from(dst_ids)),
    ];

    let batch = RecordBatch::try_new(schema.clone(), columns)?;

    let mut buf = Vec::new();
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(Default::default()))
        .build();

    let mut writer = ArrowWriter::try_new(&mut buf, schema, Some(props))
        .map_err(|e| GdbError::Storage(e.to_string()))?;
    writer.write(&batch).map_err(|e| GdbError::Storage(e.to_string()))?;
    writer.close().map_err(|e| GdbError::Storage(e.to_string()))?;

    Ok(buf)
}

/// Restores ChunkedCsr from a Parquet byte buffer.
pub fn parquet_to_csr(bytes: &[u8]) -> GdbResult<ChunkedCsr> {
    let cursor = bytes::Bytes::copy_from_slice(bytes);
    let builder = ParquetRecordBatchReaderBuilder::try_new(cursor)
        .map_err(|e| GdbError::Storage(e.to_string()))?;
    let mut reader = builder.build().map_err(|e| GdbError::Storage(e.to_string()))?;

    let mut edges = Vec::new();

    while let Some(batch_res) = reader.next() {
        let batch = batch_res.map_err(|e| GdbError::Storage(e.to_string()))?;
        let src_col = batch.column(0).as_any().downcast_ref::<UInt64Array>()
            .ok_or_else(|| GdbError::Storage("Invalid src_col".into()))?;
        let type_col = batch.column(1).as_any().downcast_ref::<UInt32Array>()
            .ok_or_else(|| GdbError::Storage("Invalid type_col".into()))?;
        let rank_col = batch.column(2).as_any().downcast_ref::<Int64Array>()
            .ok_or_else(|| GdbError::Storage("Invalid rank_col".into()))?;
        let dst_col = batch.column(3).as_any().downcast_ref::<UInt64Array>()
            .ok_or_else(|| GdbError::Storage("Invalid dst_col".into()))?;

        for i in 0..batch.num_rows() {
            edges.push(EdgeId::new(
                VertexId(src_col.value(i)),
                EdgeType(type_col.value(i)),
                rank_col.value(i),
                VertexId(dst_col.value(i)),
            ));
        }
    }

    Ok(ChunkedCsr::from_edges(edges))
}

pub mod client_service;
pub mod exchange;
pub mod service;

pub use client_service::GdbClientFlightService;
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

    #[tokio::test]
    async fn test_flight_encode_decode() {
        use arrow_flight::decode::FlightRecordBatchStream;
        use arrow_flight::encode::FlightDataEncoderBuilder;
        use futures::StreamExt;

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::UInt64, false),
        ]));
        let batch = RecordBatch::try_new(schema.clone(), vec![
            Arc::new(UInt64Array::from(vec![10u64, 20, 30])),
        ]).unwrap();

        let encoder = FlightDataEncoderBuilder::new()
            .with_schema(schema)
            .build(futures::stream::once(async move { Ok(batch) }));
        let flight_data_vec: Vec<_> = encoder.collect().await;
        assert!(!flight_data_vec.is_empty());

        let stream = futures::stream::iter(flight_data_vec);
        let mut decoder = FlightRecordBatchStream::new(arrow_flight::decode::FlightDataDecoder::new(stream));
        let decoded_batch = decoder.next().await.unwrap().unwrap();
        assert_eq!(decoded_batch.num_rows(), 3);
    }

    #[tokio::test]
    async fn test_client_flight_service_do_put_and_do_get() {
        use arrow_flight::encode::FlightDataEncoderBuilder;
        use arrow_flight::Ticket;
        use futures::StreamExt;
        use gdb_core::schema::GraphSchema;
        use gdb_planner::QueryExecutor;
        use gdb_storage::PartitionStorageEngine;
        use parking_lot::RwLock;

        let schema = Arc::new(RwLock::new(GraphSchema::new("flight_test")));
        let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
        let executor = Arc::new(QueryExecutor::new(schema.clone(), storage.clone()));
        let service = GdbClientFlightService::new(storage.clone(), schema.clone(), executor.clone());

        // Create User vertex schema
        executor.execute(gdb_parser::parse("CREATE VERTEX User (name STRING, age INT64)").unwrap()).unwrap();

        // Prepare vertex Arrow RecordBatch: id, name, age
        let batch_schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::UInt64, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("age", DataType::Int64, false),
        ]));
        let batch = RecordBatch::try_new(batch_schema.clone(), vec![
            Arc::new(UInt64Array::from(vec![101u64, 102])),
            Arc::new(arrow::array::StringArray::from(vec!["Dave", "Eve"])),
            Arc::new(Int64Array::from(vec![40i64, 35])),
        ]).unwrap();

        // Run ephemeral local gRPC server on 127.0.0.1:0
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(service.into_server())
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        let mut client = arrow_flight::flight_service_client::FlightServiceClient::connect(format!("http://{}", addr))
            .await
            .unwrap();

        // 1. Ingest via DoPut
        let encoder = FlightDataEncoderBuilder::new()
            .with_schema(batch_schema)
            .build(futures::stream::once(async move { Ok(batch) }));
        let flight_stream = encoder.map(|r| r.unwrap());
        let put_resp = client.do_put(flight_stream).await.unwrap();
        let mut put_stream = put_resp.into_inner();
        let put_res = put_stream.next().await.unwrap().unwrap();
        assert!(!put_res.app_metadata.is_empty());

        // Compact into CSR to enable Cypher queries
        storage.compact();

        // 2. Query via DoGet
        let ticket = Ticket {
            ticket: "MATCH (a:User) RETURN a.name".as_bytes().to_vec().into(),
        };
        let get_resp = client.do_get(ticket).await.unwrap();
        let mut get_stream = get_resp.into_inner();
        let mut flight_batches = Vec::new();
        while let Some(item) = get_stream.next().await {
            flight_batches.push(item);
        }
        assert!(!flight_batches.is_empty());
    }
}

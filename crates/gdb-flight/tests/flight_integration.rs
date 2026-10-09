use arrow::array::{ArrayRef, Int64Array, RecordBatch, StringArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow_flight::encode::FlightDataEncoderBuilder;
use arrow_flight::flight_service_client::FlightServiceClient;
use arrow_flight::{HandshakeRequest, Ticket};
use futures::StreamExt;
use gdb_core::schema::GraphSchema;
use gdb_flight::{GdbClientFlightService, ShufflePartitioner};
use gdb_planner::QueryExecutor;
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use std::sync::Arc;

async fn setup_test_flight_server() -> (String, Arc<PartitionStorageEngine>, Arc<RwLock<GraphSchema>>, Arc<QueryExecutor>) {
    let schema = Arc::new(RwLock::new(GraphSchema::new("flight_integ_test")));
    let storage = Arc::new(PartitionStorageEngine::new(0, schema.clone()));
    let executor = Arc::new(QueryExecutor::new(schema.clone(), storage.clone()));
    let service = GdbClientFlightService::new(storage.clone(), schema.clone(), executor.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(service.into_server())
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    (format!("http://{}", addr), storage, schema, executor)
}

#[tokio::test]
async fn test_flight_handshake() {
    let (endpoint, _, _, _) = setup_test_flight_server().await;
    let mut client = FlightServiceClient::connect(endpoint).await.unwrap();

    let request_stream = futures::stream::once(async move {
        HandshakeRequest {
            protocol_version: 1,
            payload: bytes::Bytes::from("auth_token_secret"),
        }
    });

    let resp = client.handshake(request_stream).await.unwrap();
    let mut stream = resp.into_inner();
    let handshake_resp = stream.next().await.unwrap().unwrap();
    assert_eq!(handshake_resp.protocol_version, 1);
    assert_eq!(handshake_resp.payload, bytes::Bytes::from("GDB-Flight-Auth-OK"));
}

#[tokio::test]
async fn test_flight_streaming_ingest_and_query() {
    let (endpoint, storage, _schema, executor) = setup_test_flight_server().await;
    let mut client = FlightServiceClient::connect(endpoint).await.unwrap();

    // 1. Setup DDL Schema
    executor.execute(gdb_parser::parse("CREATE VERTEX Server (hostname STRING, cores INT64)").unwrap()).unwrap();
    executor.execute(gdb_parser::parse("CREATE EDGE CONNECTS (latency INT64)").unwrap()).unwrap();

    // 2. Prepare Vertex Batches (Batch 1: 50 servers, Batch 2: 50 servers)
    let v_schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::UInt64, false),
        Field::new("hostname", DataType::Utf8, false),
        Field::new("cores", DataType::Int64, false),
    ]));

    let mut ids_1 = Vec::new();
    let mut hosts_1 = Vec::new();
    let mut cores_1 = Vec::new();
    for i in 1..=50u64 {
        ids_1.push(i);
        hosts_1.push(format!("srv-{:03}.dc1.internal", i));
        cores_1.push((i % 16 + 4) as i64);
    }
    let batch_v1 = RecordBatch::try_new(
        v_schema.clone(),
        vec![
            Arc::new(UInt64Array::from(ids_1)) as ArrayRef,
            Arc::new(StringArray::from(hosts_1)) as ArrayRef,
            Arc::new(Int64Array::from(cores_1)) as ArrayRef,
        ],
    ).unwrap();

    let mut ids_2 = Vec::new();
    let mut hosts_2 = Vec::new();
    let mut cores_2 = Vec::new();
    for i in 51..=100u64 {
        ids_2.push(i);
        hosts_2.push(format!("srv-{:03}.dc2.internal", i));
        cores_2.push((i % 32 + 8) as i64);
    }
    let batch_v2 = RecordBatch::try_new(
        v_schema.clone(),
        vec![
            Arc::new(UInt64Array::from(ids_2)) as ArrayRef,
            Arc::new(StringArray::from(hosts_2)) as ArrayRef,
            Arc::new(Int64Array::from(cores_2)) as ArrayRef,
        ],
    ).unwrap();

    // Stream Vertex Batches to Flight DoPut
    let encoder_v = FlightDataEncoderBuilder::new()
        .with_schema(v_schema)
        .build(futures::stream::iter(vec![Ok(batch_v1), Ok(batch_v2)]));
    let flight_stream_v = encoder_v.map(|r| r.unwrap());
    let put_resp_v = client.do_put(flight_stream_v).await.unwrap();
    let mut put_stream_v = put_resp_v.into_inner();
    let mut put_count_v = 0;
    while let Some(res) = put_stream_v.next().await {
        assert!(res.is_ok());
        put_count_v += 1;
    }
    assert!(put_count_v >= 1);

    // 3. Prepare Edge Batches (Connecting servers in a chain: 1->2, 2->3, ..., 99->100)
    let e_schema = Arc::new(Schema::new(vec![
        Field::new("src", DataType::UInt64, false),
        Field::new("dst", DataType::UInt64, false),
        Field::new("latency", DataType::Int64, false),
    ]));

    let mut srcs = Vec::new();
    let mut dsts = Vec::new();
    let mut latencies = Vec::new();
    for i in 1..100u64 {
        srcs.push(i);
        dsts.push(i + 1);
        latencies.push((i * 2) as i64);
    }
    let batch_e = RecordBatch::try_new(
        e_schema.clone(),
        vec![
            Arc::new(UInt64Array::from(srcs)) as ArrayRef,
            Arc::new(UInt64Array::from(dsts)) as ArrayRef,
            Arc::new(Int64Array::from(latencies)) as ArrayRef,
        ],
    ).unwrap();

    let encoder_e = FlightDataEncoderBuilder::new()
        .with_schema(e_schema)
        .build(futures::stream::once(async move { Ok(batch_e) }));
    let flight_stream_e = encoder_e.map(|r| r.unwrap());
    let put_resp_e = client.do_put(flight_stream_e).await.unwrap();
    let mut put_stream_e = put_resp_e.into_inner();
    while let Some(res) = put_stream_e.next().await {
        assert!(res.is_ok());
    }

    // Compact into CSR
    storage.compact();

    // 4. Query via DoGet (Fetch all 100 servers)
    let ticket_all = Ticket {
        ticket: "MATCH (s:Server) RETURN s.hostname, s.cores".as_bytes().to_vec().into(),
    };
    let get_resp = client.do_get(ticket_all).await.unwrap();
    let mut get_stream = get_resp.into_inner();
    let mut total_flight_items = 0;
    while let Some(item) = get_stream.next().await {
        assert!(item.is_ok());
        total_flight_items += 1;
    }
    assert!(total_flight_items >= 1, "Should receive flight data frames");

    // 5. Query via DoGet with Multi-hop Traversal
    let ticket_hop = Ticket {
        ticket: "MATCH (a:Server)-[:CONNECTS]->(b:Server) RETURN a.hostname, b.hostname".as_bytes().to_vec().into(),
    };
    let get_resp_hop = client.do_get(ticket_hop).await.unwrap();
    let mut get_stream_hop = get_resp_hop.into_inner();
    let mut hop_frames = 0;
    while let Some(item) = get_stream_hop.next().await {
        assert!(item.is_ok());
        hop_frames += 1;
    }
    assert!(hop_frames >= 1);
}

#[test]
fn test_mpp_shuffle_partitioner_multi_partition() {
    let schema = Arc::new(Schema::new(vec![
        Field::new("node_id", DataType::UInt64, false),
        Field::new("metric", DataType::Int64, false),
    ]));

    let num_rows = 1000usize;
    let mut vids = Vec::with_capacity(num_rows);
    let mut metrics = Vec::with_capacity(num_rows);
    for i in 0..num_rows {
        vids.push(i as u64);
        metrics.push((i * 10) as i64);
    }

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(UInt64Array::from(vids)) as ArrayRef,
            Arc::new(Int64Array::from(metrics)) as ArrayRef,
        ],
    ).unwrap();

    // Test with 4 partitions
    let partitioner_4 = ShufflePartitioner::new(4);
    let partitioned_4 = partitioner_4.partition_batch(&batch, 0).unwrap();
    assert_eq!(partitioned_4.len(), 4);
    let mut total_count_4 = 0;
    for (part_id, part_batch) in &partitioned_4 {
        assert!(*part_id < 4);
        total_count_4 += part_batch.num_rows();
        assert_eq!(part_batch.num_rows(), 250);
    }
    assert_eq!(total_count_4, num_rows);

    // Test with 8 partitions
    let partitioner_8 = ShufflePartitioner::new(8);
    let partitioned_8 = partitioner_8.partition_batch(&batch, 0).unwrap();
    assert_eq!(partitioned_8.len(), 8);
    let mut total_count_8 = 0;
    for (part_id, part_batch) in &partitioned_8 {
        assert!(*part_id < 8);
        total_count_8 += part_batch.num_rows();
        assert_eq!(part_batch.num_rows(), 125);
    }
    assert_eq!(total_count_8, num_rows);
}

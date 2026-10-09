use arrow_flight::flight_service_client::FlightServiceClient;
use arrow_flight::{Action, Criteria, FlightDescriptor, Ticket};
use futures::StreamExt;
use gdb_flight::GdbFlightService;

async fn setup_gdb_flight_server() -> String {
    let service = GdbFlightService::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(service.into_server())
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    format!("http://{}", addr)
}

#[tokio::test]
async fn test_gdb_flight_service_grpc_calls() {
    let endpoint = setup_gdb_flight_server().await;
    let mut client = FlightServiceClient::connect(endpoint).await.unwrap();

    // 1. do_get
    let ticket = Ticket {
        ticket: "MATCH (n) RETURN n".as_bytes().to_vec().into(),
    };
    let resp = client.do_get(ticket).await.unwrap();
    let mut stream = resp.into_inner();
    assert!(stream.next().await.is_none());

    // 2. Unimplemented methods return gRPC error status
    let res_flights = client.list_flights(Criteria::default()).await;
    assert!(res_flights.is_err());

    let res_info = client.get_flight_info(FlightDescriptor::default()).await;
    assert!(res_info.is_err());

    let res_action = client.do_action(Action::default()).await;
    assert!(res_action.is_err());

    let res_actions = client.list_actions(arrow_flight::Empty {}).await;
    assert!(res_actions.is_err());

    let res_schema = client.get_schema(FlightDescriptor::default()).await;
    assert!(res_schema.is_err());

    let res_put = client.do_put(futures::stream::empty()).await;
    assert!(res_put.is_err());

    let res_exchange = client.do_exchange(futures::stream::empty()).await;
    assert!(res_exchange.is_ok());
}

#[tokio::test]
async fn test_gdb_client_flight_service_unimplemented_and_errors() {
    let schema = std::sync::Arc::new(parking_lot::RwLock::new(gdb_core::schema::GraphSchema::new("test")));
    let storage = std::sync::Arc::new(gdb_storage::PartitionStorageEngine::new(0, schema.clone()));
    let executor = std::sync::Arc::new(gdb_planner::QueryExecutor::new(schema.clone(), storage.clone()));
    let service = gdb_flight::GdbClientFlightService::new(storage, schema, executor.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(service.into_server())
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    let mut client = FlightServiceClient::connect(format!("http://{}", addr)).await.unwrap();

    // 1. Handshake
    let handshake_stream = client.handshake(futures::stream::empty()).await.unwrap();
    let mut hs_in = handshake_stream.into_inner();
    let hs_res = hs_in.next().await.unwrap().unwrap();
    assert_eq!(hs_res.protocol_version, 1);

    // 2. Unimplemented methods
    assert!(client.list_flights(Criteria::default()).await.is_err());
    assert!(client.get_flight_info(FlightDescriptor::default()).await.is_err());
    assert!(client.do_action(Action::default()).await.is_err());
    assert!(client.list_actions(arrow_flight::Empty {}).await.is_err());
    assert!(client.get_schema(FlightDescriptor::default()).await.is_err());

    // 3. do_get with invalid UTF-8
    let bad_ticket = Ticket {
        ticket: vec![0xFF, 0xFE, 0xFD].into(),
    };
    assert!(client.do_get(bad_ticket).await.is_err());

    // 4. do_get with syntax error
    let err_ticket = Ticket {
        ticket: "INVALID SYNTAX STATEMENT".as_bytes().to_vec().into(),
    };
    assert!(client.do_get(err_ticket).await.is_err());

    // 5. do_get with valid query returning data
    executor.execute(gdb_parser::parse("CREATE VERTEX User (name STRING);").unwrap()).unwrap();
    executor.execute(gdb_parser::parse("INSERT VERTEX User (id, name) VALUES (1, 'Eve');").unwrap()).unwrap();

    let query_ticket = Ticket {
        ticket: "MATCH (u:User) RETURN u.name;".as_bytes().to_vec().into(),
    };
    let flight_stream = client.do_get(query_ticket).await.unwrap().into_inner();
    let items: Vec<_> = flight_stream.collect().await;
    assert!(!items.is_empty());

    // 6. do_get with empty query
    let empty_ticket = Ticket {
        ticket: "CREATE EDGE FOO ();".as_bytes().to_vec().into(),
    };
    let mut empty_stream = client.do_get(empty_ticket).await.unwrap().into_inner();
    assert!(empty_stream.next().await.is_none());

    // 7. do_exchange echoes stream
    let ex_resp = client.do_exchange(futures::stream::empty()).await.unwrap();
    let mut ex_stream = ex_resp.into_inner();
    assert!(ex_stream.next().await.is_none());
}


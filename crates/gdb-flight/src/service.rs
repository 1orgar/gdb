use arrow::array::RecordBatch;
use arrow_flight::decode::{FlightDataDecoder, FlightRecordBatchStream};
use arrow_flight::encode::FlightDataEncoderBuilder;
use arrow_flight::flight_service_server::{FlightService, FlightServiceServer};
use arrow_flight::{
    Action, ActionType, Criteria, Empty, FlightData, FlightDescriptor, FlightInfo,
    HandshakeRequest, HandshakeResponse, PutResult, SchemaResult, Ticket,
};
use bytes::Bytes;
use futures::{Stream, StreamExt};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tonic::{Request, Response, Status, Streaming};

/// Internal Cluster Arrow Flight Service for high-speed binary shuffle and inter-node exchange.
pub struct GdbFlightService {
    exchange_buffers: Arc<RwLock<HashMap<String, Vec<RecordBatch>>>>,
}

impl Default for GdbFlightService {
    fn default() -> Self {
        Self::new()
    }
}

impl GdbFlightService {
    pub fn new() -> Self {
        Self {
            exchange_buffers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn into_server(self) -> FlightServiceServer<Self> {
        FlightServiceServer::new(self)
            .max_decoding_message_size(256 * 1024 * 1024)
            .max_encoding_message_size(256 * 1024 * 1024)
    }
}

#[tonic::async_trait]
impl FlightService for GdbFlightService {
    type HandshakeStream =
        Pin<Box<dyn Stream<Item = Result<HandshakeResponse, Status>> + Send + 'static>>;
    type ListFlightsStream =
        Pin<Box<dyn Stream<Item = Result<FlightInfo, Status>> + Send + 'static>>;
    type DoGetStream = Pin<Box<dyn Stream<Item = Result<FlightData, Status>> + Send + 'static>>;
    type DoPutStream = Pin<Box<dyn Stream<Item = Result<PutResult, Status>> + Send + 'static>>;
    type DoActionStream =
        Pin<Box<dyn Stream<Item = Result<arrow_flight::Result, Status>> + Send + 'static>>;
    type ListActionsStream =
        Pin<Box<dyn Stream<Item = Result<ActionType, Status>> + Send + 'static>>;
    type DoExchangeStream =
        Pin<Box<dyn Stream<Item = Result<FlightData, Status>> + Send + 'static>>;

    async fn handshake(
        &self,
        _request: Request<Streaming<HandshakeRequest>>,
    ) -> Result<Response<Self::HandshakeStream>, Status> {
        Err(Status::unimplemented("handshake"))
    }

    async fn list_flights(
        &self,
        _request: Request<Criteria>,
    ) -> Result<Response<Self::ListFlightsStream>, Status> {
        Err(Status::unimplemented("list_flights"))
    }

    async fn get_flight_info(
        &self,
        _request: Request<FlightDescriptor>,
    ) -> Result<Response<FlightInfo>, Status> {
        Err(Status::unimplemented("get_flight_info"))
    }

    async fn poll_flight_info(
        &self,
        _request: Request<FlightDescriptor>,
    ) -> Result<Response<arrow_flight::PollInfo>, Status> {
        Err(Status::unimplemented("poll_flight_info"))
    }

    async fn get_schema(
        &self,
        _request: Request<FlightDescriptor>,
    ) -> Result<Response<SchemaResult>, Status> {
        Err(Status::unimplemented("get_schema"))
    }

    async fn do_get(
        &self,
        request: Request<Ticket>,
    ) -> Result<Response<Self::DoGetStream>, Status> {
        let ticket = request.into_inner();
        let key = String::from_utf8(ticket.ticket.to_vec())
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let batches_opt = self.exchange_buffers.read().get(&key).cloned();
        if let Some(batches) = batches_opt {
            if let Some(first) = batches.first() {
                let schema = first.schema();
                let batch_stream = futures::stream::iter(batches.into_iter().map(Ok));
                let flight_stream = FlightDataEncoderBuilder::new()
                    .with_schema(schema)
                    .build(batch_stream)
                    .map(|res| res.map_err(|e| Status::internal(e.to_string())));
                return Ok(Response::new(Box::pin(flight_stream)));
            }
        }

        let output = futures::stream::empty();
        Ok(Response::new(Box::pin(output)))
    }

    async fn do_put(
        &self,
        request: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoPutStream>, Status> {
        let mut in_stream = request.into_inner().peekable();
        let mut key = "default".to_string();

        match Pin::new(&mut in_stream).peek().await {
            Some(Ok(first_msg)) => {
                if let Some(ref desc) = first_msg.flight_descriptor {
                    if !desc.cmd.is_empty() {
                        if let Ok(s) = String::from_utf8(desc.cmd.to_vec()) {
                            key = s;
                        }
                    } else if let Some(first_path) = desc.path.first() {
                        key = first_path.clone();
                    }
                }
            }
            Some(Err(e)) => return Err(e.clone()),
            None => return Err(Status::invalid_argument("Empty DoPut stream")),
        }

        let mapped_stream =
            in_stream.map(|res| res.map_err(arrow_flight::error::FlightError::Tonic));
        let mut record_batch_stream =
            FlightRecordBatchStream::new(FlightDataDecoder::new(mapped_stream));

        let mut stored_batches = Vec::new();
        let mut total_rows = 0usize;
        while let Some(batch_res) = record_batch_stream.next().await {
            let batch = batch_res.map_err(|e| Status::internal(e.to_string()))?;
            total_rows += batch.num_rows();
            stored_batches.push(batch);
        }

        self.exchange_buffers.write().insert(key, stored_batches);

        let result = PutResult {
            app_metadata: Bytes::from(format!("Stored {} rows", total_rows)),
        };
        let out = futures::stream::once(async move { Ok(result) });
        Ok(Response::new(Box::pin(out)))
    }

    async fn do_action(
        &self,
        _request: Request<Action>,
    ) -> Result<Response<Self::DoActionStream>, Status> {
        Err(Status::unimplemented("do_action"))
    }

    async fn list_actions(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::ListActionsStream>, Status> {
        Err(Status::unimplemented("list_actions"))
    }

    async fn do_exchange(
        &self,
        request: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoExchangeStream>, Status> {
        let in_stream = request.into_inner();
        Ok(Response::new(Box::pin(in_stream)))
    }
}

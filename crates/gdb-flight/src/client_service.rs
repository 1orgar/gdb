use arrow::array::{Array, AsArray};
use arrow::datatypes::DataType;
use arrow_flight::decode::{FlightDataDecoder, FlightRecordBatchStream};
use arrow_flight::encode::FlightDataEncoderBuilder;
use arrow_flight::flight_service_server::{FlightService, FlightServiceServer};
use arrow_flight::{
    Action, ActionType, Criteria, Empty, FlightData, FlightDescriptor, FlightInfo,
    HandshakeRequest, HandshakeResponse, PutResult, SchemaResult, Ticket,
};
use futures::{Stream, StreamExt};
use gdb_core::schema::GraphSchema;
use gdb_core::{DataValue, EdgeId, EdgeType, LabelId, VertexId};
use gdb_planner::QueryExecutor;
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tonic::{Request, Response, Status, Streaming};

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
struct IngestDescriptor {
    #[serde(default)]
    r#type: String, // "vertex" or "edge"
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    edge_type: Option<String>,
}

/// External Client Arrow Flight Service for fast streaming ingestion and queries.
pub struct GdbClientFlightService {
    storage: Arc<PartitionStorageEngine>,
    schema: Arc<RwLock<GraphSchema>>,
    executor: Arc<QueryExecutor>,
}

impl GdbClientFlightService {
    pub fn new(
        storage: Arc<PartitionStorageEngine>,
        schema: Arc<RwLock<GraphSchema>>,
        executor: Arc<QueryExecutor>,
    ) -> Self {
        Self {
            storage,
            schema,
            executor,
        }
    }

    pub fn into_server(self) -> FlightServiceServer<Self> {
        FlightServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl FlightService for GdbClientFlightService {
    type HandshakeStream = Pin<Box<dyn Stream<Item = Result<HandshakeResponse, Status>> + Send + 'static>>;
    type ListFlightsStream = Pin<Box<dyn Stream<Item = Result<FlightInfo, Status>> + Send + 'static>>;
    type DoGetStream = Pin<Box<dyn Stream<Item = Result<FlightData, Status>> + Send + 'static>>;
    type DoPutStream = Pin<Box<dyn Stream<Item = Result<PutResult, Status>> + Send + 'static>>;
    type DoActionStream = Pin<Box<dyn Stream<Item = Result<arrow_flight::Result, Status>> + Send + 'static>>;
    type ListActionsStream = Pin<Box<dyn Stream<Item = Result<ActionType, Status>> + Send + 'static>>;
    type DoExchangeStream = Pin<Box<dyn Stream<Item = Result<FlightData, Status>> + Send + 'static>>;

    async fn handshake(
        &self,
        _request: Request<Streaming<HandshakeRequest>>,
    ) -> Result<Response<Self::HandshakeStream>, Status> {
        let resp = HandshakeResponse {
            protocol_version: 1,
            payload: bytes::Bytes::from("GDB-Flight-Auth-OK"),
        };
        Ok(Response::new(Box::pin(futures::stream::once(async move { Ok(resp) }))))
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

    /// Fast openCypher query execution returning Arrow RecordBatches directly via Flight streams.
    async fn do_get(
        &self,
        request: Request<Ticket>,
    ) -> Result<Response<Self::DoGetStream>, Status> {
        let ticket = request.into_inner();
        let query_str = String::from_utf8(ticket.ticket.to_vec())
            .map_err(|e| Status::invalid_argument(format!("Invalid ticket UTF-8: {}", e)))?;

        let parsed = gdb_parser::parse(&query_str)
            .map_err(|e| Status::invalid_argument(format!("Query parse error: {}", e)))?;

        let query_result = self.executor.execute(parsed)
            .map_err(|e| Status::internal(format!("Query execution error: {}", e)))?;

        if let Some(batch) = query_result.batch {
            let schema = batch.schema();
            let encoder = FlightDataEncoderBuilder::new()
                .with_schema(schema)
                .build(futures::stream::once(async move { Ok(batch) }));
            let mapped = encoder.map(|res| res.map_err(|e| Status::internal(e.to_string())));
            Ok(Response::new(Box::pin(mapped)))
        } else {
            Ok(Response::new(Box::pin(futures::stream::empty())))
        }
    }

    /// High-throughput streaming ingest of Arrow RecordBatches directly into DeltaMemTable.
    async fn do_put(
        &self,
        request: Request<Streaming<FlightData>>,
    ) -> Result<Response<Self::DoPutStream>, Status> {
        let in_stream = request.into_inner();
        let storage = self.storage.clone();
        let schema = self.schema.clone();

        let (tx, rx) = tokio::sync::mpsc::channel(4);

        tokio::spawn(async move {
            let mapped_stream = in_stream.map(|res| res.map_err(arrow_flight::error::FlightError::Tonic));
            let mut batch_stream = FlightRecordBatchStream::new(FlightDataDecoder::new(mapped_stream));

            let mut total_rows = 0usize;
            let mut is_edge: Option<bool> = None;
            let mut label_or_type: Option<String> = None;

            while let Some(batch_res) = batch_stream.next().await {
                let batch = match batch_res {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(Err(Status::internal(format!("Flight decode error: {}", e)))).await;
                        return;
                    }
                };
                let batch_schema = batch.schema();

                if is_edge.is_none() {
                    let field_names: Vec<&str> = batch_schema.fields().iter().map(|f| f.name().as_str()).collect();
                    if field_names.contains(&"src") && field_names.contains(&"dst") {
                        is_edge = Some(true);
                        label_or_type = Some("EDGE".into());
                    } else {
                        is_edge = Some(false);
                        label_or_type = Some("VERTEX".into());
                    }
                }

                let num_rows = batch.num_rows();
                if num_rows == 0 {
                    continue;
                }

                if is_edge == Some(true) {
                    let src_col = match batch.column_by_name("src") {
                        Some(c) => c,
                        None => {
                            let _ = tx.send(Err(Status::invalid_argument("Missing 'src' column"))).await;
                            return;
                        }
                    };
                    let dst_col = match batch.column_by_name("dst") {
                        Some(c) => c,
                        None => {
                            let _ = tx.send(Err(Status::invalid_argument("Missing 'dst' column"))).await;
                            return;
                        }
                    };
                    let rank_col = batch.column_by_name("rank");
                    let edge_type_col = batch.column_by_name("edge_type");

                    let commit_ver = storage.next_commit_version();

                    for i in 0..num_rows {
                        let src_id = match extract_vid(src_col, i) {
                            Ok(v) => v,
                            Err(e) => {
                                let _ = tx.send(Err(e)).await;
                                return;
                            }
                        };
                        let dst_id = match extract_vid(dst_col, i) {
                            Ok(v) => v,
                            Err(e) => {
                                let _ = tx.send(Err(e)).await;
                                return;
                            }
                        };
                        let rank = if let Some(rc) = rank_col {
                            extract_i64(rc, i).unwrap_or(0)
                        } else {
                            0
                        };

                        let et_id = if let Some(etc) = edge_type_col {
                            let et_name = extract_string(etc, i).unwrap_or_else(|| "DEFAULT".into());
                            let mut sc = schema.write();
                            sc.register_edge_type(&et_name, vec![]).unwrap_or(EdgeType(1))
                        } else {
                            EdgeType(1)
                        };

                        let edge = EdgeId::new(src_id, et_id, rank, dst_id);
                        storage.insert_edge(edge, commit_ver);
                    }
                } else {
                    let id_col = match batch.column_by_name("id").or_else(|| batch.column_by_name("_id")) {
                        Some(c) => c,
                        None => {
                            let _ = tx.send(Err(Status::invalid_argument("Missing 'id' column"))).await;
                            return;
                        }
                    };

                    let label_col = batch.column_by_name("label");
                    let prop_cols: Vec<(String, &arrow::array::ArrayRef)> = batch_schema
                        .fields()
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| f.name() != "id" && f.name() != "_id" && f.name() != "label")
                        .map(|(idx, f)| (f.name().clone(), batch.column(idx)))
                        .collect();

                    let default_label = label_or_type.clone().unwrap_or_else(|| "User".into());

                    for i in 0..num_rows {
                        let vid = match extract_vid(id_col, i) {
                            Ok(v) => v,
                            Err(e) => {
                                let _ = tx.send(Err(e)).await;
                                return;
                            }
                        };
                        let label_name = if let Some(lc) = label_col {
                            extract_string(lc, i).unwrap_or_else(|| default_label.clone())
                        } else {
                            default_label.clone()
                        };

                        let label_id = {
                            let mut sg = schema.write();
                            sg.register_vertex_label(&label_name, vec![]).unwrap_or(LabelId(1))
                        };

                        let mut props = HashMap::new();
                        for (prop_name, col) in &prop_cols {
                            if let Some(val) = extract_data_value(col, i) {
                                props.insert(prop_name.clone(), val);
                            }
                        }

                        let _ = storage.set_vertex_properties(vid, label_id, props);
                    }
                }

                total_rows += num_rows;
            }

            let resp_payload = serde_json::json!({
                "status": "ok",
                "rows_affected": total_rows,
            });

            let put_res = PutResult {
                app_metadata: bytes::Bytes::from(resp_payload.to_string()),
            };

            let _ = tx.send(Ok(put_res)).await;
        });

        let out_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(out_stream)))
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

// Extraction helpers
fn extract_vid(col: &arrow::array::ArrayRef, idx: usize) -> Result<VertexId, Status> {
    match col.data_type() {
        DataType::UInt64 => {
            let arr = col.as_primitive::<arrow::datatypes::UInt64Type>();
            Ok(VertexId(arr.value(idx)))
        }
        DataType::Int64 => {
            let arr = col.as_primitive::<arrow::datatypes::Int64Type>();
            Ok(VertexId(arr.value(idx) as u64))
        }
        DataType::UInt32 => {
            let arr = col.as_primitive::<arrow::datatypes::UInt32Type>();
            Ok(VertexId(arr.value(idx) as u64))
        }
        DataType::Int32 => {
            let arr = col.as_primitive::<arrow::datatypes::Int32Type>();
            Ok(VertexId(arr.value(idx) as u64))
        }
        DataType::Utf8 => {
            let arr = col.as_string::<i32>();
            Ok(VertexId::from_str_key(arr.value(idx)))
        }
        DataType::LargeUtf8 => {
            let arr = col.as_string::<i64>();
            Ok(VertexId::from_str_key(arr.value(idx)))
        }
        other => Err(Status::invalid_argument(format!(
            "Unsupported id column type: {:?}",
            other
        ))),
    }
}

fn extract_i64(col: &arrow::array::ArrayRef, idx: usize) -> Option<i64> {
    match col.data_type() {
        DataType::Int64 => Some(col.as_primitive::<arrow::datatypes::Int64Type>().value(idx)),
        DataType::Int32 => Some(col.as_primitive::<arrow::datatypes::Int32Type>().value(idx) as i64),
        DataType::UInt64 => Some(col.as_primitive::<arrow::datatypes::UInt64Type>().value(idx) as i64),
        DataType::UInt32 => Some(col.as_primitive::<arrow::datatypes::UInt32Type>().value(idx) as i64),
        _ => None,
    }
}

fn extract_string(col: &arrow::array::ArrayRef, idx: usize) -> Option<String> {
    match col.data_type() {
        DataType::Utf8 => Some(col.as_string::<i32>().value(idx).to_string()),
        DataType::LargeUtf8 => Some(col.as_string::<i64>().value(idx).to_string()),
        _ => None,
    }
}

fn extract_data_value(col: &arrow::array::ArrayRef, idx: usize) -> Option<DataValue> {
    if col.is_null(idx) {
        return Some(DataValue::Null);
    }
    match col.data_type() {
        DataType::Int64 => Some(DataValue::Int64(
            col.as_primitive::<arrow::datatypes::Int64Type>().value(idx),
        )),
        DataType::Int32 => Some(DataValue::Int64(
            col.as_primitive::<arrow::datatypes::Int32Type>().value(idx) as i64,
        )),
        DataType::Float64 => Some(DataValue::Float64(
            col.as_primitive::<arrow::datatypes::Float64Type>().value(idx),
        )),
        DataType::Float32 => Some(DataValue::Float64(
            col.as_primitive::<arrow::datatypes::Float32Type>().value(idx) as f64,
        )),
        DataType::Utf8 => Some(DataValue::String(
            col.as_string::<i32>().value(idx).to_string(),
        )),
        DataType::LargeUtf8 => Some(DataValue::String(
            col.as_string::<i64>().value(idx).to_string(),
        )),
        DataType::Boolean => Some(DataValue::Boolean(
            col.as_boolean().value(idx),
        )),
        _ => None,
    }
}

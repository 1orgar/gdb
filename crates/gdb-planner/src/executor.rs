use gdb_analytics::AnalyticsEngine;
use crate::eval::{eval_expr, PathRow};
use crate::plan::PhysicalOperator;
use arrow::array::{ArrayRef, Int64Array, RecordBatch};
use arrow::datatypes::{DataType as ArrowDataType, Field, Schema as ArrowSchema};
use gdb_core::schema::GraphSchema;
use gdb_core::{DataValue, EdgeId, GdbError, GdbResult, LabelId, VertexId};
use gdb_parser::ast::{CypherQuery, Expr, ReturnItem, Statement};
use gdb_storage::PartitionStorageEngine;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

pub struct QueryResult {
    pub message: String,
    pub batch: Option<RecordBatch>,
    pub rows_affected: usize,
}

pub struct QueryExecutor {
    pub schema: Arc<RwLock<GraphSchema>>,
    pub storage: Arc<PartitionStorageEngine>,
}

impl QueryExecutor {
    pub fn new(schema: Arc<RwLock<GraphSchema>>, storage: Arc<PartitionStorageEngine>) -> Self {
        Self { schema, storage }
    }

    pub fn execute(&self, stmt: Statement) -> GdbResult<QueryResult> {
        match stmt {
            Statement::CreateVertexLabel { label, properties } => {
                let mut schema = self.schema.write();
                let label_id = schema.register_vertex_label(&label, properties)?;
                Ok(QueryResult {
                    message: format!("Created vertex label '{}' ({:?})", label, label_id),
                    batch: None,
                    rows_affected: 0,
                })
            }
            Statement::CreateEdgeType { edge_type, properties } => {
                let mut schema = self.schema.write();
                let et = schema.register_edge_type(&edge_type, properties)?;
                Ok(QueryResult {
                    message: format!("Created edge type '{}' ({:?})", edge_type, et),
                    batch: None,
                    rows_affected: 0,
                })
            }
            Statement::InsertVertex { label, id, properties } => {
                let label_id = {
                    let schema = self.schema.read();
                    schema
                        .get_vertex_schema(&label)
                        .map(|s| s.label_id)
                        .ok_or_else(|| GdbError::Schema(format!("Unknown vertex label: {}", label)))?
                };

                let props_map: HashMap<String, DataValue> = properties.into_iter().collect();
                self.storage.set_vertex_properties(id, label_id, props_map)?;
                Ok(QueryResult {
                    message: format!("Inserted vertex {}", id),
                    batch: None,
                    rows_affected: 1,
                })
            }
            Statement::InsertEdge { edge_type, src, dst, rank, .. } => {
                let et = {
                    let schema = self.schema.read();
                    schema
                        .get_edge_schema(&edge_type)
                        .map(|s| s.edge_type)
                        .ok_or_else(|| GdbError::Schema(format!("Unknown edge type: {}", edge_type)))?
                };

                let ver = self.storage.next_commit_version();
                let edge = EdgeId::new(src, et, rank, dst);
                self.storage.insert_edge(edge, ver);
                Ok(QueryResult {
                    message: format!("Inserted edge {:?}", edge),
                    batch: None,
                    rows_affected: 1,
                })
            }
            Statement::DeleteEdge { edge_type, src, dst, rank } => {
                let et = {
                    let schema = self.schema.read();
                    schema
                        .get_edge_schema(&edge_type)
                        .map(|s| s.edge_type)
                        .ok_or_else(|| GdbError::Schema(format!("Unknown edge type: {}", edge_type)))?
                };

                let ver = self.storage.next_commit_version();
                let edge = EdgeId::new(src, et, rank, dst);
                self.storage.delete_edge(edge, ver);
                Ok(QueryResult {
                    message: format!("Deleted edge {:?}", edge),
                    batch: None,
                    rows_affected: 1,
                })
            }
            Statement::Query(query) => self.execute_cypher(query),
            Statement::CallAlgorithm { algorithm, args, yield_items } => {
                self.execute_call(&algorithm, args, yield_items)
            }
        }
    }

    fn execute_cypher(&self, query: CypherQuery) -> GdbResult<QueryResult> {
        let plan = self.create_physical_plan(&query)?;
        let snapshot = self.storage.latest_version();
        let rows = self.execute_plan(&plan, snapshot)?;

        // Build result RecordBatch
        let batch = self.build_record_batch(&query.return_items, &rows)?;
        let count = batch.as_ref().map(|b| b.num_rows()).unwrap_or(0);

        Ok(QueryResult {
            message: format!("Query completed, {} rows returned", count),
            batch,
            rows_affected: count,
        })
    }

    fn create_physical_plan(&self, query: &CypherQuery) -> GdbResult<PhysicalOperator> {
        let schema = self.schema.read();
        let start = &query.pattern.start_node;
        let start_var = start.variable.clone().unwrap_or_else(|| "_start".into());

        let label_id = if let Some(ref l) = start.label {
            schema.get_vertex_schema(l).map(|s| s.label_id)
                .ok_or_else(|| GdbError::Schema(format!("Label not found: {}", l)))?
        } else {
            LabelId(1)
        };

        let mut current_plan = PhysicalOperator::ScanVertices {
            var_name: start_var.clone(),
            label_id,
            id_filter: start.id_filter,
        };

        let mut prev_var = start_var;
        for (edge_pat, node_pat) in &query.pattern.hops {
            let dst_var = node_pat.variable.clone().unwrap_or_else(|| "_dst".into());
            let edge_var = edge_pat.variable.clone();

            let edge_type = if let Some(ref et_name) = edge_pat.edge_type {
                Some(
                    schema.get_edge_schema(et_name).map(|s| s.edge_type)
                        .ok_or_else(|| GdbError::Schema(format!("Edge type not found: {}", et_name)))?
                )
            } else {
                None
            };

            current_plan = PhysicalOperator::ExpandEdges {
                input: Box::new(current_plan),
                src_var: prev_var,
                edge_var,
                dst_var: dst_var.clone(),
                edge_type,
            };

            prev_var = dst_var;
        }

        if let Some(ref where_expr) = query.where_clause {
            current_plan = PhysicalOperator::Filter {
                input: Box::new(current_plan),
                predicate: where_expr.clone(),
            };
        }

        if let Some(limit) = query.limit {
            current_plan = PhysicalOperator::Limit {
                input: Box::new(current_plan),
                limit,
            };
        }

        Ok(current_plan)
    }

    fn execute_plan(&self, plan: &PhysicalOperator, snapshot: u64) -> GdbResult<Vec<PathRow>> {
        match plan {
            PhysicalOperator::ScanVertices { var_name, label_id, id_filter } => {
                let mut rows = Vec::new();
                if let Some(target_id) = id_filter {
                    let mut row = PathRow::default();
                    row.vertices.insert(var_name.clone(), (*target_id, *label_id));
                    rows.push(row);
                } else {
                    // Fetch all known vertices in CSR and Delta
                    let csr = self.storage.current_csr();
                    for &vid_raw in &csr.reverse_map {
                        let mut row = PathRow::default();
                        row.vertices.insert(var_name.clone(), (VertexId(vid_raw), *label_id));
                        rows.push(row);
                    }
                }
                Ok(rows)
            }
            PhysicalOperator::ExpandEdges { input, src_var, edge_var, dst_var, edge_type } => {
                let input_rows = self.execute_plan(input, snapshot)?;
                let mut output_rows = Vec::new();

                for row in input_rows {
                    if let Some(&(src_vid, _)) = row.vertices.get(src_var) {
                        let edges = self.storage.get_out_edges(src_vid, *edge_type, snapshot);
                        for edge in edges {
                            let mut new_row = row.clone();
                            if let Some(ev) = edge_var {
                                new_row.edges.insert(ev.clone(), edge);
                            }
                            // Default to LabelId(1) for target if unknown
                            new_row.vertices.insert(dst_var.clone(), (edge.dst, LabelId(1)));
                            output_rows.push(new_row);
                        }
                    }
                }

                Ok(output_rows)
            }
            PhysicalOperator::Filter { input, predicate } => {
                let input_rows = self.execute_plan(input, snapshot)?;
                let mut filtered = Vec::new();
                for row in input_rows {
                    let res = eval_expr(predicate, &row, &self.storage)?;
                    if let DataValue::Boolean(true) = res {
                        filtered.push(row);
                    }
                }
                Ok(filtered)
            }
            PhysicalOperator::Limit { input, limit } => {
                let input_rows = self.execute_plan(input, snapshot)?;
                Ok(input_rows.into_iter().take(*limit).collect())
            }
            PhysicalOperator::Project { input, .. } => {
                self.execute_plan(input, snapshot)
            }
        }
    }


    fn execute_call(
        &self,
        algorithm: &str,
        args: HashMap<String, DataValue>,
        yield_items: Vec<String>,
    ) -> GdbResult<QueryResult> {
        let csr = self.storage.current_csr();
        let algo_lower = algorithm.to_lowercase();

        let batch = match algo_lower.as_str() {
            "algo.pagerank" => {
                let damping = match args.get("damping") {
                    Some(DataValue::Float64(f)) => *f,
                    Some(DataValue::Int64(i)) => *i as f64,
                    _ => 0.85,
                };
                let max_iter = match args.get("max_iter") {
                    Some(DataValue::Int64(i)) => *i as usize,
                    _ => 20,
                };
                let tolerance = match args.get("tolerance") {
                    Some(DataValue::Float64(f)) => *f,
                    _ => 1e-5,
                };
                AnalyticsEngine::run_pagerank(&csr, damping, max_iter, tolerance)?
            }
            "algo.wcc" => AnalyticsEngine::run_wcc(&csr)?,
            "algo.scc" => AnalyticsEngine::run_scc(&csr)?,
            "algo.louvain" => {
                let max_iter = match args.get("max_iter") {
                    Some(DataValue::Int64(i)) => *i as usize,
                    _ => 10,
                };
                AnalyticsEngine::run_louvain(&csr, max_iter)?
            }
            "algo.lpa" => {
                let max_iter = match args.get("max_iter") {
                    Some(DataValue::Int64(i)) => *i as usize,
                    _ => 10,
                };
                AnalyticsEngine::run_lpa(&csr, max_iter)?
            }
            "algo.kcore" => AnalyticsEngine::run_kcore(&csr)?,
            "algo.trianglecount" | "algo.triangles" => AnalyticsEngine::run_triangle_count(&csr)?,
            "algo.betweenness" => {
                let norm = match args.get("normalized") {
                    Some(DataValue::Boolean(b)) => *b,
                    _ => true,
                };
                AnalyticsEngine::run_betweenness(&csr, norm)?
            }
            "algo.closeness" => AnalyticsEngine::run_closeness(&csr)?,
            "algo.degree" => AnalyticsEngine::run_degree(&csr)?,
            "algo.sssp" => {
                let source = match args.get("source") {
                    Some(DataValue::Int64(i)) => VertexId(*i as u64),
                    _ => VertexId(1),
                };
                AnalyticsEngine::run_sssp(&csr, source)?
            }
            "algo.similarity" | "algo.jaccard" => {
                let node1 = match args.get("node1") {
                    Some(DataValue::Int64(i)) => VertexId(*i as u64),
                    _ => VertexId(1),
                };
                let node2 = match args.get("node2") {
                    Some(DataValue::Int64(i)) => VertexId(*i as u64),
                    _ => VertexId(2),
                };
                AnalyticsEngine::run_similarity(&csr, node1, node2)?
            }
            other => return Err(GdbError::Execution(format!("Unknown graph algorithm: {}", other))),
        };

        // If yield_items is provided, project columns
        let final_batch = if !yield_items.is_empty() {
            let schema = batch.schema();
            let mut proj_indices = Vec::new();
            for item in &yield_items {
                if let Ok(idx) = schema.index_of(item) {
                    proj_indices.push(idx);
                }
            }
            if !proj_indices.is_empty() {
                batch.project(&proj_indices)?
            } else {
                batch
            }
        } else {
            batch
        };

        let num_rows = final_batch.num_rows();
        Ok(QueryResult {
            message: format!("Algorithm {} completed ({} rows)", algorithm, num_rows),
            batch: Some(final_batch),
            rows_affected: num_rows,
        })
    }

    fn build_record_batch(
        &self,
        return_items: &[ReturnItem],
        rows: &[PathRow],
    ) -> GdbResult<Option<RecordBatch>> {
        if return_items.is_empty() || rows.is_empty() {
            return Ok(None);
        }

        // Special case: check if COUNT(*)
        if return_items.len() == 1 && matches!(return_items[0].expr, Expr::CountStar) {
            let field = Field::new("count", ArrowDataType::Int64, false);
            let schema = Arc::new(ArrowSchema::new(vec![field]));
            let count_arr = Arc::new(Int64Array::from(vec![rows.len() as i64]));
            let batch = RecordBatch::try_new(schema, vec![count_arr])?;
            return Ok(Some(batch));
        }

        let mut fields = Vec::with_capacity(return_items.len());
        let mut column_values: Vec<Vec<DataValue>> = vec![Vec::with_capacity(rows.len()); return_items.len()];

        for (col_idx, item) in return_items.iter().enumerate() {
            let col_name = item.alias.clone().unwrap_or_else(|| match &item.expr {
                Expr::Property { variable, property } => format!("{}.{}", variable, property),
                Expr::Variable(v) => v.clone(),
                Expr::Literal(l) => l.to_string(),
                _ => format!("col_{}", col_idx),
            });

            for row in rows {
                let val = eval_expr(&item.expr, row, &self.storage)?;
                column_values[col_idx].push(val);
            }

            // Determine data type from first non-null value
            let arrow_type = column_values[col_idx]
                .iter()
                .find(|v| **v != DataValue::Null)
                .map(|v| v.data_type())
                .unwrap_or(ArrowDataType::Utf8);

            fields.push(Field::new(col_name, arrow_type, true));
        }

        let arrow_schema = Arc::new(ArrowSchema::new(fields));
        let mut columns: Vec<ArrayRef> = Vec::with_capacity(return_items.len());

        for (col_idx, field) in arrow_schema.fields().iter().enumerate() {
            let vals = &column_values[col_idx];
            match field.data_type() {
                ArrowDataType::Int64 => {
                    let mut builder = arrow::array::Int64Builder::with_capacity(vals.len());
                    for v in vals {
                        match v {
                            DataValue::Int64(i) => builder.append_value(*i),
                            _ => builder.append_null(),
                        }
                    }
                    columns.push(Arc::new(builder.finish()));
                }
                ArrowDataType::Float64 => {
                    let mut builder = arrow::array::Float64Builder::with_capacity(vals.len());
                    for v in vals {
                        match v {
                            DataValue::Float64(f) => builder.append_value(*f),
                            _ => builder.append_null(),
                        }
                    }
                    columns.push(Arc::new(builder.finish()));
                }
                _ => {
                    let mut builder = arrow::array::StringBuilder::with_capacity(vals.len(), vals.len() * 16);
                    for v in vals {
                        match v {
                            DataValue::String(s) => builder.append_value(s),
                            DataValue::Null => builder.append_null(),
                            other => builder.append_value(other.to_string()),
                        }
                    }
                    columns.push(Arc::new(builder.finish()));
                }
            }
        }

        let batch = RecordBatch::try_new(arrow_schema, columns)?;
        Ok(Some(batch))
    }
}

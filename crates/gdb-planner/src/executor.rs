use crate::eval::{eval_expr, PathRow};
use crate::plan::PhysicalOperator;
use arrow::array::{ArrayRef, Float64Array, Int64Array, RecordBatch, StringArray, UInt32Array};
use arrow::datatypes::{DataType as ArrowDataType, Field, Schema as ArrowSchema};
use gdb_analytics::AnalyticsEngine;
use gdb_core::schema::GraphSchema;
use gdb_core::{DataValue, EdgeId, EdgeType, GdbError, GdbResult, LabelId, VertexId};
use gdb_parser::ast::{
    BinaryOperator, CypherQuery, Expr, PathPattern, ReturnItem, Statement, UpdateClause, WithClause,
};
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
    pub gpu: Option<Arc<gdb_gpu::GpuDispatcher>>,
    pub stats: Arc<RwLock<Option<gdb_storage::GraphStatistics>>>,
}

impl QueryExecutor {
    pub fn new(schema: Arc<RwLock<GraphSchema>>, storage: Arc<PartitionStorageEngine>) -> Self {
        Self {
            schema,
            storage,
            gpu: None,
            stats: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_gpu(
        schema: Arc<RwLock<GraphSchema>>,
        storage: Arc<PartitionStorageEngine>,
        gpu: Arc<gdb_gpu::GpuDispatcher>,
    ) -> Self {
        Self {
            schema,
            storage,
            gpu: Some(gpu),
            stats: Arc::new(RwLock::new(None)),
        }
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
            Statement::CreateIndex { label, property } => {
                let label_id = {
                    let schema = self.schema.read();
                    schema
                        .get_vertex_schema(&label)
                        .map(|s| s.label_id)
                        .ok_or_else(|| GdbError::Schema(format!("Unknown vertex label: {}", label)))?
                };

                {
                    let mut schema = self.schema.write();
                    schema.register_index(&label, &property)?;
                }

                self.storage.create_vertex_index(label_id, &property)?;
                Ok(QueryResult {
                    message: format!("Created secondary index ON :{}({})", label, property),
                    batch: None,
                    rows_affected: 0,
                })
            }
            Statement::DropIndex { label, property } => {
                let label_id = {
                    let schema = self.schema.read();
                    schema
                        .get_vertex_schema(&label)
                        .map(|s| s.label_id)
                        .ok_or_else(|| GdbError::Schema(format!("Unknown vertex label: {}", label)))?
                };

                {
                    let mut schema = self.schema.write();
                    schema.drop_index(&label, &property)?;
                }

                self.storage.drop_vertex_index(label_id, &property)?;
                Ok(QueryResult {
                    message: format!("Dropped secondary index ON :{}({})", label, property),
                    batch: None,
                    rows_affected: 0,
                })
            }
            Statement::DropVertexLabel { label } => {
                let label_id = {
                    let mut schema = self.schema.write();
                    schema.drop_vertex_label(&label)?
                };
                self.storage.drop_vertex_label(label_id);
                Ok(QueryResult {
                    message: format!("Dropped vertex label '{}'", label),
                    batch: None,
                    rows_affected: 0,
                })
            }
            Statement::DropEdgeType { edge_type } => {
                {
                    let mut schema = self.schema.write();
                    schema.drop_edge_type(&edge_type)?;
                }
                Ok(QueryResult {
                    message: format!("Dropped edge type '{}'", edge_type),
                    batch: None,
                    rows_affected: 0,
                })
            }
            Statement::AlterVertexLabel {
                label,
                add_properties,
                drop_properties,
            } => {
                let label_id = {
                    let schema = self.schema.read();
                    schema
                        .get_vertex_schema(&label)
                        .map(|s| s.label_id)
                        .ok_or_else(|| GdbError::Schema(format!("Unknown vertex label: {}", label)))?
                };
                for dp in &drop_properties {
                    self.storage.drop_vertex_property(label_id, dp);
                }
                {
                    let mut schema = self.schema.write();
                    schema.alter_vertex_label(&label, add_properties, drop_properties)?;
                }
                Ok(QueryResult {
                    message: format!("Altered vertex label '{}'", label),
                    batch: None,
                    rows_affected: 0,
                })
            }
            Statement::AlterEdgeType {
                edge_type,
                add_properties,
                drop_properties,
            } => {
                {
                    let mut schema = self.schema.write();
                    schema.alter_edge_type(&edge_type, add_properties, drop_properties)?;
                }
                Ok(QueryResult {
                    message: format!("Altered edge type '{}'", edge_type),
                    batch: None,
                    rows_affected: 0,
                })
            }
            Statement::ShowSchema => {
                let schema_guard = self.schema.read();
                let mut entity_types: Vec<String> = Vec::new();
                let mut names: Vec<String> = Vec::new();
                let mut ids: Vec<i64> = Vec::new();
                let mut props_desc: Vec<String> = Vec::new();
                let mut indexes_desc: Vec<String> = Vec::new();

                for vs in schema_guard.list_vertex_schemas() {
                    entity_types.push("VERTEX".into());
                    names.push(vs.label.clone());
                    ids.push(vs.label_id.0 as i64);
                    let p_str = vs
                        .properties
                        .iter()
                        .map(|p| format!("{}: {:?}", p.name, p.data_type))
                        .collect::<Vec<_>>()
                        .join(", ");
                    props_desc.push(if p_str.is_empty() { "-".into() } else { p_str });
                    let idx = schema_guard.get_indexes(&vs.label).join(", ");
                    indexes_desc.push(if idx.is_empty() { "-".into() } else { idx });
                }

                for es in schema_guard.list_edge_schemas() {
                    entity_types.push("EDGE".into());
                    names.push(es.edge_type_name.clone());
                    ids.push(es.edge_type.0 as i64);
                    let p_str = es
                        .properties
                        .iter()
                        .map(|p| format!("{}: {:?}", p.name, p.data_type))
                        .collect::<Vec<_>>()
                        .join(", ");
                    props_desc.push(if p_str.is_empty() { "-".into() } else { p_str });
                    indexes_desc.push("-".into());
                }

                let batch_schema = Arc::new(ArrowSchema::new(vec![
                    Field::new("entity_type", ArrowDataType::Utf8, false),
                    Field::new("name", ArrowDataType::Utf8, false),
                    Field::new("id", ArrowDataType::Int64, false),
                    Field::new("properties", ArrowDataType::Utf8, false),
                    Field::new("indexes", ArrowDataType::Utf8, false),
                ]));

                let batch = RecordBatch::try_new(
                    batch_schema,
                    vec![
                        Arc::new(StringArray::from(entity_types)),
                        Arc::new(StringArray::from(names)),
                        Arc::new(Int64Array::from(ids)),
                        Arc::new(StringArray::from(props_desc)),
                        Arc::new(StringArray::from(indexes_desc)),
                    ],
                )?;

                let count = batch.num_rows();
                Ok(QueryResult {
                    message: format!("Catalog schema: {} entities registered", count),
                    batch: Some(batch),
                    rows_affected: count,
                })
            }
            Statement::ShowVertexLabels => {
                let schema_guard = self.schema.read();
                let mut names: Vec<String> = Vec::new();
                let mut ids: Vec<i64> = Vec::new();
                let mut props_desc: Vec<String> = Vec::new();
                let mut indexes_desc: Vec<String> = Vec::new();

                for vs in schema_guard.list_vertex_schemas() {
                    names.push(vs.label.clone());
                    ids.push(vs.label_id.0 as i64);
                    let p_str = vs
                        .properties
                        .iter()
                        .map(|p| format!("{}: {:?}", p.name, p.data_type))
                        .collect::<Vec<_>>()
                        .join(", ");
                    props_desc.push(if p_str.is_empty() { "-".into() } else { p_str });
                    let idx = schema_guard.get_indexes(&vs.label).join(", ");
                    indexes_desc.push(if idx.is_empty() { "-".into() } else { idx });
                }

                let batch_schema = Arc::new(ArrowSchema::new(vec![
                    Field::new("label", ArrowDataType::Utf8, false),
                    Field::new("label_id", ArrowDataType::Int64, false),
                    Field::new("properties", ArrowDataType::Utf8, false),
                    Field::new("indexes", ArrowDataType::Utf8, false),
                ]));

                let batch = RecordBatch::try_new(
                    batch_schema,
                    vec![
                        Arc::new(StringArray::from(names)),
                        Arc::new(Int64Array::from(ids)),
                        Arc::new(StringArray::from(props_desc)),
                        Arc::new(StringArray::from(indexes_desc)),
                    ],
                )?;

                let count = batch.num_rows();
                Ok(QueryResult {
                    message: format!("Registered vertex tags: {}", count),
                    batch: Some(batch),
                    rows_affected: count,
                })
            }
            Statement::ShowEdgeTypes => {
                let schema_guard = self.schema.read();
                let mut names: Vec<String> = Vec::new();
                let mut ids: Vec<i64> = Vec::new();
                let mut props_desc: Vec<String> = Vec::new();

                for es in schema_guard.list_edge_schemas() {
                    names.push(es.edge_type_name.clone());
                    ids.push(es.edge_type.0 as i64);
                    let p_str = es
                        .properties
                        .iter()
                        .map(|p| format!("{}: {:?}", p.name, p.data_type))
                        .collect::<Vec<_>>()
                        .join(", ");
                    props_desc.push(if p_str.is_empty() { "-".into() } else { p_str });
                }

                let batch_schema = Arc::new(ArrowSchema::new(vec![
                    Field::new("edge_type", ArrowDataType::Utf8, false),
                    Field::new("edge_id", ArrowDataType::Int64, false),
                    Field::new("properties", ArrowDataType::Utf8, false),
                ]));

                let batch = RecordBatch::try_new(
                    batch_schema,
                    vec![
                        Arc::new(StringArray::from(names)),
                        Arc::new(Int64Array::from(ids)),
                        Arc::new(StringArray::from(props_desc)),
                    ],
                )?;

                let count = batch.num_rows();
                Ok(QueryResult {
                    message: format!("Registered edge types: {}", count),
                    batch: Some(batch),
                    rows_affected: count,
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
            Statement::InsertVertices { label, vertices } => {
                let label_id = {
                    let schema = self.schema.read();
                    schema
                        .get_vertex_schema(&label)
                        .map(|s| s.label_id)
                        .ok_or_else(|| GdbError::Schema(format!("Unknown vertex label: {}", label)))?
                };

                let count = vertices.len();
                for (id, properties) in vertices {
                    let props_map: HashMap<String, DataValue> = properties.into_iter().collect();
                    self.storage.set_vertex_properties(id, label_id, props_map)?;
                }
                Ok(QueryResult {
                    message: format!("Inserted {} vertices", count),
                    batch: None,
                    rows_affected: count,
                })
            }
            Statement::MergeVertex { label, id, properties } => {
                let label_id = {
                    let schema = self.schema.read();
                    schema
                        .get_vertex_schema(&label)
                        .map(|s| s.label_id)
                        .ok_or_else(|| GdbError::Schema(format!("Unknown vertex label: {}", label)))?
                };

                for (name, val) in properties {
                    self.storage.update_vertex_property(id, label_id, &name, val)?;
                }
                Ok(QueryResult {
                    message: format!("Merged vertex {}", id),
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
            Statement::InsertEdges { edge_type, edges } => {
                let et = {
                    let schema = self.schema.read();
                    schema
                        .get_edge_schema(&edge_type)
                        .map(|s| s.edge_type)
                        .ok_or_else(|| GdbError::Schema(format!("Unknown edge type: {}", edge_type)))?
                };

                let count = edges.len();
                let ver = self.storage.next_commit_version();
                for (src, dst, rank, _props) in edges {
                    let edge = EdgeId::new(src, et, rank, dst);
                    self.storage.insert_edge(edge, ver);
                }
                Ok(QueryResult {
                    message: format!("Inserted {} edges", count),
                    batch: None,
                    rows_affected: count,
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
            Statement::Explain(inner) => self.execute_explain(*inner),
            Statement::AnalyzeGraph => {
                let stats = self.storage.analyze_graph();
                let msg = format!(
                    "Analyzed graph: {} vertices across {} labels, {} edges across {} types (avg degree: {:.2})",
                    stats.total_vertices,
                    stats.vertices_per_label.len(),
                    stats.total_edges,
                    stats.edges_per_type.len(),
                    stats.avg_degree
                );
                *self.stats.write() = Some(stats);
                Ok(QueryResult {
                    message: msg,
                    batch: None,
                    rows_affected: 0,
                })
            }
        }
    }

    fn execute_explain(&self, stmt: Statement) -> GdbResult<QueryResult> {
        match stmt {
            Statement::Query(query) => {
                let plan = self.create_physical_plan(&query)?;
                let ascii_tree = plan.format_ascii_tree(0);

                let mut steps = Vec::new();
                Self::collect_plan_steps(&plan, &mut steps);

                let mut step_col = Vec::new();
                let mut op_col = Vec::new();
                let mut details_col = Vec::new();
                let mut cost_col = Vec::new();

                for (idx, (op, details, cost)) in steps.into_iter().enumerate() {
                    step_col.push((idx + 1) as u32);
                    op_col.push(op);
                    details_col.push(details);
                    cost_col.push(cost);
                }

                let schema = Arc::new(ArrowSchema::new(vec![
                    Field::new("step", ArrowDataType::UInt32, false),
                    Field::new("operator", ArrowDataType::Utf8, false),
                    Field::new("details", ArrowDataType::Utf8, false),
                    Field::new("cost", ArrowDataType::Float64, false),
                ]));

                let batch = RecordBatch::try_new(
                    schema,
                    vec![
                        Arc::new(UInt32Array::from(step_col)),
                        Arc::new(StringArray::from(op_col)),
                        Arc::new(StringArray::from(details_col)),
                        Arc::new(Float64Array::from(cost_col)),
                    ],
                )?;

                Ok(QueryResult {
                    message: ascii_tree,
                    batch: Some(batch),
                    rows_affected: 1,
                })
            }
            Statement::AnalyzeGraph => {
                let stats = self.storage.analyze_graph();
                *self.stats.write() = Some(stats.clone());
                let msg = format!(
                    "Graph analysis completed: {} vertices, {} edges, avg_degree={:.2}, max_degree={}",
                    stats.total_vertices, stats.total_edges, stats.avg_degree, stats.max_degree
                );
                let schema = Arc::new(ArrowSchema::new(vec![
                    Field::new("metric", ArrowDataType::Utf8, false),
                    Field::new("value", ArrowDataType::Utf8, false),
                ]));
                let metrics = vec![
                    "total_vertices".to_string(),
                    "total_edges".to_string(),
                    "avg_degree".to_string(),
                    "max_degree".to_string(),
                ];
                let values = vec![
                    stats.total_vertices.to_string(),
                    stats.total_edges.to_string(),
                    format!("{:.2}", stats.avg_degree),
                    stats.max_degree.to_string(),
                ];
                let batch = RecordBatch::try_new(
                    schema,
                    vec![
                        Arc::new(StringArray::from(metrics)),
                        Arc::new(StringArray::from(values)),
                    ],
                )?;
                Ok(QueryResult {
                    message: msg,
                    batch: Some(batch),
                    rows_affected: 4,
                })
            }
            other => {
                let desc = format!("{:?}", other);
                let schema = Arc::new(ArrowSchema::new(vec![
                    Field::new("operator", ArrowDataType::Utf8, false),
                    Field::new("details", ArrowDataType::Utf8, false),
                ]));
                let batch = RecordBatch::try_new(
                    schema,
                    vec![
                        Arc::new(StringArray::from(vec!["DirectExecution"])),
                        Arc::new(StringArray::from(vec![desc.clone()])),
                    ],
                )?;
                Ok(QueryResult {
                    message: format!("Direct Execution Plan:\n  └─ {}", desc),
                    batch: Some(batch),
                    rows_affected: 1,
                })
            }
        }
    }

    fn collect_plan_steps(plan: &PhysicalOperator, steps: &mut Vec<(String, String, f64)>) {
        match plan {
            PhysicalOperator::ScanVertices { .. } | PhysicalOperator::IndexScan { .. } => {
                steps.push((plan.operator_name().to_string(), plan.operator_details(), 1.0));
            }
            PhysicalOperator::ExpandEdges { input, .. } => {
                Self::collect_plan_steps(input, steps);
                steps.push((plan.operator_name().to_string(), plan.operator_details(), 2.5));
            }
            PhysicalOperator::VarLengthExpand { input, .. } => {
                Self::collect_plan_steps(input, steps);
                steps.push((plan.operator_name().to_string(), plan.operator_details(), 5.0));
            }
            PhysicalOperator::Filter { input, .. } => {
                Self::collect_plan_steps(input, steps);
                steps.push((plan.operator_name().to_string(), plan.operator_details(), 1.2));
            }
            PhysicalOperator::Mutate { input, .. } => {
                Self::collect_plan_steps(input, steps);
                steps.push((plan.operator_name().to_string(), plan.operator_details(), 3.0));
            }
            PhysicalOperator::Distinct { input } => {
                Self::collect_plan_steps(input, steps);
                steps.push((plan.operator_name().to_string(), plan.operator_details(), 1.5));
            }
            PhysicalOperator::Sort { input, .. } => {
                Self::collect_plan_steps(input, steps);
                steps.push((plan.operator_name().to_string(), plan.operator_details(), 2.0));
            }
            PhysicalOperator::Skip { input, .. } => {
                Self::collect_plan_steps(input, steps);
                steps.push((plan.operator_name().to_string(), plan.operator_details(), 1.1));
            }
            PhysicalOperator::Limit { input, .. } => {
                Self::collect_plan_steps(input, steps);
                steps.push((plan.operator_name().to_string(), plan.operator_details(), 1.0));
            }
            PhysicalOperator::Project { input, .. } => {
                Self::collect_plan_steps(input, steps);
                steps.push((plan.operator_name().to_string(), plan.operator_details(), 1.0));
            }
        }
    }

    fn execute_cypher(&self, query: CypherQuery) -> GdbResult<QueryResult> {
        if query.with_clause.is_some() {
            return self.execute_cypher_with(query);
        }

        let plan = self.create_physical_plan(&query)?;
        let snapshot = self.storage.latest_version();
        let rows = self.execute_plan(&plan, snapshot)?;

        if query.return_items.is_empty() {
            let count = rows.len();
            return Ok(QueryResult {
                message: format!("Query completed, {} rows affected", count),
                batch: None,
                rows_affected: count,
            });
        }

        let batch = self.build_record_batch(&query.return_items, &rows)?;
        let count = batch.as_ref().map(|b| b.num_rows()).unwrap_or(0);

        Ok(QueryResult {
            message: format!("Query completed, {} rows returned", count),
            batch,
            rows_affected: count,
        })
    }

    fn execute_cypher_with(&self, query: CypherQuery) -> GdbResult<QueryResult> {
        let snapshot = self.storage.latest_version();
        let with_clause = query.with_clause.clone().unwrap();

        // 1. Plan and execute the initial MATCH pattern and WHERE clause
        let base_query = CypherQuery {
            pattern: query.pattern.clone(),
            where_clause: query.where_clause.clone(),
            with_clause: None,
            next_match: None,
            updates: Vec::new(),
            distinct: false,
            return_items: Vec::new(),
            order_by: Vec::new(),
            skip: None,
            limit: None,
        };
        let base_plan = self.create_physical_plan(&base_query)?;
        let initial_rows = self.execute_plan(&base_plan, snapshot)?;

        // 2. Evaluate WITH clause (projection, grouping, aggregation, filtering)
        let mut with_rows = self.evaluate_with_clause(&with_clause, &initial_rows)?;

        // 3. If next_match is present, expand from bound intermediate vertices
        if let Some((ref next_pat, ref next_where)) = query.next_match {
            with_rows = self.execute_next_match(next_pat, next_where.as_ref(), with_rows, snapshot)?;
        }

        // 4. If updates are present, apply them
        if !query.updates.is_empty() {
            for row in &with_rows {
                for update in &query.updates {
                    match update {
                        UpdateClause::Set { variable, property, expr } => {
                            if let Some(&(vid, label_id)) = row.vertices.get(variable) {
                                let val = eval_expr(expr, row, &self.storage)?;
                                self.storage.update_vertex_property(vid, label_id, property, val)?;
                            }
                        }
                        UpdateClause::Delete { variable, detach } => {
                            if let Some(&(vid, label_id)) = row.vertices.get(variable) {
                                self.storage.delete_vertex(vid, label_id, *detach)?;
                            }
                        }
                        UpdateClause::CreateEdge { src_var, dst_var, edge_type, .. } => {
                            if let (Some(&(src_vid, _)), Some(&(dst_vid, _))) = (row.vertices.get(src_var), row.vertices.get(dst_var)) {
                                let et = {
                                    let schema = self.schema.read();
                                    schema.get_edge_schema(edge_type).map(|s| s.edge_type).unwrap_or(EdgeType(1))
                                };
                                let ver = self.storage.next_commit_version();
                                let edge = EdgeId::new(src_vid, et, 0, dst_vid);
                                self.storage.insert_edge(edge, ver);
                            }
                        }
                        UpdateClause::MergeEdge { src_var, dst_var, edge_type, .. } => {
                            if let (Some(&(src_vid, _)), Some(&(dst_vid, _))) = (row.vertices.get(src_var), row.vertices.get(dst_var)) {
                                let et = {
                                    let schema = self.schema.read();
                                    schema.get_edge_schema(edge_type).map(|s| s.edge_type).unwrap_or(EdgeType(1))
                                };
                                let ver = self.storage.next_commit_version();
                                let edge = EdgeId::new(src_vid, et, 0, dst_vid);
                                self.storage.insert_edge(edge, ver);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // 5. Final distinct
        if query.distinct {
            let mut seen = std::collections::HashSet::new();
            with_rows.retain(|r| {
                let key: Vec<DataValue> = query
                    .return_items
                    .iter()
                    .map(|it| eval_expr(&it.expr, r, &self.storage).unwrap_or(DataValue::Null))
                    .collect();
                seen.insert(format!("{:?}", key))
            });
        }

        // 6. Final order_by
        if !query.order_by.is_empty() {
            with_rows.sort_by(|a, b| {
                for item in &query.order_by {
                    let va = eval_expr(&item.expr, a, &self.storage).unwrap_or(DataValue::Null);
                    let vb = eval_expr(&item.expr, b, &self.storage).unwrap_or(DataValue::Null);
                    let ord = match (va, vb) {
                        (DataValue::Int64(x), DataValue::Int64(y)) => x.cmp(&y),
                        (DataValue::Float64(x), DataValue::Float64(y)) => {
                            x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        (DataValue::String(x), DataValue::String(y)) => x.cmp(&y),
                        (DataValue::Boolean(x), DataValue::Boolean(y)) => x.cmp(&y),
                        _ => std::cmp::Ordering::Equal,
                    };
                    if ord != std::cmp::Ordering::Equal {
                        return if item.ascending { ord } else { ord.reverse() };
                    }
                }
                std::cmp::Ordering::Equal
            });
        }

        // 7. Final skip and limit
        if let Some(skip) = query.skip {
            if skip < with_rows.len() {
                with_rows = with_rows.split_off(skip);
            } else {
                with_rows.clear();
            }
        }
        if let Some(limit) = query.limit {
            with_rows.truncate(limit);
        }

        if query.return_items.is_empty() {
            let count = with_rows.len();
            return Ok(QueryResult {
                message: format!("Query completed, {} rows affected", count),
                batch: None,
                rows_affected: count,
            });
        }

        let batch = self.build_record_batch(&query.return_items, &with_rows)?;
        let count = batch.as_ref().map(|b| b.num_rows()).unwrap_or(0);

        Ok(QueryResult {
            message: format!("Query completed, {} rows returned", count),
            batch,
            rows_affected: count,
        })
    }

    fn evaluate_with_clause(&self, with_clause: &WithClause, rows: &[PathRow]) -> GdbResult<Vec<PathRow>> {
        let has_aggregates = with_clause.items.iter().any(|item| item.expr.is_aggregate());

        let mut output_rows: Vec<PathRow> = if !has_aggregates {
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let mut new_row = PathRow::default();
                for item in &with_clause.items {
                    let val = eval_expr(&item.expr, row, &self.storage)?;
                    let alias = item.alias.clone().unwrap_or_else(|| match &item.expr {
                        Expr::Property { variable, property } => format!("{}.{}", variable, property),
                        Expr::Variable(v) => v.clone(),
                        _ => "val".into(),
                    });
                    new_row.custom_values.insert(alias.clone(), val.clone());

                    // If expression is a variable referencing a node, preserve the node binding
                    if let Expr::Variable(v) = &item.expr {
                        if let Some(&node_binding) = row.vertices.get(v) {
                            let target_var = item.alias.as_deref().unwrap_or(v.as_str());
                            new_row.vertices.insert(target_var.to_string(), node_binding);
                        }
                    }
                }
                out.push(new_row);
            }
            out
        } else {
            let group_key_indices: Vec<usize> = with_clause
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| !item.expr.is_aggregate())
                .map(|(idx, _)| idx)
                .collect();

            let mut groups: HashMap<Vec<DataValue>, Vec<usize>> = HashMap::new();
            for (row_idx, row) in rows.iter().enumerate() {
                let mut key = Vec::with_capacity(group_key_indices.len());
                for &k_idx in &group_key_indices {
                    let val = eval_expr(&with_clause.items[k_idx].expr, row, &self.storage)
                        .unwrap_or(DataValue::Null);
                    key.push(val);
                }
                groups.entry(key).or_default().push(row_idx);
            }

            let mut out = Vec::with_capacity(groups.len());
            for (group_key, row_indices) in &groups {
                let mut new_row = PathRow::default();
                let sample_row = &rows[row_indices[0]];

                for (item_idx, item) in with_clause.items.iter().enumerate() {
                    let alias = item.alias.clone().unwrap_or_else(|| match &item.expr {
                        Expr::Property { variable, property } => format!("{}.{}", variable, property),
                        Expr::Variable(v) => v.clone(),
                        Expr::CountStar => "count(*)".into(),
                        Expr::FunctionCall { name, .. } => name.to_lowercase(),
                        _ => format!("col_{}", item_idx),
                    });

                    if let Some(pos) = group_key_indices.iter().position(|&idx| idx == item_idx) {
                        let val = group_key[pos].clone();
                        new_row.custom_values.insert(alias.clone(), val);
                        if let Expr::Variable(v) = &item.expr {
                            if let Some(&node_binding) = sample_row.vertices.get(v) {
                                let target_var = item.alias.as_deref().unwrap_or(v.as_str());
                                new_row.vertices.insert(target_var.to_string(), node_binding);
                            }
                        }
                    } else {
                        let agg_val = match &item.expr {
                            Expr::CountStar => DataValue::Int64(row_indices.len() as i64),
                            Expr::FunctionCall { name, args } => {
                                let n = name.to_uppercase();
                                match n.as_str() {
                                    "COUNT" => {
                                        if let Some(arg) = args.first() {
                                            let mut count = 0i64;
                                            for &r_idx in row_indices {
                                                if let Ok(v) = eval_expr(arg, &rows[r_idx], &self.storage) {
                                                    if v != DataValue::Null {
                                                        count += 1;
                                                    }
                                                }
                                            }
                                            DataValue::Int64(count)
                                        } else {
                                            DataValue::Int64(row_indices.len() as i64)
                                        }
                                    }
                                    "SUM" => {
                                        if let Some(arg) = args.first() {
                                            let mut total_int = 0i64;
                                            let mut total_float = 0.0f64;
                                            let mut is_float = false;
                                            for &r_idx in row_indices {
                                                if let Ok(v) = eval_expr(arg, &rows[r_idx], &self.storage) {
                                                    match v {
                                                        DataValue::Int64(i) => total_int += i,
                                                        DataValue::Float64(f) => {
                                                            is_float = true;
                                                            total_float += f;
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                            if is_float {
                                                DataValue::Float64(total_float + total_int as f64)
                                            } else {
                                                DataValue::Int64(total_int)
                                            }
                                        } else {
                                            DataValue::Null
                                        }
                                    }
                                    "AVG" => {
                                        if let Some(arg) = args.first() {
                                            let mut sum = 0.0f64;
                                            let mut count = 0usize;
                                            for &r_idx in row_indices {
                                                if let Ok(v) = eval_expr(arg, &rows[r_idx], &self.storage) {
                                                    match v {
                                                        DataValue::Int64(i) => {
                                                            sum += i as f64;
                                                            count += 1;
                                                        }
                                                        DataValue::Float64(f) => {
                                                            sum += f;
                                                            count += 1;
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                            if count > 0 {
                                                DataValue::Float64(sum / count as f64)
                                            } else {
                                                DataValue::Null
                                            }
                                        } else {
                                            DataValue::Null
                                        }
                                    }
                                    "MIN" => {
                                        if let Some(arg) = args.first() {
                                            let mut min_val: Option<DataValue> = None;
                                            for &r_idx in row_indices {
                                                if let Ok(v) = eval_expr(arg, &rows[r_idx], &self.storage) {
                                                    if v != DataValue::Null {
                                                        min_val = Some(match min_val {
                                                            None => v,
                                                            Some(cur) => if v < cur { v } else { cur },
                                                        });
                                                    }
                                                }
                                            }
                                            min_val.unwrap_or(DataValue::Null)
                                        } else {
                                            DataValue::Null
                                        }
                                    }
                                    "MAX" => {
                                        if let Some(arg) = args.first() {
                                            let mut max_val: Option<DataValue> = None;
                                            for &r_idx in row_indices {
                                                if let Ok(v) = eval_expr(arg, &rows[r_idx], &self.storage) {
                                                    if v != DataValue::Null {
                                                        max_val = Some(match max_val {
                                                            None => v,
                                                            Some(cur) => if v > cur { v } else { cur },
                                                        });
                                                    }
                                                }
                                            }
                                            max_val.unwrap_or(DataValue::Null)
                                        } else {
                                            DataValue::Null
                                        }
                                    }
                                    _ => DataValue::Null,
                                }
                            }
                            _ => DataValue::Null,
                        };
                        new_row.custom_values.insert(alias, agg_val);
                    }
                }
                out.push(new_row);
            }
            out
        };

        // Filter by WHERE in WITH
        if let Some(ref where_expr) = with_clause.where_clause {
            output_rows.retain(|r| {
                matches!(eval_expr(where_expr, r, &self.storage), Ok(DataValue::Boolean(true)))
            });
        }

        // Distinct in WITH
        if with_clause.distinct {
            let mut seen = std::collections::HashSet::new();
            output_rows.retain(|r| {
                let key: Vec<(&String, &DataValue)> = r.custom_values.iter().collect();
                seen.insert(format!("{:?}", key))
            });
        }

        // Order by in WITH
        if !with_clause.order_by.is_empty() {
            output_rows.sort_by(|a, b| {
                for item in &with_clause.order_by {
                    let va = eval_expr(&item.expr, a, &self.storage).unwrap_or(DataValue::Null);
                    let vb = eval_expr(&item.expr, b, &self.storage).unwrap_or(DataValue::Null);
                    let ord = match (va, vb) {
                        (DataValue::Int64(x), DataValue::Int64(y)) => x.cmp(&y),
                        (DataValue::Float64(x), DataValue::Float64(y)) => {
                            x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        (DataValue::String(x), DataValue::String(y)) => x.cmp(&y),
                        (DataValue::Boolean(x), DataValue::Boolean(y)) => x.cmp(&y),
                        _ => std::cmp::Ordering::Equal,
                    };
                    if ord != std::cmp::Ordering::Equal {
                        return if item.ascending { ord } else { ord.reverse() };
                    }
                }
                std::cmp::Ordering::Equal
            });
        }

        // Skip & Limit in WITH
        if let Some(skip) = with_clause.skip {
            if skip < output_rows.len() {
                output_rows = output_rows.split_off(skip);
            } else {
                output_rows.clear();
            }
        }
        if let Some(limit) = with_clause.limit {
            output_rows.truncate(limit);
        }

        Ok(output_rows)
    }

    fn execute_next_match(
        &self,
        next_pat: &PathPattern,
        next_where: Option<&Expr>,
        input_rows: Vec<PathRow>,
        snapshot: u64,
    ) -> GdbResult<Vec<PathRow>> {
        let schema = self.schema.read();
        let start_var = next_pat.start_node.variable.clone().unwrap_or_else(|| "_next_start".into());

        let mut output_rows = Vec::new();

        for in_row in input_rows {
            let start_vids: Vec<(VertexId, LabelId)> = if let Some(&binding) = in_row.vertices.get(&start_var) {
                vec![binding]
            } else {
                let target_label_id = if let Some(ref l) = next_pat.start_node.label {
                    schema.get_vertex_schema(l).map(|s| s.label_id)
                } else {
                    None
                };
                let all_vids = self.storage.get_all_vertex_ids(target_label_id);
                all_vids.into_iter().map(|vid| (vid, target_label_id.unwrap_or(LabelId(1)))).collect()
            };

            for (start_vid, label_id) in start_vids {
                let mut current_rows = vec![{
                    let mut r = in_row.clone();
                    r.vertices.insert(start_var.clone(), (start_vid, label_id));
                    r
                }];

                let mut prev_var = start_var.clone();
                for (edge_pat, node_pat) in &next_pat.hops {
                    let dst_var = node_pat.variable.clone().unwrap_or_else(|| "_next_dst".into());
                    let edge_type = if let Some(ref et) = edge_pat.edge_type {
                        schema.get_edge_schema(et).map(|s| s.edge_type)
                    } else {
                        None
                    };

                    let mut next_hop_rows = Vec::new();
                    for r in current_rows {
                        if let Some(&(src_vid, _)) = r.vertices.get(&prev_var) {
                            let out_edges = self.storage.get_out_edges(src_vid, edge_type, snapshot);
                            for e in out_edges {
                                let mut new_r = r.clone();
                                if let Some(ev) = &edge_pat.variable {
                                    new_r.edges.insert(ev.clone(), e);
                                }
                                let target_label_id = if let Some(ref l) = node_pat.label {
                                    schema.get_vertex_schema(l).map(|s| s.label_id).unwrap_or(LabelId(1))
                                } else {
                                    LabelId(1)
                                };
                                new_r.vertices.insert(dst_var.clone(), (e.dst, target_label_id));
                                next_hop_rows.push(new_r);
                            }
                        }
                    }
                    current_rows = next_hop_rows;
                    prev_var = dst_var;
                }

                if let Some(pred) = next_where {
                    current_rows.retain(|r| {
                        matches!(eval_expr(pred, r, &self.storage), Ok(DataValue::Boolean(true)))
                    });
                }

                output_rows.extend(current_rows);
            }
        }

        Ok(output_rows)
    }

    fn create_physical_plan(&self, query: &CypherQuery) -> GdbResult<PhysicalOperator> {
        let schema = self.schema.read();
        let start = &query.pattern.start_node;
        let start_var = start.variable.clone().unwrap_or_else(|| "_start".into());

        let (label_id, label_id_val) = if let Some(ref l) = start.label {
            let lid = schema.get_vertex_schema(l).map(|s| s.label_id)
                .ok_or_else(|| GdbError::Schema(format!("Label not found: {}", l)))?;
            (Some(lid), lid)
        } else {
            (None, LabelId(1))
        };

        // Check for index match in WHERE clause
        let mut index_scan = None;
        let mut filter_needed = true;

        if let Some(ref where_expr) = query.where_clause {
            if let Expr::BinaryOp { left, op: BinaryOperator::Eq, right } = where_expr {
                let matched = match (&**left, &**right) {
                    (Expr::Property { variable, property }, Expr::Literal(lit)) if variable == &start_var => {
                        Some((property.clone(), lit.clone()))
                    }
                    (Expr::Literal(lit), Expr::Property { variable, property }) if variable == &start_var => {
                        Some((property.clone(), lit.clone()))
                    }
                    _ => None,
                };

                if let Some((prop, val)) = matched {
                    if start.label.is_some() && self.storage.has_vertex_index(label_id_val, &prop) {
                        index_scan = Some(PhysicalOperator::IndexScan {
                            var_name: start_var.clone(),
                            label_id: label_id_val,
                            property: prop,
                            value: val,
                        });
                        filter_needed = false;
                    }
                }
            }
        }

        let mut current_plan = if let Some(idx) = index_scan {
            idx
        } else {
            PhysicalOperator::ScanVertices {
                var_name: start_var.clone(),
                label_id,
                id_filter: start.id_filter,
            }
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

            if edge_pat.min_hops == 1 && edge_pat.max_hops == Some(1) {
                current_plan = PhysicalOperator::ExpandEdges {
                    input: Box::new(current_plan),
                    src_var: prev_var,
                    edge_var,
                    dst_var: dst_var.clone(),
                    edge_type,
                };
            } else {
                current_plan = PhysicalOperator::VarLengthExpand {
                    input: Box::new(current_plan),
                    src_var: prev_var,
                    edge_var,
                    dst_var: dst_var.clone(),
                    edge_type,
                    min_hops: edge_pat.min_hops,
                    max_hops: edge_pat.max_hops,
                };
            }

            prev_var = dst_var;
        }

        if let Some(ref where_expr) = query.where_clause {
            if filter_needed {
                current_plan = PhysicalOperator::Filter {
                    input: Box::new(current_plan),
                    predicate: where_expr.clone(),
                };
            }
        }

        if !query.updates.is_empty() {
            current_plan = PhysicalOperator::Mutate {
                input: Box::new(current_plan),
                updates: query.updates.clone(),
            };
        }

        if query.distinct {
            current_plan = PhysicalOperator::Distinct {
                input: Box::new(current_plan),
            };
        }

        if !query.order_by.is_empty() {
            current_plan = PhysicalOperator::Sort {
                input: Box::new(current_plan),
                order_by: query.order_by.clone(),
            };
        }

        if let Some(skip) = query.skip {
            current_plan = PhysicalOperator::Skip {
                input: Box::new(current_plan),
                skip,
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
                let default_lid = label_id.unwrap_or(LabelId(1));
                if let Some(target_id) = id_filter {
                    let mut row = PathRow::default();
                    row.vertices.insert(var_name.clone(), (*target_id, default_lid));
                    rows.push(row);
                } else {
                    let all_vids = self.storage.get_all_vertex_ids(*label_id);
                    for vid in all_vids {
                        let mut row = PathRow::default();
                        row.vertices.insert(var_name.clone(), (vid, default_lid));
                        rows.push(row);
                    }
                }
                Ok(rows)
            }
            PhysicalOperator::IndexScan { var_name, label_id, property, value } => {
                let mut rows = Vec::new();
                if let Some(vids) = self.storage.lookup_vertex_by_index(*label_id, property, value) {
                    for vid in vids {
                        let mut row = PathRow::default();
                        row.vertices.insert(var_name.clone(), (vid, *label_id));
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
                            new_row.vertices.insert(dst_var.clone(), (edge.dst, LabelId(1)));
                            output_rows.push(new_row);
                        }
                    }
                }

                Ok(output_rows)
            }
            PhysicalOperator::VarLengthExpand {
                input,
                src_var,
                edge_var,
                dst_var,
                edge_type,
                min_hops,
                max_hops,
            } => {
                let input_rows = self.execute_plan(input, snapshot)?;
                let mut output_rows = Vec::new();
                let max_depth = max_hops.unwrap_or(15);
                let csr = self.storage.current_csr();
                let use_gpu = if let Some(ref gpu) = self.gpu {
                    gpu.enabled && csr.edge_count() >= gpu.threshold_edges
                } else {
                    false
                };

                for row in input_rows {
                    if let Some(&(start_vid, _)) = row.vertices.get(src_var) {
                        if use_gpu && max_depth > 1 {
                            if let Some(ref gpu) = self.gpu {
                                if let Ok(bfs_results) = gpu.bfs(&csr, start_vid, max_depth as u32) {
                                    for (reached_vid, depth) in bfs_results {
                                        let d = depth as usize;
                                        if d >= *min_hops && d <= max_depth && reached_vid != start_vid {
                                            let mut new_row = row.clone();
                                            new_row.vertices.insert(dst_var.clone(), (reached_vid, LabelId(1)));
                                            output_rows.push(new_row);
                                        }
                                    }
                                    continue;
                                }
                            }
                        }

                        let mut queue = std::collections::VecDeque::new();
                        queue.push_back((start_vid, 0usize, std::collections::HashSet::new(), None));

                        while let Some((curr_vid, depth, visited_edges, last_edge)) = queue.pop_front() {
                            if depth >= *min_hops {
                                let mut new_row = row.clone();
                                if let (Some(ev), Some(le)) = (edge_var, last_edge) {
                                    new_row.edges.insert(ev.clone(), le);
                                }
                                new_row.vertices.insert(dst_var.clone(), (curr_vid, LabelId(1)));
                                output_rows.push(new_row);
                            }

                            if depth < max_depth {
                                let edges = self.storage.get_out_edges(curr_vid, *edge_type, snapshot);
                                for edge in edges {
                                    if !visited_edges.contains(&edge) {
                                        let mut next_visited = visited_edges.clone();
                                        next_visited.insert(edge);
                                        queue.push_back((edge.dst, depth + 1, next_visited, Some(edge)));
                                    }
                                }
                            }
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
            PhysicalOperator::Mutate { input, updates } => {
                let input_rows = self.execute_plan(input, snapshot)?;
                for row in &input_rows {
                    for update in updates {
                        match update {
                            UpdateClause::Set { variable, property, expr } => {
                                if let Some(&(vid, label_id)) = row.vertices.get(variable) {
                                    let val = eval_expr(expr, row, &self.storage)?;
                                    self.storage.update_vertex_property(vid, label_id, property, val)?;
                                }
                            }
                            UpdateClause::Delete { variable, detach } => {
                                if let Some(&(vid, label_id)) = row.vertices.get(variable) {
                                    self.storage.delete_vertex(vid, label_id, *detach)?;
                                }
                            }
                            UpdateClause::MergeVertex { variable, label, id, properties } => {
                                let label_id = {
                                    let schema = self.schema.read();
                                    schema.get_vertex_schema(label).map(|s| s.label_id).unwrap_or(LabelId(1))
                                };
                                let target_vid = if let Some(vid) = id {
                                    *vid
                                } else if let Some(var) = variable {
                                    row.vertices.get(var).map(|(v, _)| *v).unwrap_or(VertexId(0))
                                } else {
                                    VertexId(0)
                                };
                                for (prop_name, expr) in properties {
                                    let val = eval_expr(expr, row, &self.storage)?;
                                    self.storage.update_vertex_property(target_vid, label_id, prop_name, val)?;
                                }
                            }
                            UpdateClause::CreateEdge { src_var, dst_var, edge_type, .. } => {
                                if let (Some(&(src_vid, _)), Some(&(dst_vid, _))) = (row.vertices.get(src_var), row.vertices.get(dst_var)) {
                                    let et = {
                                        let schema = self.schema.read();
                                        schema.get_edge_schema(edge_type).map(|s| s.edge_type).unwrap_or(EdgeType(1))
                                    };
                                    let ver = self.storage.next_commit_version();
                                    let edge = EdgeId::new(src_vid, et, 0, dst_vid);
                                    self.storage.insert_edge(edge, ver);
                                }
                            }
                            UpdateClause::MergeEdge { src_var, dst_var, edge_type, .. } => {
                                if let (Some(&(src_vid, _)), Some(&(dst_vid, _))) = (row.vertices.get(src_var), row.vertices.get(dst_var)) {
                                    let et = {
                                        let schema = self.schema.read();
                                        schema.get_edge_schema(edge_type).map(|s| s.edge_type).unwrap_or(EdgeType(1))
                                    };
                                    let ver = self.storage.next_commit_version();
                                    let edge = EdgeId::new(src_vid, et, 0, dst_vid);
                                    self.storage.insert_edge(edge, ver);
                                }
                            }
                        }
                    }
                }
                Ok(input_rows)
            }
            PhysicalOperator::Distinct { input } => {
                let input_rows = self.execute_plan(input, snapshot)?;
                let mut seen = std::collections::HashSet::new();
                let mut output = Vec::new();

                for row in input_rows {
                    let mut key: Vec<(&String, u64)> = row.vertices.iter().map(|(k, (v, _))| (k, v.as_u64())).collect();
                    key.sort_by(|a, b| a.0.cmp(b.0));
                    if seen.insert(format!("{:?}", key)) {
                        output.push(row);
                    }
                }
                Ok(output)
            }
            PhysicalOperator::Sort { input, order_by } => {
                let mut input_rows = self.execute_plan(input, snapshot)?;
                input_rows.sort_by(|a, b| {
                    for item in order_by {
                        let va = eval_expr(&item.expr, a, &self.storage).unwrap_or(DataValue::Null);
                        let vb = eval_expr(&item.expr, b, &self.storage).unwrap_or(DataValue::Null);
                        let cmp = if item.ascending {
                            va.partial_cmp(&vb)
                        } else {
                            vb.partial_cmp(&va)
                        };
                        if let Some(c) = cmp {
                            if c != std::cmp::Ordering::Equal {
                                return c;
                            }
                        }
                    }
                    std::cmp::Ordering::Equal
                });
                Ok(input_rows)
            }
            PhysicalOperator::Skip { input, skip } => {
                let input_rows = self.execute_plan(input, snapshot)?;
                Ok(input_rows.into_iter().skip(*skip).collect())
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
            "algo.node2vec" => {
                let dim = match args.get("dimensions").or_else(|| args.get("dim")) {
                    Some(DataValue::Int64(i)) => *i as usize,
                    _ => 64,
                };
                let walk_len = match args.get("walk_length") {
                    Some(DataValue::Int64(i)) => *i as usize,
                    _ => 10,
                };
                let num_walks = match args.get("num_walks") {
                    Some(DataValue::Int64(i)) => *i as usize,
                    _ => 10,
                };
                let p = match args.get("p") {
                    Some(DataValue::Float64(f)) => *f,
                    Some(DataValue::Int64(i)) => *i as f64,
                    _ => 1.0,
                };
                let q = match args.get("q") {
                    Some(DataValue::Float64(f)) => *f,
                    Some(DataValue::Int64(i)) => *i as f64,
                    _ => 1.0,
                };
                AnalyticsEngine::run_node2vec(&csr, dim, walk_len, num_walks, p, q)?
            }
            "vector.similaritysearch" | "vector.search" => {
                let label = match args.get("label") {
                    Some(DataValue::String(s)) => s.clone(),
                    _ => "Document".to_string(),
                };
                let property = match args.get("property") {
                    Some(DataValue::String(s)) => s.clone(),
                    _ => "embedding".to_string(),
                };
                let k = match args.get("k").or_else(|| args.get("top_k")) {
                    Some(DataValue::Int64(i)) => *i as usize,
                    _ => 10,
                };
                let metric = match args.get("metric") {
                    Some(DataValue::String(s)) => s.clone(),
                    _ => "cosine".to_string(),
                };
                let query_vec: Vec<f32> = match args.get("query").or_else(|| args.get("embedding")) {
                    Some(DataValue::Vector(v)) => v.clone(),
                    Some(DataValue::List(l)) => l.iter().filter_map(|x| match x {
                        DataValue::Float64(f) => Some(*f as f32),
                        DataValue::Int64(i) => Some(*i as f32),
                        _ => None,
                    }).collect(),
                    _ => Vec::new(),
                };

                let label_id = {
                    let schema = self.schema.read();
                    schema.get_vertex_schema(&label).map(|s| s.label_id).unwrap_or(LabelId(1))
                };

                let results = self.storage.vector_similarity_search(label_id, &property, &query_vec, k, &metric);
                let mut vids = Vec::with_capacity(results.len());
                let mut sims = Vec::with_capacity(results.len());
                for (vid, sim) in results {
                    vids.push(vid.as_u64());
                    sims.push(sim as f64);
                }

                let arrow_schema = Arc::new(ArrowSchema::new(vec![
                    Field::new("node", ArrowDataType::UInt64, false),
                    Field::new("similarity", ArrowDataType::Float64, false),
                ]));
                let cols: Vec<ArrayRef> = vec![
                    Arc::new(arrow::array::UInt64Array::from(vids)),
                    Arc::new(arrow::array::Float64Array::from(sims)),
                ];
                RecordBatch::try_new(arrow_schema, cols)?
            }
            other => return Err(GdbError::Execution(format!("Unknown graph algorithm: {}", other))),
        };

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

        let has_aggregates = return_items.iter().any(|item| item.expr.is_aggregate());

        if !has_aggregates {
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

                let arrow_type = column_values[col_idx]
                    .iter()
                    .find(|v| **v != DataValue::Null)
                    .map(|v| v.data_type())
                    .unwrap_or(ArrowDataType::Utf8);

                fields.push(Field::new(col_name, arrow_type, true));
            }

            let arrow_schema = Arc::new(ArrowSchema::new(fields));
            let columns = Self::build_arrow_columns(&arrow_schema, &column_values)?;
            let batch = RecordBatch::try_new(arrow_schema, columns)?;
            return Ok(Some(batch));
        }

        // Grouping & Aggregations
        let group_key_indices: Vec<usize> = return_items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.expr.is_aggregate())
            .map(|(idx, _)| idx)
            .collect();

        let mut groups: HashMap<Vec<DataValue>, Vec<usize>> = HashMap::new();

        for (row_idx, row) in rows.iter().enumerate() {
            let mut key = Vec::with_capacity(group_key_indices.len());
            for &k_idx in &group_key_indices {
                let val = eval_expr(&return_items[k_idx].expr, row, &self.storage).unwrap_or(DataValue::Null);
                key.push(val);
            }
            groups.entry(key).or_default().push(row_idx);
        }

        let num_output_rows = groups.len();
        let mut column_values: Vec<Vec<DataValue>> = vec![Vec::with_capacity(num_output_rows); return_items.len()];
        let mut fields = Vec::with_capacity(return_items.len());

        for (col_idx, item) in return_items.iter().enumerate() {
            let col_name = item.alias.clone().unwrap_or_else(|| match &item.expr {
                Expr::Property { variable, property } => format!("{}.{}", variable, property),
                Expr::Variable(v) => v.clone(),
                Expr::CountStar => "count(*)".into(),
                Expr::FunctionCall { name, .. } => format!("{}(...)", name.to_lowercase()),
                _ => format!("col_{}", col_idx),
            });

            for (group_key, row_indices) in &groups {
                if let Some(pos) = group_key_indices.iter().position(|&idx| idx == col_idx) {
                    column_values[col_idx].push(group_key[pos].clone());
                } else {
                    let agg_val = match &item.expr {
                        Expr::CountStar => DataValue::Int64(row_indices.len() as i64),
                        Expr::FunctionCall { name, args } => {
                            let n = name.to_uppercase();
                            match n.as_str() {
                                "COUNT" => {
                                    if let Some(arg) = args.first() {
                                        let mut count = 0i64;
                                        for &r_idx in row_indices {
                                            if let Ok(v) = eval_expr(arg, &rows[r_idx], &self.storage) {
                                                if v != DataValue::Null {
                                                    count += 1;
                                                }
                                            }
                                        }
                                        DataValue::Int64(count)
                                    } else {
                                        DataValue::Int64(row_indices.len() as i64)
                                    }
                                }
                                "SUM" => {
                                    let mut sum_f = 0.0f64;
                                    let mut is_float = false;
                                    let mut sum_i = 0i64;
                                    if let Some(arg) = args.first() {
                                        for &r_idx in row_indices {
                                            if let Ok(v) = eval_expr(arg, &rows[r_idx], &self.storage) {
                                                match v {
                                                    DataValue::Int64(i) => sum_i += i,
                                                    DataValue::Float64(f) => {
                                                        is_float = true;
                                                        sum_f += f;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                    if is_float {
                                        DataValue::Float64(sum_f + sum_i as f64)
                                    } else {
                                        DataValue::Int64(sum_i)
                                    }
                                }
                                "AVG" => {
                                    let mut sum = 0.0f64;
                                    let mut count = 0usize;
                                    if let Some(arg) = args.first() {
                                        for &r_idx in row_indices {
                                            if let Ok(v) = eval_expr(arg, &rows[r_idx], &self.storage) {
                                                match v {
                                                    DataValue::Int64(i) => {
                                                        sum += i as f64;
                                                        count += 1;
                                                    }
                                                    DataValue::Float64(f) => {
                                                        sum += f;
                                                        count += 1;
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                    if count > 0 {
                                        DataValue::Float64(sum / count as f64)
                                    } else {
                                        DataValue::Null
                                    }
                                }
                                "MIN" => {
                                    let mut min_val: Option<DataValue> = None;
                                    if let Some(arg) = args.first() {
                                        for &r_idx in row_indices {
                                            if let Ok(v) = eval_expr(arg, &rows[r_idx], &self.storage) {
                                                if v != DataValue::Null {
                                                    min_val = match min_val {
                                                        None => Some(v),
                                                        Some(curr) => if v < curr { Some(v) } else { Some(curr) },
                                                    };
                                                }
                                            }
                                        }
                                    }
                                    min_val.unwrap_or(DataValue::Null)
                                }
                                "MAX" => {
                                    let mut max_val: Option<DataValue> = None;
                                    if let Some(arg) = args.first() {
                                        for &r_idx in row_indices {
                                            if let Ok(v) = eval_expr(arg, &rows[r_idx], &self.storage) {
                                                if v != DataValue::Null {
                                                    max_val = match max_val {
                                                        None => Some(v),
                                                        Some(curr) => if v > curr { Some(v) } else { Some(curr) },
                                                    };
                                                }
                                            }
                                        }
                                    }
                                    max_val.unwrap_or(DataValue::Null)
                                }
                                _ => DataValue::Null,
                            }
                        }
                        _ => DataValue::Null,
                    };
                    column_values[col_idx].push(agg_val);
                }
            }

            let arrow_type = column_values[col_idx]
                .iter()
                .find(|v| **v != DataValue::Null)
                .map(|v| v.data_type())
                .unwrap_or(ArrowDataType::Utf8);

            fields.push(Field::new(col_name, arrow_type, true));
        }

        let arrow_schema = Arc::new(ArrowSchema::new(fields));
        let columns = Self::build_arrow_columns(&arrow_schema, &column_values)?;
        let batch = RecordBatch::try_new(arrow_schema, columns)?;
        Ok(Some(batch))
    }

    fn build_arrow_columns(
        schema: &Arc<ArrowSchema>,
        column_values: &[Vec<DataValue>],
    ) -> GdbResult<Vec<ArrayRef>> {
        let mut columns: Vec<ArrayRef> = Vec::with_capacity(schema.fields().len());

        for (col_idx, field) in schema.fields().iter().enumerate() {
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
                            DataValue::Int64(i) => builder.append_value(*i as f64),
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
        Ok(columns)
    }
}

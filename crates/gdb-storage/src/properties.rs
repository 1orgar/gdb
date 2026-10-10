use arrow::array::{ArrayRef, RecordBatch};
use arrow::datatypes::SchemaRef;
use dashmap::DashMap;
use gdb_core::schema::VertexSchema;
use gdb_core::{DataValue, GdbError, GdbResult, LabelId, VertexId};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Columnar Property Storage for vertices belonging to a specific Label.
pub struct VertexPropertyTable {
    pub label_id: LabelId,
    pub schema: SchemaRef,
    /// Fast in-memory map of vertex properties for instant OLTP point queries:
    /// VertexId -> HashMap<PropertyName, DataValue>
    row_cache: DashMap<u64, HashMap<String, DataValue>, ahash::RandomState>,
    /// Secondary property indexes: PropertyName -> (DataValue -> Vec<VertexId>)
    indexes: DashMap<String, DashMap<DataValue, Vec<VertexId>, ahash::RandomState>, ahash::RandomState>,
    /// Columnar Arrow RecordBatches for vectorized analytical scans (OLAP)
    batches: RwLock<Vec<RecordBatch>>,
}

impl VertexPropertyTable {
    pub fn new(schema_def: &VertexSchema) -> Self {
        let arrow_schema = schema_def.to_arrow_schema();
        Self {
            label_id: schema_def.label_id,
            schema: arrow_schema,
            row_cache: DashMap::with_hasher(ahash::RandomState::new()),
            indexes: DashMap::with_hasher(ahash::RandomState::new()),
            batches: RwLock::new(Vec::new()),
        }
    }

    /// Creates a secondary index on a vertex property.
    pub fn create_index(&self, property_name: &str) {
        let index_map = DashMap::with_hasher(ahash::RandomState::new());
        for entry in self.row_cache.iter() {
            let vid = VertexId(*entry.key());
            if let Some(val) = entry.value().get(property_name) {
                index_map.entry(val.clone()).or_insert_with(Vec::new).push(vid);
            }
        }
        self.indexes.insert(property_name.to_string(), index_map);
    }

    /// Drops a secondary index on a vertex property.
    pub fn drop_index(&self, property_name: &str) -> bool {
        self.indexes.remove(property_name).is_some()
    }

    /// Checks if a secondary index exists on a vertex property.
    pub fn has_index(&self, property_name: &str) -> bool {
        self.indexes.contains_key(property_name)
    }

    /// Looks up vertices matching a property value via secondary index.
    pub fn lookup_by_index(&self, property_name: &str, value: &DataValue) -> Option<Vec<VertexId>> {
        self.indexes.get(property_name).and_then(|idx| {
            idx.get(value).and_then(|v| {
                if v.is_empty() {
                    None
                } else {
                    Some(v.clone())
                }
            })
        })
    }

    /// Fast OLTP insertion of vertex properties, updating indexes if present.
    pub fn set_properties(&self, vid: VertexId, props: HashMap<String, DataValue>) {
        if let Some(old) = self.row_cache.get(&vid.as_u64()) {
            for idx_entry in self.indexes.iter() {
                let prop_name = idx_entry.key();
                let idx_map = idx_entry.value();
                if let Some(old_val) = old.get(prop_name) {
                    if let Some(mut vids) = idx_map.get_mut(old_val) {
                        vids.retain(|v| *v != vid);
                    }
                }
            }
        }

        for idx_entry in self.indexes.iter() {
            let prop_name = idx_entry.key();
            let idx_map = idx_entry.value();
            if let Some(new_val) = props.get(prop_name) {
                idx_map.entry(new_val.clone()).or_insert_with(Vec::new).push(vid);
            }
        }

        self.row_cache.insert(vid.as_u64(), props);
    }

    /// Updates a single property on a vertex.
    pub fn update_property(&self, vid: VertexId, prop_name: &str, new_val: DataValue) {
        if let Some(mut map) = self.row_cache.get_mut(&vid.as_u64()) {
            let old_val = map.insert(prop_name.to_string(), new_val.clone());
            if let Some(idx_map) = self.indexes.get(prop_name) {
                if let Some(ref old) = old_val {
                    if let Some(mut vids) = idx_map.get_mut(old) {
                        vids.retain(|v| *v != vid);
                    }
                }
                idx_map.entry(new_val).or_insert_with(Vec::new).push(vid);
            }
        }
    }

    /// Deletes a vertex from property storage and indexes.
    pub fn delete_vertex(&self, vid: VertexId) -> bool {
        if let Some((_, old_props)) = self.row_cache.remove(&vid.as_u64()) {
            for idx_entry in self.indexes.iter() {
                let prop_name = idx_entry.key();
                let idx_map = idx_entry.value();
                if let Some(old_val) = old_props.get(prop_name) {
                    if let Some(mut vids) = idx_map.get_mut(old_val) {
                        vids.retain(|v| *v != vid);
                    }
                }
            }
            true
        } else {
            false
        }
    }

    /// Drops a property column from all stored vertices and removes its index.
    pub fn drop_property(&self, prop_name: &str) {
        self.indexes.remove(prop_name);
        for mut entry in self.row_cache.iter_mut() {
            entry.value_mut().remove(prop_name);
        }
    }

    /// Fast OLTP lookup of a property by name.
    pub fn get_property(&self, vid: VertexId, prop_name: &str) -> Option<DataValue> {
        self.row_cache
            .get(&vid.as_u64())
            .and_then(|map| map.get(prop_name).cloned())
    }

    /// Returns all vertex IDs currently stored in row cache.
    pub fn vertex_ids(&self) -> Vec<VertexId> {
        self.row_cache.iter().map(|entry| VertexId(*entry.key())).collect()
    }

    /// Number of vertices stored in the property table.
    pub fn len(&self) -> usize {
        self.row_cache.len()
    }

    /// Whether the property table is empty.
    pub fn is_empty(&self) -> bool {
        self.row_cache.is_empty()
    }

    /// Re-builds Arrow RecordBatches from in-memory row cache for vectorized OLAP query execution.
    pub fn rebuild_arrow_batches(&self) -> GdbResult<()> {
        let count = self.row_cache.len();
        if count == 0 {
            *self.batches.write() = Vec::new();
            return Ok(());
        }

        let mut vid_builder = arrow::array::UInt64Builder::with_capacity(count);
        let fields = self.schema.fields();

        // Prepare columns builders
        let mut column_builders: Vec<Box<dyn ArrayBuilderHelper>> = Vec::new();
        for field in fields.iter().skip(1) {
            column_builders.push(create_builder_helper(field.data_type(), count)?);
        }

        // Fill data from row cache
        for entry in self.row_cache.iter() {
            let vid = *entry.key();
            vid_builder.append_value(vid);

            let props = entry.value();
            for (i, field) in fields.iter().skip(1).enumerate() {
                let val = props.get(field.name()).unwrap_or(&DataValue::Null);
                column_builders[i].append_value(val)?;
            }
        }

        let mut columns: Vec<ArrayRef> = Vec::with_capacity(fields.len());
        columns.push(Arc::new(vid_builder.finish()));

        for mut builder in column_builders {
            columns.push(builder.finish_array());
        }

        let batch = RecordBatch::try_new(self.schema.clone(), columns)?;
        *self.batches.write() = vec![batch];
        Ok(())
    }

    /// Returns current Arrow RecordBatches for vectorized scanning.
    pub fn get_batches(&self) -> Vec<RecordBatch> {
        self.batches.read().clone()
    }

    /// Performs vector similarity search (cosine, dot, or euclidean L2) against stored Vector properties.
    pub fn vector_similarity_search(
        &self,
        property_name: &str,
        query: &[f32],
        k: usize,
        metric: &str,
    ) -> Vec<(VertexId, f32)> {
        let mut candidates = Vec::new();
        let query_norm = if metric.eq_ignore_ascii_case("cosine") {
            let sum_sq: f32 = query.iter().map(|x| x * x).sum();
            sum_sq.sqrt()
        } else {
            1.0
        };

        for entry in self.row_cache.iter() {
            let vid = VertexId(*entry.key());
            if let Some(DataValue::Vector(v)) = entry.value().get(property_name) {
                if v.len() == query.len() {
                    let score = match metric.to_lowercase().as_str() {
                        "cosine" => {
                            let dot: f32 = v.iter().zip(query.iter()).map(|(a, b)| a * b).sum();
                            let v_norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                            if v_norm == 0.0 || query_norm == 0.0 {
                                0.0
                            } else {
                                dot / (v_norm * query_norm)
                            }
                        }
                        "dot" | "dot_product" => {
                            v.iter().zip(query.iter()).map(|(a, b)| a * b).sum()
                        }
                        "l2" | "euclidean" => {
                            let dist_sq: f32 = v.iter().zip(query.iter()).map(|(a, b)| (a - b) * (a - b)).sum();
                            1.0 / (1.0 + dist_sq.sqrt())
                        }
                        _ => {
                            let dot: f32 = v.iter().zip(query.iter()).map(|(a, b)| a * b).sum();
                            let v_norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
                            if v_norm == 0.0 || query_norm == 0.0 {
                                0.0
                            } else {
                                dot / (v_norm * query_norm)
                            }
                        }
                    };
                    candidates.push((vid, score));
                }
            }
        }

        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(k);
        candidates
    }
}

/// Helper trait to append DataValues to Arrow array builders dynamically.
trait ArrayBuilderHelper: Send + Sync {
    fn append_value(&mut self, val: &DataValue) -> GdbResult<()>;
    fn finish_array(&mut self) -> ArrayRef;
}

struct Int64BuilderHelper(arrow::array::Int64Builder);
impl ArrayBuilderHelper for Int64BuilderHelper {
    fn append_value(&mut self, val: &DataValue) -> GdbResult<()> {
        match val {
            DataValue::Int64(v) => self.0.append_value(*v),
            DataValue::Null => self.0.append_null(),
            _ => self.0.append_null(),
        }
        Ok(())
    }
    fn finish_array(&mut self) -> ArrayRef {
        Arc::new(self.0.finish())
    }
}

struct Float64BuilderHelper(arrow::array::Float64Builder);
impl ArrayBuilderHelper for Float64BuilderHelper {
    fn append_value(&mut self, val: &DataValue) -> GdbResult<()> {
        match val {
            DataValue::Float64(v) => self.0.append_value(*v),
            DataValue::Null => self.0.append_null(),
            _ => self.0.append_null(),
        }
        Ok(())
    }
    fn finish_array(&mut self) -> ArrayRef {
        Arc::new(self.0.finish())
    }
}

struct StringBuilderHelper(arrow::array::StringBuilder);
impl ArrayBuilderHelper for StringBuilderHelper {
    fn append_value(&mut self, val: &DataValue) -> GdbResult<()> {
        match val {
            DataValue::String(s) => self.0.append_value(s),
            DataValue::Null => self.0.append_null(),
            _ => self.0.append_null(),
        }
        Ok(())
    }
    fn finish_array(&mut self) -> ArrayRef {
        Arc::new(self.0.finish())
    }
}

struct BooleanBuilderHelper(arrow::array::BooleanBuilder);
impl ArrayBuilderHelper for BooleanBuilderHelper {
    fn append_value(&mut self, val: &DataValue) -> GdbResult<()> {
        match val {
            DataValue::Boolean(b) => self.0.append_value(*b),
            DataValue::Null => self.0.append_null(),
            _ => self.0.append_null(),
        }
        Ok(())
    }
    fn finish_array(&mut self) -> ArrayRef {
        Arc::new(self.0.finish())
    }
}

fn create_builder_helper(dt: &arrow::datatypes::DataType, cap: usize) -> GdbResult<Box<dyn ArrayBuilderHelper>> {
    match dt {
        arrow::datatypes::DataType::Int64 => {
            Ok(Box::new(Int64BuilderHelper(arrow::array::Int64Builder::with_capacity(cap))))
        }
        arrow::datatypes::DataType::Float64 => {
            Ok(Box::new(Float64BuilderHelper(arrow::array::Float64Builder::with_capacity(cap))))
        }
        arrow::datatypes::DataType::Utf8 => {
            Ok(Box::new(StringBuilderHelper(arrow::array::StringBuilder::with_capacity(cap, cap * 16))))
        }
        arrow::datatypes::DataType::Boolean => {
            Ok(Box::new(BooleanBuilderHelper(arrow::array::BooleanBuilder::with_capacity(cap))))
        }
        other => Err(GdbError::Storage(format!("Unsupported Arrow builder type: {:?}", other))),
    }
}

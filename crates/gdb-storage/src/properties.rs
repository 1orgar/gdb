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
            batches: RwLock::new(Vec::new()),
        }
    }

    /// Fast OLTP insertion of vertex properties.
    pub fn set_properties(&self, vid: VertexId, props: HashMap<String, DataValue>) {
        self.row_cache.insert(vid.as_u64(), props);
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

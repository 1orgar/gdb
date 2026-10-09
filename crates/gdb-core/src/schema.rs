use crate::error::{GdbError, GdbResult};
use crate::id::{EdgeType, LabelId};
use arrow_schema::{DataType as ArrowDataType, Field, Schema as ArrowSchema};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    Boolean,
    Int64,
    Float64,
    String,
    Date,
    Timestamp,
}

impl DataType {
    pub fn to_arrow(&self) -> ArrowDataType {
        match self {
            DataType::Boolean => ArrowDataType::Boolean,
            DataType::Int64 => ArrowDataType::Int64,
            DataType::Float64 => ArrowDataType::Float64,
            DataType::String => ArrowDataType::Utf8,
            DataType::Date => ArrowDataType::Date32,
            DataType::Timestamp => ArrowDataType::Timestamp(arrow_schema::TimeUnit::Microsecond, None),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertySpec {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
}

impl PropertySpec {
    pub fn new(name: impl Into<String>, data_type: DataType, nullable: bool) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable,
        }
    }

    pub fn to_arrow_field(&self) -> Field {
        Field::new(&self.name, self.data_type.to_arrow(), self.nullable)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VertexSchema {
    pub label: String,
    pub label_id: LabelId,
    pub properties: Vec<PropertySpec>,
}

impl VertexSchema {
    pub fn new(label: impl Into<String>, label_id: LabelId, properties: Vec<PropertySpec>) -> Self {
        Self {
            label: label.into(),
            label_id,
            properties,
        }
    }

    pub fn to_arrow_schema(&self) -> Arc<ArrowSchema> {
        let mut fields = Vec::with_capacity(self.properties.len() + 1);
        // First column is always the 64-bit vertex ID
        fields.push(Field::new("_vertex_id", ArrowDataType::UInt64, false));
        for prop in &self.properties {
            fields.push(prop.to_arrow_field());
        }
        Arc::new(ArrowSchema::new(fields))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeSchema {
    pub edge_type_name: String,
    pub edge_type: EdgeType,
    pub properties: Vec<PropertySpec>,
}

impl EdgeSchema {
    pub fn new(edge_type_name: impl Into<String>, edge_type: EdgeType, properties: Vec<PropertySpec>) -> Self {
        Self {
            edge_type_name: edge_type_name.into(),
            edge_type,
            properties,
        }
    }

    pub fn to_arrow_schema(&self) -> Arc<ArrowSchema> {
        let mut fields = Vec::with_capacity(self.properties.len() + 4);
        fields.push(Field::new("_src_id", ArrowDataType::UInt64, false));
        fields.push(Field::new("_edge_type", ArrowDataType::UInt32, false));
        fields.push(Field::new("_rank", ArrowDataType::Int64, false));
        fields.push(Field::new("_dst_id", ArrowDataType::UInt64, false));
        for prop in &self.properties {
            fields.push(prop.to_arrow_field());
        }
        Arc::new(ArrowSchema::new(fields))
    }
}

/// Catalog managing all registered vertex labels and edge types.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphSchema {
    pub name: String,
    labels: HashMap<String, VertexSchema>,
    label_ids: HashMap<LabelId, String>,
    edges: HashMap<String, EdgeSchema>,
    edge_types: HashMap<EdgeType, String>,
    #[serde(default)]
    indexes: HashMap<String, std::collections::HashSet<String>>,
    next_label_id: u32,
    next_edge_type: u32,
}

impl GraphSchema {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            labels: HashMap::new(),
            label_ids: HashMap::new(),
            edges: HashMap::new(),
            edge_types: HashMap::new(),
            indexes: HashMap::new(),
            next_label_id: 1,
            next_edge_type: 1,
        }
    }

    pub fn register_vertex_label(&mut self, label: &str, properties: Vec<PropertySpec>) -> GdbResult<LabelId> {
        if self.labels.contains_key(label) {
            return Err(GdbError::Schema(format!("Vertex label '{}' already exists", label)));
        }
        let id = LabelId(self.next_label_id);
        self.next_label_id += 1;
        let schema = VertexSchema::new(label, id, properties);
        self.labels.insert(label.to_string(), schema);
        self.label_ids.insert(id, label.to_string());
        Ok(id)
    }

    pub fn register_edge_type(&mut self, edge_type_name: &str, properties: Vec<PropertySpec>) -> GdbResult<EdgeType> {
        if self.edges.contains_key(edge_type_name) {
            return Err(GdbError::Schema(format!("Edge type '{}' already exists", edge_type_name)));
        }
        let id = EdgeType(self.next_edge_type);
        self.next_edge_type += 1;
        let schema = EdgeSchema::new(edge_type_name, id, properties);
        self.edges.insert(edge_type_name.to_string(), schema);
        self.edge_types.insert(id, edge_type_name.to_string());
        Ok(id)
    }

    pub fn register_index(&mut self, label: &str, property: &str) -> GdbResult<()> {
        if !self.labels.contains_key(label) {
            return Err(GdbError::Schema(format!("Vertex label '{}' does not exist", label)));
        }
        self.indexes
            .entry(label.to_string())
            .or_default()
            .insert(property.to_string());
        Ok(())
    }

    pub fn drop_index(&mut self, label: &str, property: &str) -> GdbResult<bool> {
        if let Some(props) = self.indexes.get_mut(label) {
            Ok(props.remove(property))
        } else {
            Ok(false)
        }
    }

    pub fn has_index(&self, label: &str, property: &str) -> bool {
        self.indexes
            .get(label)
            .map(|set| set.contains(property))
            .unwrap_or(false)
    }

    pub fn get_indexes(&self, label: &str) -> Vec<String> {
        self.indexes
            .get(label)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_vertex_schema(&self, label: &str) -> Option<&VertexSchema> {
        self.labels.get(label)
    }

    pub fn get_vertex_schema_by_id(&self, id: LabelId) -> Option<&VertexSchema> {
        self.label_ids.get(&id).and_then(|name| self.labels.get(name))
    }

    pub fn get_edge_schema(&self, edge_type_name: &str) -> Option<&EdgeSchema> {
        self.edges.get(edge_type_name)
    }

    pub fn get_edge_schema_by_type(&self, edge_type: EdgeType) -> Option<&EdgeSchema> {
        self.edge_types.get(&edge_type).and_then(|name| self.edges.get(name))
    }

    pub fn drop_vertex_label(&mut self, label: &str) -> GdbResult<LabelId> {
        if let Some(schema) = self.labels.remove(label) {
            self.label_ids.remove(&schema.label_id);
            self.indexes.remove(label);
            Ok(schema.label_id)
        } else {
            Err(GdbError::Schema(format!("Vertex label '{}' does not exist", label)))
        }
    }

    pub fn drop_edge_type(&mut self, edge_type_name: &str) -> GdbResult<EdgeType> {
        if let Some(schema) = self.edges.remove(edge_type_name) {
            self.edge_types.remove(&schema.edge_type);
            Ok(schema.edge_type)
        } else {
            Err(GdbError::Schema(format!("Edge type '{}' does not exist", edge_type_name)))
        }
    }

    pub fn alter_vertex_label(
        &mut self,
        label: &str,
        add_props: Vec<PropertySpec>,
        drop_props: Vec<String>,
    ) -> GdbResult<()> {
        let schema = self
            .labels
            .get_mut(label)
            .ok_or_else(|| GdbError::Schema(format!("Vertex label '{}' does not exist", label)))?;

        // Drop specified properties
        for dp in &drop_props {
            schema.properties.retain(|p| &p.name != dp);
            if let Some(idx_set) = self.indexes.get_mut(label) {
                idx_set.remove(dp);
            }
        }

        // Add new properties
        for ap in add_props {
            if schema.properties.iter().any(|p| p.name == ap.name) {
                return Err(GdbError::Schema(format!(
                    "Property '{}' already exists on label '{}'",
                    ap.name, label
                )));
            }
            schema.properties.push(ap);
        }

        Ok(())
    }

    pub fn alter_edge_type(
        &mut self,
        edge_type_name: &str,
        add_props: Vec<PropertySpec>,
        drop_props: Vec<String>,
    ) -> GdbResult<()> {
        let schema = self
            .edges
            .get_mut(edge_type_name)
            .ok_or_else(|| GdbError::Schema(format!("Edge type '{}' does not exist", edge_type_name)))?;

        for dp in &drop_props {
            schema.properties.retain(|p| &p.name != dp);
        }

        for ap in add_props {
            if schema.properties.iter().any(|p| p.name == ap.name) {
                return Err(GdbError::Schema(format!(
                    "Property '{}' already exists on edge type '{}'",
                    ap.name, edge_type_name
                )));
            }
            schema.properties.push(ap);
        }

        Ok(())
    }

    pub fn list_vertex_schemas(&self) -> Vec<&VertexSchema> {
        let mut list: Vec<&VertexSchema> = self.labels.values().collect();
        list.sort_by_key(|s| s.label_id.0);
        list
    }

    pub fn list_edge_schemas(&self) -> Vec<&EdgeSchema> {
        let mut list: Vec<&EdgeSchema> = self.edges.values().collect();
        list.sort_by_key(|s| s.edge_type.0);
        list
    }

    pub fn list_vertex_labels(&self) -> Vec<String> {
        let mut list: Vec<String> = self.labels.keys().cloned().collect();
        list.sort();
        list
    }

    pub fn list_edge_types(&self) -> Vec<String> {
        let mut list: Vec<String> = self.edges.keys().cloned().collect();
        list.sort();
        list
    }
}

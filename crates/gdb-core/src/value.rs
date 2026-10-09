use crate::error::{GdbError, GdbResult};
use arrow::array::{
    Array, ArrayRef, BooleanArray, Float64Array, Int64Array, StringArray,
};
use arrow::datatypes::DataType as ArrowDataType;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

#[derive(Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum DataValue {
    Null,
    Boolean(bool),
    Int64(i64),
    Float64(f64),
    String(String),
    Date(i32),      // Days since epoch
    Timestamp(i64), // Microseconds since epoch
    List(Vec<DataValue>),
}

impl Eq for DataValue {}

impl std::hash::Hash for DataValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            DataValue::Null => {}
            DataValue::Boolean(b) => b.hash(state),
            DataValue::Int64(i) => i.hash(state),
            DataValue::Float64(f) => {
                let bits = if f.is_nan() {
                    f64::NAN.to_bits()
                } else if *f == 0.0 {
                    0.0f64.to_bits()
                } else {
                    f.to_bits()
                };
                bits.hash(state);
            }
            DataValue::String(s) => s.hash(state),
            DataValue::Date(d) => d.hash(state),
            DataValue::Timestamp(t) => t.hash(state),
            DataValue::List(l) => l.hash(state),
        }
    }
}

impl fmt::Debug for DataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataValue::Null => write!(f, "NULL"),
            DataValue::Boolean(b) => write!(f, "{}", b),
            DataValue::Int64(v) => write!(f, "{}", v),
            DataValue::Float64(v) => write!(f, "{:.4}", v),
            DataValue::String(s) => write!(f, "\"{}\"", s),
            DataValue::Date(d) => write!(f, "Date({})", d),
            DataValue::Timestamp(t) => write!(f, "Timestamp({})", t),
            DataValue::List(l) => write!(f, "{:?}", l),
        }
    }
}

impl fmt::Display for DataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataValue::Null => write!(f, "NULL"),
            DataValue::Boolean(b) => write!(f, "{}", b),
            DataValue::Int64(v) => write!(f, "{}", v),
            DataValue::Float64(v) => write!(f, "{}", v),
            DataValue::String(s) => write!(f, "{}", s),
            DataValue::Date(d) => write!(f, "{}", d),
            DataValue::Timestamp(t) => write!(f, "{}", t),
            DataValue::List(l) => {
                let items: Vec<String> = l.iter().map(|item| item.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
        }
    }
}

impl DataValue {
    pub fn data_type(&self) -> ArrowDataType {
        match self {
            DataValue::Null => ArrowDataType::Null,
            DataValue::Boolean(_) => ArrowDataType::Boolean,
            DataValue::Int64(_) => ArrowDataType::Int64,
            DataValue::Float64(_) => ArrowDataType::Float64,
            DataValue::String(_) => ArrowDataType::Utf8,
            DataValue::Date(_) => ArrowDataType::Date32,
            DataValue::Timestamp(_) => ArrowDataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None),
            DataValue::List(_) => ArrowDataType::List(Arc::new(arrow::datatypes::Field::new("item", ArrowDataType::Utf8, true))),
        }
    }

    pub fn extract_from_array(array: &ArrayRef, row_idx: usize) -> GdbResult<Self> {
        if array.is_null(row_idx) {
            return Ok(DataValue::Null);
        }

        match array.data_type() {
            ArrowDataType::Boolean => {
                let arr = array.as_any().downcast_ref::<BooleanArray>()
                    .ok_or_else(|| GdbError::Internal("Downcast BooleanArray failed".into()))?;
                Ok(DataValue::Boolean(arr.value(row_idx)))
            }
            ArrowDataType::Int64 => {
                let arr = array.as_any().downcast_ref::<Int64Array>()
                    .ok_or_else(|| GdbError::Internal("Downcast Int64Array failed".into()))?;
                Ok(DataValue::Int64(arr.value(row_idx)))
            }
            ArrowDataType::UInt64 => {
                let arr = array.as_any().downcast_ref::<arrow::array::UInt64Array>()
                    .ok_or_else(|| GdbError::Internal("Downcast UInt64Array failed".into()))?;
                Ok(DataValue::Int64(arr.value(row_idx) as i64))
            }
            ArrowDataType::UInt32 => {
                let arr = array.as_any().downcast_ref::<arrow::array::UInt32Array>()
                    .ok_or_else(|| GdbError::Internal("Downcast UInt32Array failed".into()))?;
                Ok(DataValue::Int64(arr.value(row_idx) as i64))
            }
            ArrowDataType::Float64 => {
                let arr = array.as_any().downcast_ref::<Float64Array>()
                    .ok_or_else(|| GdbError::Internal("Downcast Float64Array failed".into()))?;
                Ok(DataValue::Float64(arr.value(row_idx)))
            }
            ArrowDataType::Utf8 => {
                let arr = array.as_any().downcast_ref::<StringArray>()
                    .ok_or_else(|| GdbError::Internal("Downcast StringArray failed".into()))?;
                Ok(DataValue::String(arr.value(row_idx).to_string()))
            }
            ArrowDataType::Date32 => {
                let arr = array.as_any().downcast_ref::<arrow::array::Date32Array>()
                    .ok_or_else(|| GdbError::Internal("Downcast Date32Array failed".into()))?;
                Ok(DataValue::Date(arr.value(row_idx)))
            }
            ArrowDataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, _) => {
                let arr = array.as_any().downcast_ref::<arrow::array::TimestampMicrosecondArray>()
                    .ok_or_else(|| GdbError::Internal("Downcast TimestampMicrosecondArray failed".into()))?;
                Ok(DataValue::Timestamp(arr.value(row_idx)))
            }
            other => Err(GdbError::Storage(format!("Unsupported Arrow type extraction: {:?}", other))),
        }
    }
}

impl From<bool> for DataValue {
    fn from(b: bool) -> Self {
        DataValue::Boolean(b)
    }
}

impl From<String> for DataValue {
    fn from(s: String) -> Self {
        DataValue::String(s)
    }
}

impl From<i64> for DataValue {
    fn from(v: i64) -> Self {
        DataValue::Int64(v)
    }
}

impl From<f64> for DataValue {
    fn from(v: f64) -> Self {
        DataValue::Float64(v)
    }
}

impl From<&str> for DataValue {
    fn from(s: &str) -> Self {
        DataValue::String(s.to_string())
    }
}

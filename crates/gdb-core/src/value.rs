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
    Vector(Vec<f32>),
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
            DataValue::Vector(v) => {
                for x in v {
                    let bits = if x.is_nan() {
                        f32::NAN.to_bits()
                    } else if *x == 0.0 {
                        0.0f32.to_bits()
                    } else {
                        x.to_bits()
                    };
                    bits.hash(state);
                }
            }
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
            DataValue::Vector(v) => write!(f, "Vector({:?})", v),
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
            DataValue::Vector(v) => {
                let items: Vec<String> = v.iter().map(|x| format!("{:.4}", x)).collect();
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
            DataValue::Vector(v) => ArrowDataType::FixedSizeList(
                Arc::new(arrow::datatypes::Field::new("item", ArrowDataType::Float32, false)),
                v.len() as i32,
            ),
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
            ArrowDataType::FixedSizeList(_, _) => {
                let arr = array.as_any().downcast_ref::<arrow::array::FixedSizeListArray>()
                    .ok_or_else(|| GdbError::Internal("Downcast FixedSizeListArray failed".into()))?;
                let value_arr = arr.value(row_idx);
                let float_arr = value_arr.as_any().downcast_ref::<arrow::array::Float32Array>()
                    .ok_or_else(|| GdbError::Internal("Downcast Float32Array failed".into()))?;
                let mut vec = Vec::with_capacity(float_arr.len());
                for i in 0..float_arr.len() {
                    vec.push(float_arr.value(i));
                }
                Ok(DataValue::Vector(vec))
            }
            other => Err(GdbError::Storage(format!("Unsupported Arrow type extraction: {:?}", other))),
        }
    }
}

impl From<Vec<f32>> for DataValue {
    fn from(v: Vec<f32>) -> Self {
        DataValue::Vector(v)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datavalue_conversions_and_display() {
        let v_int = DataValue::from(42i64);
        let v_float = DataValue::from(3.14f64);
        let v_bool = DataValue::from(true);
        let v_str = DataValue::from("hello");
        let v_vec = DataValue::from(vec![1.0f32, 2.0f32, 3.0f32]);
        let v_null = DataValue::Null;

        assert_eq!(format!("{}", v_int), "42");
        assert_eq!(format!("{}", v_float), "3.14");
        assert_eq!(format!("{}", v_bool), "true");
        assert_eq!(format!("{}", v_str), "hello");
        assert_eq!(format!("{}", v_vec), "[1.0000, 2.0000, 3.0000]");
        assert_eq!(format!("{}", v_null), "NULL");

        assert_eq!(v_int.data_type(), ArrowDataType::Int64);
        assert_eq!(v_float.data_type(), ArrowDataType::Float64);
        assert_eq!(v_bool.data_type(), ArrowDataType::Boolean);
        assert_eq!(v_str.data_type(), ArrowDataType::Utf8);
        assert_eq!(v_null.data_type(), ArrowDataType::Null);

        let dt_vec = crate::schema::DataType::Vector(4);
        let arrow_dt = dt_vec.to_arrow();
        assert!(matches!(arrow_dt, ArrowDataType::FixedSizeList(_, 4)));

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::Hash;
        v_vec.hash(&mut hasher);
        assert_eq!(v_vec, DataValue::Vector(vec![1.0, 2.0, 3.0]));
        assert_ne!(v_vec, DataValue::Vector(vec![1.0, 2.0, 4.0]));
    }

    #[test]
    fn test_from_arrow_all_types() {
        use arrow::array::{
            BooleanArray, Date32Array, FixedSizeListArray, Float32Array, Float64Array,
            Int64Array, StringArray, TimestampMicrosecondArray, UInt32Array, UInt64Array,
        };
        use arrow::datatypes::Field;

        let b_arr = Arc::new(BooleanArray::from(vec![Some(true), None])) as ArrayRef;
        assert_eq!(DataValue::extract_from_array(&b_arr, 0).unwrap(), DataValue::Boolean(true));
        assert_eq!(DataValue::extract_from_array(&b_arr, 1).unwrap(), DataValue::Null);

        let i_arr = Arc::new(Int64Array::from(vec![42])) as ArrayRef;
        assert_eq!(DataValue::extract_from_array(&i_arr, 0).unwrap(), DataValue::Int64(42));

        let u64_arr = Arc::new(UInt64Array::from(vec![100])) as ArrayRef;
        assert_eq!(DataValue::extract_from_array(&u64_arr, 0).unwrap(), DataValue::Int64(100));

        let u32_arr = Arc::new(UInt32Array::from(vec![50])) as ArrayRef;
        assert_eq!(DataValue::extract_from_array(&u32_arr, 0).unwrap(), DataValue::Int64(50));

        let f_arr = Arc::new(Float64Array::from(vec![2.718])) as ArrayRef;
        assert_eq!(DataValue::extract_from_array(&f_arr, 0).unwrap(), DataValue::Float64(2.718));

        let s_arr = Arc::new(StringArray::from(vec!["gdb"])) as ArrayRef;
        assert_eq!(DataValue::extract_from_array(&s_arr, 0).unwrap(), DataValue::String("gdb".into()));

        let d_arr = Arc::new(Date32Array::from(vec![19000])) as ArrayRef;
        assert_eq!(DataValue::extract_from_array(&d_arr, 0).unwrap(), DataValue::Date(19000));

        let ts_arr = Arc::new(TimestampMicrosecondArray::from(vec![123456789])) as ArrayRef;
        assert_eq!(DataValue::extract_from_array(&ts_arr, 0).unwrap(), DataValue::Timestamp(123456789));

        let values = Float32Array::from(vec![0.1f32, 0.2f32]);
        let field = Arc::new(Field::new("item", ArrowDataType::Float32, false));
        let list_arr = Arc::new(FixedSizeListArray::new(
            field,
            2,
            Arc::new(values),
            None,
        )) as ArrayRef;
        assert_eq!(
            DataValue::extract_from_array(&list_arr, 0).unwrap(),
            DataValue::Vector(vec![0.1, 0.2])
        );

        let unsupported = Arc::new(arrow::array::Int8Array::from(vec![1])) as ArrayRef;
        assert!(DataValue::extract_from_array(&unsupported, 0).is_err());
    }
}

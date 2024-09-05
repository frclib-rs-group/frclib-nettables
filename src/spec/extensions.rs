
use frclib_core::value::{FrcType, FrcTimestampedValue as TimestampedValue};

use crate::NetworkTablesError;

pub trait FrcTypeExt {
    fn as_u8(&self) -> u8;
    fn from_u8(byte: u8) -> Option<Self>
    where
        Self: Sized;
    fn stringify(&self) -> String;
}

impl FrcTypeExt for FrcType {
    fn as_u8(&self) -> u8 {
        match self {
            Self::Boolean => 0,
            Self::Double => 1,
            Self::Int => 2,
            Self::Float => 3,
            Self::String => 4,
            Self::Raw | Self::Void | Self::Struct(_) | Self::StructArray(_) => 5,
            Self::BooleanArray => 16,
            Self::DoubleArray => 17,
            Self::IntArray => 18,
            Self::FloatArray => 19,
            Self::StringArray => 20,
        }
    }
    fn from_u8(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Boolean),
            1 => Some(Self::Double),
            2 => Some(Self::Int),
            3 => Some(Self::Float),
            4 => Some(Self::String),
            5 => Some(Self::Raw),
            16 => Some(Self::BooleanArray),
            17 => Some(Self::DoubleArray),
            18 => Some(Self::IntArray),
            19 => Some(Self::FloatArray),
            20 => Some(Self::StringArray),
            _ => None,
        }
    }
    fn stringify(&self) -> String {
        match self {
            Self::Boolean => "boolean",
            Self::Double => "double",
            Self::Int => "int",
            Self::Float => "float",
            Self::String => "string",
            Self::Raw | Self::Void => "raw",
            Self::Struct(desc) => desc.type_str,
            Self::StructArray(desc) => {
                return format!("{}[]", desc.type_str);
            }
            Self::BooleanArray => "boolean[]",
            Self::DoubleArray => "double[]",
            Self::IntArray => "int[]",
            Self::FloatArray => "float[]",
            Self::StringArray => "string[]",
        }
        .to_string()
    }
}

pub trait FrcTimestampedValueExt {
    fn as_bytes(&self, topic_id: i32) -> Result<Vec<u8>, NetworkTablesError>;
    fn replace_inplace(&mut self, other: Self);
}

impl FrcTimestampedValueExt for TimestampedValue {
    fn as_bytes(&self, topic_id: i32) -> Result<Vec<u8>, NetworkTablesError> {
        let mut buf = Vec::new();
        rmp::encode::write_array_len(&mut buf, 4)?;
        rmp::encode::write_i32(&mut buf, topic_id)?;
        rmp::encode::write_uint(&mut buf, self.timestamp)?;
        rmp::encode::write_u8(&mut buf, self.value.get_type().as_u8())?;
        rmpv::encode::write_value(&mut buf, &rmpv::Value::from(self.value.clone()))?;
        Ok(buf)
    }

    fn replace_inplace(&mut self, other: Self) {
        self.timestamp = other.timestamp;
        self.value = other.value;
    }
}

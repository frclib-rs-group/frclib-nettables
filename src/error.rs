use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkTablesError {
    #[error("Json error: {0:?}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("Io error: {0:?}")]
    Io(#[from] std::io::Error),
    #[error("MsgPack write error: {0:?}")]
    MsgPackWrite(#[from] rmp::encode::ValueWriteError),
    #[error("Internal Server Error")]
    ServerError(#[from] crate::server::error::NTServerError),
    #[error("Wrong metatopic type")]
    WrongMetaTopicType,
}

impl Serialize for NetworkTablesError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(format!("{self}").as_str())
    }
}

/// A unit type that represents a successful operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize, Default)]
pub struct NtSuccess;
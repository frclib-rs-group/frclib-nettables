use frclib_core::value::FrcType;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NTServerError {
    #[error("Expected client does not exist: {0}")]
    ClientDoesNotExist(String),
    #[error("Expected client already exists: {0}")]
    ClientAlreadyExists(String),
    #[error("Expected topic does not exist: {0}")]
    TopicDoesNotExist(String),
    #[error("Expected subscription does not exist: {0}")]
    SubscriptionDoesNotExist(String),
    #[error("Wrong type of value for topic. got: {0}, wanted: {1}")]
    WrongValueType(FrcType, FrcType),
    #[error("Expected a meta-topic, got a non-meta topic: {0}")]
    NotMetaTopic(String),
    #[error("Expected a normal-topic, got a meta topic: {0}")]
    NotNormalTopic(String),
}
impl Serialize for NTServerError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

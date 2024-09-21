use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub mod messages;
pub mod metatopic;

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#subscription-options)
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct SubscriptionOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub periodic: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topics_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<bool>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>
}


/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#properties)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub struct PublishProperties {
    /// If true, the last set value will be periodically saved to persistent storage on the server and be restored during server startup.
    /// Topics with this property set to true will not be deleted by the server when the last publisher stops publishing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persistent: Option<bool>,
    /// Topics with this property set to true will not be deleted by the server when the last publisher stops publishing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retained: Option<bool>,
    /// If false, the server and clients will not store the value of the topic. This means that only value updates will be available for the topic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<bool>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>
}
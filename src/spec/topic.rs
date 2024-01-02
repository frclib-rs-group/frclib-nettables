use frclib_core::value::{FrcType, FrcTimestampedValue as TimestampedValue};
use serde::{Deserialize, Serialize};

use super::messages::{NTMessage, PublishTopic, UnpublishTopic};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub struct PublishedTopic {
    pub name: String,
    pub pubuid: u32,
    pub r#type: FrcType,
    pub properties: Option<PublishProperties>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub struct Topic {
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: FrcType,
    pub properties: Option<PublishProperties>,
}

impl Topic {
    ///Easier way to make a new meta-topic
    pub(crate) const fn new_meta(name: String) -> Self {
        Self {
            name,
            r#type: FrcType::Raw,
            properties: None,
        }
    }

    pub fn from_publish_topic(pub_topic: PublishTopic) -> Self {
        Self {
            name: pub_topic.name,
            r#type: pub_topic.r#type,
            properties: pub_topic.properties,
        }
    }

    /// Returns true if either persistent of retained is true in properties
    pub fn has_implicit_publisher(&self) -> bool {
        if let Some(props) = self.properties {
            if props.retained == Some(true) { return true };
            if props.persistent == Some(true) { return true };
        }
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Copy)]
#[serde(rename_all = "lowercase")]
pub struct PublishProperties {
    /// If true, the last set value will be periodically saved to persistent storage on the server and be restored during server startup.
    /// Topics with this property set to true will not be deleted by the server when the last publisher stops publishing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persistent: Option<bool>,
    /// Topics with this property set to true will not be deleted by the server when the last publisher stops publishing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retained: Option<bool>,
}

impl PublishedTopic {
    pub const fn as_unpublish(&self) -> NTMessage {
        NTMessage::Unpublish(UnpublishTopic {
            pubuid: self.pubuid,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TopicValue {
    pub(crate) topic: String,
    pub(crate) value: TimestampedValue,
}

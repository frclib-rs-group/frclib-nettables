use frclib_core::value::{FrcTimestampedValue, FrcType};
use serde::Serialize;

use crate::spec::{messages::PublishTopic, PublishProperties};

use super::extensions::FrcTypeExt;

/// [Nt4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#supported-data-types)
#[derive(Debug, Clone)]
pub enum TopicType {
    Frc(FrcType),
    Arbitrary(String),
}
impl serde::Serialize for TopicType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        match self {
            TopicType::Frc(frc_type) => frc_type.stringify().serialize(serializer),
            TopicType::Arbitrary(arb_type) => arb_type.serialize(serializer),
        }
    }
}


#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub struct Topic {
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub properties: Option<PublishProperties>,
}

impl Topic {
    ///Easier way to make a new meta-topic
    pub(crate) fn new_meta(name: String) -> Self {
        Self {
            name,
            r#type: String::from("raw"),
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
        if let Some(props) = &self.properties {
            if props.retained == Some(true) { return true };
            if props.persistent == Some(true) { return true };
        }
        false
    }
}


#[derive(Debug, Clone)]
pub struct TopicValue {
    pub(crate) topic: String,
    pub(crate) value: FrcTimestampedValue,
}

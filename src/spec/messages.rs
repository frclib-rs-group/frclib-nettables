use std::{collections::HashSet, net::SocketAddr};

use frclib_core::value::FrcType;
use serde::{Deserialize, Serialize};

use super::{subscription::SubscriptionOptions, topic::PublishProperties};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "lowercase")]
pub enum NTMessage {
    Publish(PublishTopic),
    Unpublish(UnpublishTopic),
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    SetProperties(SetProperties),
    Announce(Announce),
    UnAnnounce(UnAnnounce),
    Properties(Properties),
    //only used internally by the server
    Close(Close),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct PublishTopic {
    pub name: String,
    pub pubuid: u32,
    pub r#type: FrcType,
    /// Initial topic properties.
    /// If the topic is newly created (e.g. there are no other publishers) this sets the topic properties.
    /// If the topic was previously published, this is ignored. The announce message contains the actual topic properties.
    /// Clients can use the setproperties message to change properties after topic creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<PublishProperties>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(missing_copy_implementations)]
pub struct UnpublishTopic {
    pub pubuid: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Subscribe {
    pub subuid: i32,
    pub topics: HashSet<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<SubscriptionOptions>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(missing_copy_implementations)]
pub struct Unsubscribe {
    pub subuid: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct SetProperties {
    pub name: String,
    pub update: PublishProperties,
}

/// The server shall send this message for each of the following conditions:
/// - To all clients subscribed to a matching prefix when a topic is created
/// - To a client in response to an Publish Request Message (publish) from that client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Announce {
    /// Topic name
    pub name: String,
    /// Topic id
    pub id: i32,
    /// Topic type
    pub r#type: FrcType,
    /// If this message was sent in response to a publish message,
    /// the Publisher UID provided in that message. Otherwise absent.
    pub pubuid: Option<i32>,
    /// Topic properties
    pub properties: PublishProperties,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct UnAnnounce {
    /// Topic name
    pub name: String,
    /// Topic id
    pub id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Properties {
    /// Topic name
    pub name: String,
    /// Acknowledgement - True if this message is in response to a setproperties message from the same client.
    /// Otherwise absent.
    pub ack: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Close {
    pub addr: SocketAddr,
    pub name: String,
}

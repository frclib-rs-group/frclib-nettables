use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{SubscriptionOptions, PublishProperties};

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#publish-messages-client-to-server)
/// 
/// Sent from a client to the server to indicate the client wants to start publishing values at the given topic.
/// The server shall respond with a [`Announce`], even if the topic was previously announced.
/// The client can start publishing data values via MessagePack messages immediately after sending this message,
/// but the messages will be ignored by the server if the publisher data type does not match the topic data type.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PublishTopic {
    pub name: String,
    pub pubuid: u32,
    #[serde(rename = "type")]
    pub r#type: String,
    /// Initial topic properties.
    /// If the topic is newly created (e.g. there are no other publishers) this sets the topic properties.
    /// If the topic was previously published, this is ignored. The announce message contains the actual topic properties.
    /// Clients can use the setproperties message to change properties after topic creation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<PublishProperties>,
}

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#publish-release-message-unpublish)
/// 
/// Sent from a client to the server to indicate the client wants to stop publishing values for the given topic and publisher.
/// The client should stop publishing data value updates via binary MessagePack messages for this publisher prior to sending this message.
///
/// When there are no remaining publishers for a non-persistent topic,
/// the server shall delete the topic and send an [`UnAnnounce`]
/// to all clients who have been sent a previous [`Announce`] for the topic.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct UnpublishTopic {
    pub pubuid: u32,
}

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#set-properties-message-setproperties)
/// 
/// Sent from a client to the server to change properties (see [`PublishProperties`]) for a given topic.
/// The server will send a corresponding [`Properties`] to all subscribers to the topic (if the topic is published).
/// This message shall be ignored by the server if the topic is not published.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetProperties {
    pub name: String,
    pub update: PublishProperties,
}

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#subscribe-message-subscribe)
/// 
/// Sent from a client to the server to indicate the client wants to subscribe to value changes for the specified topics / groups of topics.
/// The server shall send MessagePack messages containing the current values for any existing cached topics upon receipt,
/// and continue sending MessagePack messages for future value changes.
/// If a topic does not yet exist, no message is sent until it is created (via a publish),
/// at which point an [`Announce`] will be sent and MessagePack messages will automatically follow as they are published.
///
/// Subscriptions may overlap;
/// 
/// only one MessagePack message is sent per value change regardless of the number of subscriptions.
/// Sending a subscribe message with the same subscription UID as a previous subscribe message results
/// in updating the subscription (replacing the array of identifiers and updating any specified options).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subscribe {
    pub subuid: i32,
    pub topics: HashSet<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<SubscriptionOptions>,
}

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#unsubscribe-message-unsubscribe)
/// 
/// Sent from a client to the server to indicate the client wants to stop subscribing to messages for the given subscription.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Unsubscribe {
    pub subuid: i32,
}

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#topic-announcement-message-announce)
/// 
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
    pub r#type: String,
    /// If this message was sent in response to a publish message,
    /// the Publisher UID provided in that message. Otherwise absent.
    pub pubuid: Option<i32>,
    /// Topic properties
    pub properties: PublishProperties,
}

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#topic-removed-message-unannounce)
/// 
/// The server shall send this message when a previously announced (via a [`Announce`]) topic is deleted.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct UnAnnounce {
    /// Topic name
    pub name: String,
    /// Topic id
    pub id: u32,
}

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#properties-update-message-properties)
/// 
/// The server shall send this message when a previously announced (via an [`Announce`]) topic has its properties changed (via [`SetProperties`]).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Properties {
    /// Topic name
    pub name: String,
    /// Acknowledgement - True if this message is in response to a setproperties message from the same client.
    /// Otherwise absent.
    pub ack: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "lowercase")]
pub enum NTMessage {
    Publish(PublishTopic),
    Unpublish(UnpublishTopic),
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    SetProperties(SetProperties),
    Announce(Announce),
    UnAnnounce(UnAnnounce),
    Properties(Properties)
}
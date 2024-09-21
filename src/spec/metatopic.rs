use serde::{Deserialize, Serialize};

use crate::{now, NetworkTablesError};

use super::SubscriptionOptions;

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#connected-clients-clients)
/// 
/// The server shall update this topic when a client connects or disconnects.
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct ClientMetaValue {
    pub(crate) id: String,
    ///Connection information about the client; typically host:port
    pub(crate) conn: String,
}

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#client-subscriptions-clientsubclient)
/// 
/// The server shall update this topic when the corresponding client subscribes or unsubscribes to any topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSubscriptionMetaValue {
    ///A client-generated unique identifier for this subscription
    pub(crate) uid: u32,
    ///One or more topic names or prefixes (if the `prefix` option is true) that messages are sent for
    pub(crate) topics: Vec<String>,
    pub(crate) options: SubscriptionOptions,
}
impl PartialEq for ClientSubscriptionMetaValue {
    fn eq(&self, other: &Self) -> bool {
        self.uid == other.uid && self.topics == other.topics
    }
}
impl Eq for ClientSubscriptionMetaValue {}
impl std::hash::Hash for ClientSubscriptionMetaValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.uid.hash(state);
        self.topics.hash(state);
    }
}
/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#server-subscriptions-serversub)
/// 
/// Same as [`ClientSubscriptionMetaValue`], except it’s updated when the server subscribes or unsubscribes to any topic.
pub type ServerSubscriptionMetaValue = ClientSubscriptionMetaValue;

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#subscriptions-subtopic)
/// 
/// The server shall update this topic when a client subscribes or unsubscribes to <topic>.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionMetaValue {
    ///Empty string for server subscriptions
    pub(crate) client: String,
    ///A client-generated unique identifier for this subscription
    pub(crate) subuid: u32,
    pub(crate) options: SubscriptionOptions,
}
impl PartialEq for SubscriptionMetaValue {
    fn eq(&self, other: &Self) -> bool {
        self.client == other.client && self.subuid == other.subuid
    }
}
impl Eq for SubscriptionMetaValue {}
impl std::hash::Hash for SubscriptionMetaValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.client.hash(state);
        self.subuid.hash(state);
    }
}

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#client-publishers-clientpubclient)
/// 
/// The server shall update this topic when the corresponding client publishes or unpublishes any topic.
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct ClientPublisherMetaValue {
    pub(crate) topic: String,
    ///A client-generated unique identifier for this publisher
    pub(crate) uid: u32,
}
/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#server-publishers-serverpub)
/// 
/// Same as [`ClientPublisherMetaValue`], except it’s updated when the server publishes or unpublishes any topic.
pub type ServerPublisherMetaValue = ClientPublisherMetaValue;

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#publishers-pubtopic)
/// 
/// The server shall update this topic when a client publishes or unpublishes to <topic>.
#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct PublisherMetaValue {
    ///Empty string for server publishers
    pub(crate) client: String,
    ///A client-generated unique identifier for this publisher
    pub(crate) pubuid: u32,
}

/// [NT4 Spec Equivalent](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables4.adoc#server-published-meta-topics)
/// 
/// The server shall publish a standard set of topics with information about server state.
/// Clients may subscribe to these topics for diagnostics purposes or to determine when to publish value changes.
/// These topics are hidden—​they are not announced to subscribers to an empty prefix,
/// only to subscribers that have subscribed to "$" or longer prefixes.
#[derive(Debug, Clone)]
pub enum MetaTopicValue {
    Publisher(Vec<PublisherMetaValue>),
    ClientPublisher(Vec<ClientPublisherMetaValue>),
    ServerPublisher(Vec<ServerPublisherMetaValue>),
    Subscription(Vec<SubscriptionMetaValue>),
    ClientSubscription(Vec<ClientSubscriptionMetaValue>),
    ServerSubscription(Vec<ServerSubscriptionMetaValue>),
    Client(Vec<ClientMetaValue>),
}
impl MetaTopicValue {
    pub fn merge(&mut self, other: Self) {
        match self {
            Self::Publisher(inner) => {
                if let Self::Publisher(other_inner) = other {
                    inner.extend(other_inner);
                }
            }
            Self::ClientPublisher(inner) => {
                if let Self::ClientPublisher(other_inner) = other {
                    inner.extend(other_inner);
                }
            }
            Self::ServerPublisher(inner) => {
                if let Self::ServerPublisher(other_inner) = other {
                    inner.extend(other_inner);
                }
            }
            Self::Subscription(inner) => {
                if let Self::Subscription(other_inner) = other {
                    inner.extend(other_inner);
                }
            }
            Self::ClientSubscription(inner) => {
                if let Self::ClientSubscription(other_inner) = other {
                    inner.extend(other_inner);
                }
            }
            Self::ServerSubscription(inner) => {
                if let Self::ServerSubscription(other_inner) = other {
                    inner.extend(other_inner);
                }
            }
            Self::Client(inner) => {
                if let Self::Client(other_inner) = other {
                    inner.extend(other_inner);
                }
            }
        }
    }

    pub fn reduce(&mut self, other: Self) {
        match self {
            Self::Publisher(inner) => {
                if let Self::Publisher(other_inner) = other {
                    inner.retain(|x| !other_inner.contains(x));
                }
            }
            Self::ClientPublisher(inner) => {
                if let Self::ClientPublisher(other_inner) = other {
                    inner.retain(|x| !other_inner.contains(x));
                }
            }
            Self::ServerPublisher(inner) => {
                if let Self::ServerPublisher(other_inner) = other {
                    inner.retain(|x| !other_inner.contains(x));
                }
            }
            Self::Subscription(inner) => {
                if let Self::Subscription(other_inner) = other {
                    inner.retain(|x| !other_inner.contains(x));
                }
            }
            Self::ClientSubscription(inner) => {
                if let Self::ClientSubscription(other_inner) = other {
                    inner.retain(|x| !other_inner.contains(x));
                }
            }
            Self::ServerSubscription(inner) => {
                if let Self::ServerSubscription(other_inner) = other {
                    inner.retain(|x| !other_inner.contains(x));
                }
            }
            Self::Client(inner) => {
                if let Self::Client(other_inner) = other {
                    inner.retain(|x| !other_inner.contains(x));
                }
            }
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn as_bytes(&self, topic_id: i32) -> Result<Vec<u8>, NetworkTablesError> {
        let mut value = Vec::new();
        match self {
            Self::Publisher(inner) => {
                rmp::encode::write_array_len(&mut value, inner.len() as u32)?;
                for mte in inner {
                    value.extend(rmp_serde::to_vec_named(mte).unwrap());
                }
            }
            Self::ClientPublisher(inner) => {
                rmp::encode::write_array_len(&mut value, inner.len() as u32)?;
                for mte in inner {
                    value.extend(rmp_serde::to_vec_named(mte).unwrap());
                }
            }
            Self::ServerPublisher(inner) => {
                rmp::encode::write_array_len(&mut value, inner.len() as u32)?;
                for mte in inner {
                    value.extend(rmp_serde::to_vec_named(mte).unwrap());
                }
            }
            Self::Subscription(inner) => {
                rmp::encode::write_array_len(&mut value, inner.len() as u32)?;
                for mte in inner {
                    value.extend(rmp_serde::to_vec_named(mte).unwrap());
                }
            }
            Self::ClientSubscription(inner) => {
                rmp::encode::write_array_len(&mut value, inner.len() as u32)?;
                for mte in inner {
                    value.extend(rmp_serde::to_vec_named(mte).unwrap());
                }
            }
            Self::ServerSubscription(inner) => {
                rmp::encode::write_array_len(&mut value, inner.len() as u32)?;
                for mte in inner {
                    value.extend(rmp_serde::to_vec_named(mte).unwrap());
                }
            }
            Self::Client(inner) => {
                rmp::encode::write_array_len(&mut value, inner.len() as u32)?;
                for mte in inner {
                    value.extend(rmp_serde::to_vec_named(mte).unwrap());
                }
            }
        };

        let mut buf = Vec::new();
        rmp::encode::write_array_len(&mut buf, 4)?;
        rmp::encode::write_i32(&mut buf, topic_id)?;
        rmp::encode::write_uint(&mut buf, now())?;
        rmp::encode::write_u8(&mut buf, 5)?;
        buf.extend_from_slice(&value);

        Ok(buf)
    }
}

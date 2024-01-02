// use std::collections::VecDeque;

use frclib_core::value::FrcType;
use serde::{Deserialize, Serialize};

use crate::{NetworkTablesError, now};

use super::{extensions::FrcTypeExt, subscription::SubscriptionOptions};

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct PublisherMTE {
    ///Empty string for server publishers
    pub(crate) client: String,
    ///A client-generated unique identifier for this publisher
    pub(crate) pubuid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct ClientPublisherMTE {
    pub(crate) topic: String,
    ///A client-generated unique identifier for this publisher
    pub(crate) uid: u32,
}
pub type ServerPublisherMTE = ClientPublisherMTE;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionMTE {
    ///Empty string for server subscriptions
    pub(crate) client: String,
    ///A client-generated unique identifier for this subscription
    pub(crate) subuid: u32,
    pub(crate) options: SubscriptionOptions,
}
impl PartialEq for SubscriptionMTE {
    fn eq(&self, other: &Self) -> bool {
        self.client == other.client && self.subuid == other.subuid
    }
}
impl Eq for SubscriptionMTE {}
impl std::hash::Hash for SubscriptionMTE {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.client.hash(state);
        self.subuid.hash(state);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSubscriptionMTE {
    ///A client-generated unique identifier for this subscription
    pub(crate) uid: u32,
    ///One or more topic names or prefixes (if the `prefix` option is true) that messages are sent for
    pub(crate) topics: Vec<String>,
    pub(crate) options: SubscriptionOptions,
}
impl PartialEq for ClientSubscriptionMTE {
    fn eq(&self, other: &Self) -> bool {
        self.uid == other.uid && self.topics == other.topics
    }
}
impl Eq for ClientSubscriptionMTE {}
impl std::hash::Hash for ClientSubscriptionMTE {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.uid.hash(state);
        self.topics.hash(state);
    }
}
pub type ServerSubscriptionMTE = ClientSubscriptionMTE;

#[derive(Debug, Clone, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct ClientMTE {
    pub(crate) id: String,
    ///Connection information about the client; typically host:port
    pub(crate) conn: String,
}

#[derive(Debug, Clone)]
pub enum MetaTopicValue {
    Publisher(Vec<PublisherMTE>),
    ClientPublisher(Vec<ClientPublisherMTE>),
    ServerPublisher(Vec<ServerPublisherMTE>),
    Subscription(Vec<SubscriptionMTE>),
    ClientSubscription(Vec<ClientSubscriptionMTE>),
    ServerSubscription(Vec<ServerSubscriptionMTE>),
    Client(Vec<ClientMTE>),
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
        rmp::encode::write_u8(&mut buf, FrcTypeExt::as_u8(&FrcType::Raw))?;
        buf.extend_from_slice(&value);

        Ok(buf)
    }
}

// #[derive(Debug, Clone, Hash, PartialEq, Eq)]
// pub enum MetaTopicVariants {
//     Clients,
//     ServerSub,
//     ClientSub{ client_name: String },
//     ServerPub,
//     ClientPub{ client_name: String },
//     Subs{ topic_name: String },
//     Topics{ topic_name: String },
// }

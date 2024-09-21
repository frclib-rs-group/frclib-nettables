#![allow(dead_code)]

use std::{
    collections::HashMap,
    fmt::Display,
    net::{SocketAddr, SocketAddrV4},
};

use frclib_core::value::FrcTimestampedValue as TimestampedValue;
use tungstenite::Message;

use crate::{
    spec::{
        extensions::FrcTimestampedValueExt,
        messages::{Close, NTMessage},
        metatopic::*,
    },
    NetworkTablesError,
};

///The server -> client topic idx.
/// Also used to index the topic list
pub(super) type TopicIdx = u32;
/// used to index the sub list of a topic
pub(super) type SubIdx = u32;
///internal tracker of the client pos in the client list
pub(super) type ClientIdx = u32;
///the client -> server topic idx (u32)
pub(super) type PubUID = u32;
///the client -> server sub idx (u32)
pub(super) type SubUID = u32;

#[derive(Debug, Clone)]
pub(super) struct ClientMsg {
    pub(super) client_idx: ClientIdx,
    pub(super) msg: Message,
}

/// Consists of a [SocketAddr] and a name
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(super) struct ClientIdentity(SocketAddr, String);
impl Display for ClientIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.1, self.0)
    }
}
impl ClientIdentity {
    pub(super) fn new(addr: SocketAddr, name: String) -> Self {
        Self(addr, name)
    }

    pub(super) fn server() -> Self {
        Self(
            SocketAddr::V4(SocketAddrV4::new([0, 0, 0, 0].into(), 0)),
            "".to_string(),
        )
    }

    pub(super) fn addr(&self) -> SocketAddr {
        self.0
    }
    pub(super) fn name(&self) -> &str {
        &self.1
    }
    pub(super) fn update_name(&mut self, name: String) {
        self.1 = name;
    }
    pub(super) fn as_close(self) -> NTMessage {
        NTMessage::Close(Close {
            addr: self.0,
            name: self.1,
        })
    }
    pub(super) fn to_mte(&self) -> ClientMetaValue {
        ClientMetaValue {
            id: self.name().to_string(),
            conn: self.addr().to_string(),
        }
    }
    pub(super) fn from_display(display: String) -> Option<Self> {
        let mut split = display.split('@');
        let name = split.next()?;
        let addr = split.next()?;

        let (ipv4, port) = {
            let mut split = addr.split(':');
            let ipv4 = split
                .next()?
                .split('.')
                .map(|s| s.parse::<u8>().ok())
                .collect::<Option<Vec<_>>>()?;
            let port = split.next()?.parse::<u16>().ok()?;
            (ipv4, port)
        };

        if ipv4.len() != 4 {
            return None;
        }

        let mut addr = [0; 4];
        for (i, &byte) in ipv4.iter().enumerate() {
            addr[i] = byte;
        }

        Some(Self(SocketAddr::from((addr, port)), name.to_string()))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Counter {
    topic_idx: TopicIdx,
    sub_idx: SubIdx,
    client_idx: ClientIdx,
}
impl Counter {
    pub(super) fn next_topic_idx(&mut self) -> TopicIdx {
        let idx = self.topic_idx;
        self.topic_idx += 1;
        idx
    }

    pub(super) fn next_sub_idx(&mut self) -> SubIdx {
        let idx = self.sub_idx;
        self.sub_idx += 1;
        idx
    }

    pub(super) fn next_client_idx(&mut self) -> ClientIdx {
        let idx = self.client_idx;
        self.client_idx += 1;
        idx
    }
}

#[derive(Debug, Clone)]
pub(super) enum ServerSendableValue {
    ///A normal timestamped value
    NormalValue(TimestampedValue),
    ///A meta topic entry value
    MTEValue(MetaTopicValue),
}
impl ServerSendableValue {
    pub(super) fn new_normal(value: TimestampedValue) -> Self {
        Self::NormalValue(value)
    }
    pub(super) fn new_mte(mte: MetaTopicValue) -> Self {
        Self::MTEValue(mte)
    }
    pub(super) fn as_bytes(&self, topic_id: i32) -> Result<Vec<u8>, NetworkTablesError> {
        match self {
            Self::NormalValue(value) => value.as_bytes(topic_id),
            Self::MTEValue(mte) => mte.as_bytes(topic_id),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) enum StoredSubValue {
    ///Persists all values of the topic to be sent to the client on sub period.
    All(HashMap<TopicIdx, Vec<ServerSendableValue>>),
    ///Only persists the latest value of the topic to be sent to the client
    /// on sub period.
    One(HashMap<TopicIdx, ServerSendableValue>),
}
impl StoredSubValue {
    pub(super) fn new_all() -> Self {
        Self::All(HashMap::new())
    }
    pub(super) fn new_one() -> Self {
        Self::One(HashMap::new())
    }

    pub(super) fn add_value(&mut self, topic_idx: TopicIdx, value: ServerSendableValue) {
        match self {
            Self::All(map) => {
                if let Some(vec) = map.get_mut(&topic_idx) {
                    vec.push(value);
                } else {
                    map.insert(topic_idx, vec![value]);
                }
            }
            Self::One(map) => {
                map.insert(topic_idx, value);
            }
        }
    }

    pub(super) fn take_values(&mut self) -> HashMap<TopicIdx, Vec<ServerSendableValue>> {
        match self {
            Self::All(map) => std::mem::take(map),
            Self::One(map) => {
                let mut new_map = HashMap::new();
                for (topic_idx, value) in map.drain() {
                    new_map.insert(topic_idx, vec![value]);
                }
                new_map
            }
        }
    }
}

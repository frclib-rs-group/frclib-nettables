use std::{
    collections::HashSet,
    sync::{Arc, Weak},
};

use futures_util::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{
    messages::{NTMessage, Unsubscribe},
    topic::Topic,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageData {
    pub topic_name: String,
    pub timestamp: u32,
    pub r#type: String,
    pub data: rmpv::Value,
}

#[derive(Debug, Clone)]
pub struct Subscription {
    pub subuid: i32,
    pub topics: HashSet<String>,
    pub options: Option<SubscriptionOptions>,
}
impl Subscription {
    pub fn get_options(&self) -> SubscriptionOptions {
        self.options.unwrap_or_default()
    }
    pub fn cares_about(&self, topic: &String) -> bool {
        if self.get_options().prefix.unwrap_or(false) {
            self.topics
                .iter()
                .any(|topic_pat| topic.starts_with(topic_pat))
        } else {
            self.topics.iter().any(|topic_name| *topic_name == *topic)
        }
    }
}

#[derive(Debug)]
pub struct InternalSub {
    pub(crate) data: Weak<Subscription>,
    pub(crate) sender: mpsc::Sender<MessageData>,
}

#[derive(Debug)]
pub struct SubscriptionStream {
    pub data: Arc<Subscription>,
    pub receiver: mpsc::Receiver<MessageData>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Copy)]
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
}

impl InternalSub {
    pub(crate) fn is_valid(&self) -> bool {
        self.data.strong_count() != 0
    }

    #[allow(clippy::option_if_let_else)]
    pub(crate) fn matches_topic(&self, topic: &Topic) -> bool {
        if let Some(data) = self.data.upgrade() {
            let prefix = data
                .options
                .as_ref()
                .and_then(|options| options.prefix)
                .unwrap_or(false);

            if prefix {
                data.topics
                    .iter()
                    .any(|topic_pat| topic.name.starts_with(topic_pat))
            } else {
                data.topics
                    .iter()
                    .any(|topic_name| *topic_name == topic.name)
            }
        } else {
            false
        }
    }
}

impl SubscriptionStream {
    pub fn as_unsubscribe(&self) -> NTMessage {
        NTMessage::Unsubscribe(Unsubscribe {
            subuid: self.data.subuid,
        })
    }

    pub async fn next(&mut self) -> Option<MessageData> {
        self.receiver.recv().await
    }

    pub fn try_next(&mut self) -> Result<MessageData, mpsc::error::TryRecvError> {
        self.receiver.try_recv()
    }

    pub fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<MessageData>> {
        self.receiver.poll_recv(cx)
    }
}

impl Stream for SubscriptionStream {
    type Item = MessageData;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.poll_next(cx)
    }
}

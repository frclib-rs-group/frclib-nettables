use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use frclib_core::value::FrcType;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::tungstenite::Message;

use crate::{
    generate_uid,
    spec::{
        messages::{NTMessage, PublishTopic, SetProperties, Subscribe},
        subscription::{
            InternalSub, MessageData, Subscription, SubscriptionOptions, SubscriptionStream,
        },
        topic::{PublishProperties, PublishedTopic, Topic},
    },
};

use super::{
    config::ClientConfig,
    inner::{setup_socket, InnerClient},
    util::UnsignedIntOrNegativeOne,
};

///Cloning the ClientHandle will result in a new handle to the same inner client
#[derive(Debug, Clone)]
pub struct AsyncClientHandle {
    uid: u64,
    inner: Arc<InnerClient>,
}

impl AsyncClientHandle {
    pub async fn start(
        server_addr: impl Into<SocketAddr>,
        config: ClientConfig,
        identity: String,
    ) -> Result<Self, crate::NetworkTablesError> {
        let server_addr = server_addr.into();

        //hash identity to get uid
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&identity, &mut hasher);
        std::hash::Hash::hash(&generate_uid(), &mut hasher);
        std::hash::Hash::hash(&server_addr, &mut hasher);
        let uid = std::hash::Hasher::finish(&hasher);

        let (socket_sender, socket_receiver) = mpsc::channel::<Message>(100);
        let (panic_sender, panic_recv) = oneshot::channel::<crate::NetworkTablesError>();
        let inner = Arc::new(InnerClient {
            server_addr,
            subscriptions: Mutex::new(HashMap::new()),
            announced_topics: Mutex::new(HashMap::new()),
            client_published_topics: Mutex::new(HashMap::new()),
            socket_sender,
            socket_panic_receiver: Mutex::new(panic_recv),
            server_time_offset: parking_lot::Mutex::new(0),
            sub_counter: parking_lot::Mutex::new(0),
            topic_counter: parking_lot::Mutex::new(0),
            start_instant: parking_lot::Mutex::new(Instant::now()),
            //microseconds since unix epoch
            start_time: parking_lot::Mutex::new(SystemTime::now()),
            config,
            identity: identity.clone(),
        });
        setup_socket(Arc::downgrade(&inner), socket_receiver, panic_sender).await?;

        inner.on_open().await;

        // Task to handle messages from server
        let timestamp_task_client = Arc::downgrade(&inner);
        tokio::spawn(async move {
            const TIMESTAMP_INTERVAL: u64 = 5;
            loop {
                match timestamp_task_client.upgrade() {
                    Some(client) => client.update_time().await.ok(),
                    None => break,
                };

                tokio::time::sleep(Duration::from_secs(TIMESTAMP_INTERVAL)).await;
            }
        });

        Ok(Self { uid, inner })
    }

    pub fn server_addr(&self) -> SocketAddr {
        self.inner.server_addr
    }

    pub fn elapsed_server_time(&self) -> u64 {
        self.inner.server_time()
    }

    pub fn real_server_time(&self) -> u64 {
        self.inner.real_world_server_time()
    }

    pub fn to_real_time(&self, server_time: u64) -> u64 {
        self.inner.to_real_time(server_time)
    }

    pub fn clean_name(name: impl AsRef<str>) -> String {
        let mut clean = name.as_ref().to_owned();
        clean = clean.replace('$', "");
        clean = clean.replace("//", "");
        clean = clean.replace(':', "/");
        clean
    }

    pub async fn publish_topic(
        &self,
        name: impl AsRef<str>,
        topic_type: FrcType,
        properties: Option<PublishProperties>,
    ) -> Result<PublishedTopic, crate::NetworkTablesError> {
        let pubuid = self.inner.new_topic_id();
        let mut messages: Vec<NTMessage> = Vec::with_capacity(2);
        let clean_name = Self::clean_name(name);
        let publish_message = NTMessage::Publish(PublishTopic {
            name: clean_name.as_str().to_string(),
            pubuid,
            r#type: topic_type,
            properties,
        });

        if let Some(properties) = properties {
            messages.push(publish_message);
            messages.push(NTMessage::SetProperties(SetProperties {
                name: clean_name.as_str().to_string(),
                update: properties,
            }));
        } else {
            messages.push(publish_message);
        };

        // Put message in an array and serialize
        let message = serde_json::to_string(&messages)?;

        self.inner.send_message(Message::Text(message)).await?;

        let topic = PublishedTopic {
            name: clean_name,
            pubuid,
            r#type: topic_type,
            properties,
        };

        self.inner
            .client_published_topics
            .lock()
            .await
            .insert(pubuid, topic.clone());

        Ok(topic)
    }

    pub async fn unpublish(&self, topic: PublishedTopic) -> Result<(), crate::NetworkTablesError> {
        // Put message in an array and serialize
        let message = serde_json::to_string(&[topic.as_unpublish()])?;

        self.inner.send_message(Message::Text(message)).await?;

        Ok(())
    }

    #[allow(clippy::unused_async)]
    pub async fn set_properties(&self) {
        todo!()
    }

    pub async fn subscribe(
        &self,
        topic_names: &[impl ToString + Send + Sync],
    ) -> Result<SubscriptionStream, crate::NetworkTablesError> {
        self.subscribe_w_options(topic_names, None).await
    }

    pub async fn subscribe_w_options(
        &self,
        topic_names: &[impl ToString],
        options: Option<SubscriptionOptions>,
    ) -> Result<SubscriptionStream, crate::NetworkTablesError> {
        let topic_names: Vec<String> = topic_names.iter().map(ToString::to_string).collect();
        let subuid = self.inner.new_sub_id();

        // Put message in an array and serialize
        let message = serde_json::to_string(&[NTMessage::Subscribe(Subscribe {
            subuid,
            topics: HashSet::from_iter(topic_names.iter().cloned()),
            options,
        })])?;

        self.inner.send_message(Message::Text(message)).await?;

        let data = Arc::new(Subscription {
            options,
            subuid,
            topics: HashSet::from_iter(topic_names.into_iter()),
        });

        let (sender, receiver) = mpsc::channel::<MessageData>(256);
        self.inner.subscriptions.lock().await.insert(
            subuid,
            InternalSub {
                data: Arc::downgrade(&data),
                sender,
            },
        );

        Ok(SubscriptionStream { data, receiver })
    }

    pub async fn unsubscribe(
        &self,
        sub: SubscriptionStream,
    ) -> Result<(), crate::NetworkTablesError> {
        // Put message in an array and serialize
        let message = serde_json::to_string(&[sub.as_unsubscribe()])?;
        self.inner.send_message(Message::Text(message)).await?;

        // Remove from our subscriptions
        self.inner
            .subscriptions
            .lock()
            .await
            .remove(&sub.data.subuid);

        Ok(())
    }

    pub async fn publish_value_w_timestamp(
        &self,
        topic: &PublishedTopic,
        timestamp: u32,
        value: &rmpv::Value,
    ) -> Result<(), crate::NetworkTablesError> {
        self.inner
            .publish_value_w_timestamp(
                UnsignedIntOrNegativeOne::UnsignedInt(topic.pubuid),
                topic.r#type,
                timestamp,
                value,
            )
            .await
    }

    /// Value should match topic type
    pub async fn publish_value(
        &self,
        topic: &PublishedTopic,
        value: &rmpv::Value,
    ) -> Result<(), crate::NetworkTablesError> {
        self.inner
            .publish_value(
                UnsignedIntOrNegativeOne::UnsignedInt(topic.pubuid),
                topic.r#type,
                value,
            )
            .await
    }

    pub async fn use_announced_topics<F: Fn(&HashMap<i32, Topic>)>(&self, f: F) {
        f(&*self.inner.announced_topics.lock().await)
    }
}

impl PartialEq for AsyncClientHandle {
    fn eq(&self, other: &Self) -> bool {
        self.uid == other.uid
    }
}

impl Eq for AsyncClientHandle {}

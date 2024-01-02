#![allow(dead_code)]

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
};

use frclib_core::value::{FrcTimestampedValue, FrcType};
use nohash_hasher::{IntMap, IntSet};
use tokio_tungstenite::tungstenite::protocol::Message;

use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    time::Instant,
};

use crate::{
    bimap::PrimBiMap,
    error::NtSuccess,
    spec::{
        extensions::FrcTimestampedValueExt,
        messages::{
            Announce, NTMessage, PublishTopic, SetProperties, Subscribe, UnAnnounce,
            UnpublishTopic, Unsubscribe,
        },
        metatopic::{ClientPublisherMTE, MetaTopicValue},
        subscription::{Subscription, SubscriptionOptions},
        topic::{PublishProperties, Topic},
    },
    NetworkTablesError,
};

use super::{
    config::ServerConfig,
    error::NTServerError,
    util::{
        ClientIdentity, ClientIdx, Counter, PubUID, ServerSendableValue, StoredSubValue, SubIdx,
        SubUID, TopicIdx,
    },
};

#[derive(Debug, Clone)]
pub(super) struct ConnectedClient {
    pub(super) identity: ClientIdentity,
    /// A kill switch for the client, when set to true the client will be disconnected.
    pub(super) kill_bool: Arc<AtomicBool>,
    /// Client pubuid to server topic idx
    pub(super) pubuid_to_topicidx: PrimBiMap<PubUID, TopicIdx>,
    /// Client pubuid to server sub idx
    pub(super) subuid_to_subidx: PrimBiMap<SubUID, SubIdx>,
    pub(super) subs: Vec<Subscription>,
    pub(super) sub_ouput: Option<Sender<Message>>,
    pub(super) announced_topics: IntSet<TopicIdx>,
}
impl ConnectedClient {
    pub(super) fn new(
        identity: ClientIdentity,
        kill_bool: Arc<AtomicBool>,
        sub_output: Sender<Message>,
    ) -> Self {
        Self {
            identity,
            kill_bool,
            pubuid_to_topicidx: PrimBiMap::with_capacity(32),
            subuid_to_subidx: PrimBiMap::with_capacity(16),
            subs: Vec::with_capacity(16),
            sub_ouput: Some(sub_output),
            announced_topics: IntSet::with_capacity_and_hasher(24, Default::default()),
        }
    }

    pub(super) async fn send_nt_messages(
        &mut self,
        messages: &[NTMessage],
    ) -> Result<(), NetworkTablesError> {
        if self.sub_ouput.is_none() {
            return Ok(());
        }
        match serde_json::to_string(messages) {
            Ok(json) => {
                self.sub_ouput
                    .as_ref()
                    .unwrap()
                    .send(Message::Text(json))
                    .await?;
                Ok(())
            }
            Err(e) => Err(NetworkTablesError::SerdeJson(e)),
        }
    }

    pub(super) async fn send_binary(&mut self, message: Vec<u8>) -> Result<(), NetworkTablesError> {
        if let Some(output) = self.sub_ouput.as_mut() {
            output.send(Message::Binary(message)).await?;
        }
        Ok(())
    }

    pub(super) async fn flush(&mut self) -> Result<(), NetworkTablesError> {
        if let Some(output) = self.sub_ouput.as_mut() {
            output
                .send(Message::Binary(vec![b'f', b'l', b'u', b's', b'h']))
                .await?;
        }
        Ok(())
    }
}

// #[cfg(feature = "multi-thread")]
type InteriorMut<T> = tokio::sync::Mutex<T>;
// #[cfg(not(feature = "multi-thread"))]
// type InteriorMut<T> = std::cell::RefCell<T>;

///The inner server struct, this is the main struct that contains all the data for the server
/// and handles all the connections / network logic.
///
///This struct is not meant to be used directly, instead it is wrapped in a Server Handle type.
///
///The inner server is thread safe, and can be shared between threads. this is through each mutable field
/// being wrapped in a mutex.
#[derive(Debug)]
pub(super) struct InnerServer {
    ///The address of the server, port defaults to 5810, on robot ip follows 10.TE.AM.2.
    pub(super) server_addr: SocketAddr,
    ///The config for the server, contains callbacks and other things
    pub(super) config: ServerConfig,
    ///A heap of all the topics, the topics idx will be the mainw ay of accessing them.
    ///When a topic is unpublished it is set to None.
    ///When a new topic is published it can overwrite a None or be pushed to the end.
    pub(super) topics: InteriorMut<IntMap<TopicIdx, Topic>>,
    ///A map of topic name to its idx in the topics heap.
    pub(super) topic_map: InteriorMut<HashMap<String, TopicIdx>>,
    ///A count of the number of publishers for each topic.
    ///If a topics count reaches 0 it is removed from the topics heap.
    pub(super) pub_count: InteriorMut<IntMap<TopicIdx, u16>>,
    ///The latest value for each topic, used for when new clients connect.
    pub(super) topic_value_map: InteriorMut<IntMap<TopicIdx, FrcTimestampedValue>>,
    pub(super) metatopic_value_map: InteriorMut<IntMap<TopicIdx, MetaTopicValue>>,
    ///A client map, maps the client identity to the client.
    pub(super) client_map: InteriorMut<HashMap<ClientIdentity, ClientIdx>>,
    ///The clients that are connected to the server.
    pub(super) clients: InteriorMut<IntMap<ClientIdx, ConnectedClient>>,
    ///A hashmap of client_idx to metatopic_idx | `(pub, sub)`
    pub(super) client_metatopic_map: InteriorMut<IntMap<ClientIdx, (TopicIdx, TopicIdx)>>,
    ///A heap of all subscriptions, the idx of the subscription is the main way of accessing it.
    ///When a subscription is removed it is set to None.
    ///When a new subscription is added it can overwrite a None or be pushed to the end.
    pub(super) subscriptions: InteriorMut<IntMap<SubIdx, (Subscription, ClientIdx)>>,
    ///a map of every subscription to be notified when a topic value is received.
    pub(super) topic_sub_map: InteriorMut<IntMap<TopicIdx, Vec<SubIdx>>>,
    ///Sub send timestamp.
    pub(super) sub_send_timestamps: InteriorMut<IntMap<SubIdx, Instant>>,
    ///Cached sub values to send next period.
    pub(super) sub_values: InteriorMut<IntMap<SubIdx, StoredSubValue>>,
    ///A collection of incrementable counters, used for generating unique ids.
    pub(super) counters: InteriorMut<Counter>,
}

//just to make the code a bit cleaner and reduce magic numbers
const CLIENT_METATOPIC: TopicIdx = 0;
const SERVER_SUB_METADATA: TopicIdx = 1;
const SERVER_PUB_METATOPIC: TopicIdx = 2;
const SERVER_CLIENT_IDX: ClientIdx = 0;

impl InnerServer {
    /// Attempts to create a new server.
    pub(super) async fn try_new(
        server_addr: SocketAddr,
        config: ServerConfig,
    ) -> Result<Self, NetworkTablesError> {
        //Preallocate most of the datastructures, increases creation time but should increase perf later in runtime
        //numbers are completely arbitrary and can be tuned later if need be
        let slf = Self {
            server_addr,
            config,
            topics: InteriorMut::new(IntMap::with_capacity_and_hasher(128, Default::default())),
            topic_map: InteriorMut::new(HashMap::with_capacity(128)),
            pub_count: InteriorMut::new(IntMap::with_capacity_and_hasher(64, Default::default())),
            topic_value_map: InteriorMut::new(IntMap::with_capacity_and_hasher(
                64,
                Default::default(),
            )),
            metatopic_value_map: InteriorMut::new(IntMap::with_capacity_and_hasher(
                64,
                Default::default(),
            )),
            client_map: InteriorMut::new(HashMap::with_capacity(8)),
            clients: InteriorMut::new(IntMap::with_capacity_and_hasher(8, Default::default())),
            client_metatopic_map: InteriorMut::new(IntMap::with_capacity_and_hasher(
                8,
                Default::default(),
            )),
            subscriptions: InteriorMut::new(IntMap::with_capacity_and_hasher(
                64,
                Default::default(),
            )),
            topic_sub_map: InteriorMut::new(IntMap::with_capacity_and_hasher(
                64,
                Default::default(),
            )),
            sub_send_timestamps: InteriorMut::new(IntMap::with_capacity_and_hasher(
                64,
                Default::default(),
            )),
            sub_values: InteriorMut::new(IntMap::with_capacity_and_hasher(64, Default::default())),
            counters: InteriorMut::new(Default::default()),
        };

        //adds a client to act as the server client interface
        lock!(slf.clients).insert(
            lock!(slf.counters).next_client_idx(),
            ConnectedClient {
                identity: ClientIdentity::server(),
                kill_bool: Arc::new(AtomicBool::new(false)),
                sub_ouput: None,
                announced_topics: Default::default(),
                pubuid_to_topicidx: Default::default(),
                subs: Default::default(),
                subuid_to_subidx: Default::default(),
            },
        );

        //add metatopics
        slf.add_topic(Topic::new_meta("$clients".to_string()))
            .await?;
        slf.add_topic(Topic::new_meta("$serversub".to_string()))
            .await?;
        slf.add_topic(Topic::new_meta("$serverpub".to_string()))
            .await?;

        Ok(slf)
    }

    ///Checks if a client is connected to the server.
    ///
    /// ## Locks
    /// - `client_map`
    pub(super) async fn is_client_connected(&self, ident: &ClientIdentity) -> bool {
        lock!(self.client_map).contains_key(ident)
    }

    /// Disconnects a client from the server.
    ///
    /// This is done by flipping the kill switch for the
    /// tasks that are running for the client and removing the client from the client map.
    /// This function will also remove all topics that the client was publishing to if the client was the only publisher
    /// and clean up the metatopics for the client.
    ///
    /// ## Locks
    /// - `client_map`
    /// - `clients`
    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn disconnect_client(
        &self,
        ident: ClientIdentity,
    ) -> Result<(), NetworkTablesError> {
        let idx = match lock!(self.client_map).remove(&ident) {
            Some(idx) => idx,
            None => Err(NTServerError::ClientDoesNotExist(ident.to_string()))?,
        };

        match lock!(self.clients).remove(&idx) {
            Some(client) => {
                client
                    .kill_bool
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                for topic_idx in client.pubuid_to_topicidx.iter_lr().map(|(_, idx)| idx) {
                    if let Some(count) = lock!(self.pub_count).get_mut(topic_idx) {
                        *count -= 1;
                        if *count == 0 {
                            self.remove_topic(*topic_idx).await?;
                        }
                    }
                }

                let meta_idxs = lock!(self.client_metatopic_map).remove(&idx);

                if let Some(meta_idxs) = meta_idxs {
                    self.remove_topic(meta_idxs.0).await?;
                    self.remove_topic(meta_idxs.1).await?;
                }
            }
            None => {
                tracing::warn!("Client {} can't disconnect: not connected", ident);
                Err(NTServerError::ClientDoesNotExist(ident.to_string()))?
            }
        }

        Ok(())
    }

    /// Function to be called when a client requests a connection.
    ///
    /// This function will check if the client is already connected and if not
    /// will add the client to the client map and return its sub-receiver and kill-switch instance.
    ///
    /// ## Locks
    /// - `client_map`
    /// - `clients`
    /// - `counters`
    /// - `client_metatopic_map`
    /// - `metatopic_value_map`
    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn connect_client(
        &self,
        identity: &mut ClientIdentity,
    ) -> Result<(Arc<AtomicBool>, Receiver<Message>, ClientIdx), NetworkTablesError> {
        match self.is_client_connected(&identity).await {
            false => {
                tracing::info!("Client {} connected", identity);
                (self.config.on_client_connect)().await;
            }
            true => {
                tracing::warn!("Client {} already connected", identity);
                Err(NTServerError::ClientAlreadyExists(identity.to_string()))?;
            }
        }

        let mut clients = lock!(self.clients);

        //ensures no duplicate names
        let mut instances_of_ident = 0;
        for client in clients.iter() {
            if client.1.identity.name().contains(identity.name()) {
                instances_of_ident += 1;
            }
        }
        if instances_of_ident > 0 {
            tracing::info!("Client ident name already in use, changing to work");
            identity.update_name(format!("{}@{}", identity.name(), instances_of_ident));
        }

        let (outgoing_sender, outgoing_receiver) = channel::<Message>(127);
        let kill_switch = Arc::new(AtomicBool::new(false));
        let client = ConnectedClient::new(identity.clone(), kill_switch.clone(), outgoing_sender);

        let client_mte = identity.to_mte();
        let client_name = identity.name().to_string();

        //find the lowest open key in the client map and insert the client
        let mut counters = lock!(self.counters);

        let idx = counters.next_client_idx();
        clients.insert(idx, client);
        lock!(self.client_map).insert(identity.to_owned(), idx);

        //add client data to the respective $clients metatopic
        lock!(self.metatopic_value_map)
            .entry(CLIENT_METATOPIC)
            .or_insert_with(|| MetaTopicValue::Client(vec![]))
            .merge(MetaTopicValue::Client(vec![client_mte]));

        //need to free mutexes before calling add_topic
        drop(clients);
        drop(counters);

        let pub_idx = self
            .add_topic(Topic::new_meta(format!(
                "$clientpub${}",
                client_name.clone()
            )))
            .await?;
        let sub_idx = self
            .add_topic(Topic::new_meta(format!(
                "$clientsub${}",
                client_name.clone()
            )))
            .await?;

        lock!(self.client_metatopic_map).insert(idx, (pub_idx, sub_idx));

        Ok((kill_switch, outgoing_receiver, idx))
    }

    ///Returns the client identity of a client given its idx.
    pub(super) async fn get_client_ident(&self, client_idx: ClientIdx) -> Option<ClientIdentity> {
        lock!(self.clients)
            .get(&client_idx)
            .map(|client| client.identity.clone())
    }

    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn inner_publish_topic(
        &self,
        pub_topic: PublishTopic,
        client_idx: ClientIdx,
    ) -> Result<TopicIdx, NetworkTablesError> {
        let pubuid = pub_topic.pubuid;

        //get mte before topic is consumed
        let pub_mte = ClientPublisherMTE {
            topic: pub_topic.name.clone(),
            uid: pubuid,
        };

        let topic_idx = self.add_topic(Topic::from_publish_topic(pub_topic)).await?;

        self.announce_topic(topic_idx, Some((pubuid, client_idx)))
            .await?;

        //add the pubuid to the client's pubuid_to_topicidx map
        lock!(self.clients)
            .get_mut(&client_idx)
            .ok_or(NTServerError::ClientDoesNotExist(format!(
                "Client id: {}",
                client_idx
            )))?
            .pubuid_to_topicidx
            .insert(pubuid, topic_idx);

        //update metatopics
        if client_idx == SERVER_CLIENT_IDX {
            //server client
            lock!(self.metatopic_value_map)
                .entry(SERVER_PUB_METATOPIC)
                .or_insert_with(|| MetaTopicValue::ServerPublisher(vec![]))
                .merge(MetaTopicValue::ServerPublisher(vec![pub_mte]));
        } else {
            //external client
            if let Some((pub_idx, _)) = lock!(self.client_metatopic_map).get(&client_idx) {
                lock!(self.metatopic_value_map)
                    .entry(*pub_idx)
                    .or_insert_with(|| MetaTopicValue::ClientPublisher(vec![]))
                    .merge(MetaTopicValue::ClientPublisher(vec![pub_mte]));
            } else {
                Err(NTServerError::TopicDoesNotExist(format!(
                    "Client id {} metatopic did not exist",
                    client_idx
                )))?;
            }
        }

        Ok(topic_idx)
    }

    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn inner_unpublish_topic(
        &self,
        pub_topic: UnpublishTopic,
        client_idx: ClientIdx,
    ) -> Result<(), NetworkTablesError> {
        let mut clients = lock!(self.clients);

        let client = clients
            .get_mut(&client_idx)
            .ok_or(NTServerError::ClientDoesNotExist(format!(
                "Client id: {}",
                client_idx
            )))?;

        let topic_idx: TopicIdx = client
            .pubuid_to_topicidx
            .remove_by_left(&pub_topic.pubuid)
            .ok_or(NTServerError::TopicDoesNotExist(format!(
                "pubuid {} for client {} related topic did not exist",
                pub_topic.pubuid, client_idx
            )))?;

        let pub_mte = ClientPublisherMTE {
            topic: lock!(self.topics)
                .get(&topic_idx)
                .cloned()
                .map(|topic| topic.name)
                .unwrap_or_default(),
            uid: pub_topic.pubuid,
        };

        drop(clients);

        let mut pub_count = lock!(self.pub_count);

        //reduces the pub count for the topic, if reaches 0 remove the topic
        if let Some(count) = pub_count.get_mut(&topic_idx) {
            *count -= 1;
            if *count == 0 {
                self.remove_topic(topic_idx).await?;
                pub_count.remove(&topic_idx);
            }
        } else {
            return Err(NTServerError::TopicDoesNotExist(format!(
                "Topic idx {} did not exist in pub_count",
                topic_idx
            )))?;
        }

        //update metatopics
        if client_idx == SERVER_CLIENT_IDX {
            //server client
            lock!(self.metatopic_value_map)
                .entry(SERVER_PUB_METATOPIC)
                .or_insert_with(|| MetaTopicValue::ServerPublisher(vec![]))
                .reduce(MetaTopicValue::ServerPublisher(vec![pub_mte]));
        } else {
            //external client
            if let Some((pub_idx, _)) = lock!(self.client_metatopic_map).get(&client_idx) {
                lock!(self.metatopic_value_map)
                    .entry(*pub_idx)
                    .or_insert_with(|| MetaTopicValue::ClientPublisher(vec![]))
                    .reduce(MetaTopicValue::ClientPublisher(vec![pub_mte]));
            } else {
                Err(NTServerError::TopicDoesNotExist(format!(
                    "Client id {} metatopic did not exist",
                    client_idx
                )))?;
            }
        }

        Ok(())
    }

    /// Adds a subscription to a client on the server
    ///
    /// ## Locks
    /// - `clients`
    /// - `topic_map`
    /// - `topic_sub_map`
    /// - `topic_value_map`
    /// - `subscriptions`
    /// - `sub_send_timestamps`
    /// - `sub_values`
    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn subscribe(
        &self,
        sub: Subscribe,
        client_idx: ClientIdx,
    ) -> Result<(), NetworkTablesError> {
        let mut clients = lock!(self.clients);

        let client = clients
            .get_mut(&client_idx)
            .ok_or(NTServerError::ClientDoesNotExist(format!(
                "Client id: {}",
                client_idx
            )))?;

        let subscription = Subscription {
            topics: sub.topics,
            subuid: sub.subuid,
            options: sub.options,
        };

        let topic_map = lock!(self.topic_map);

        let mut interested_topics: Vec<(TopicIdx, &str)> = Vec::new();

        for topic_entry in topic_map.iter() {
            if subscription.cares_about(topic_entry.0) {
                interested_topics.push((*topic_entry.1, topic_entry.0));
            }
        }

        let mut topic_sub_map = lock!(self.topic_sub_map);
        let topic_values = lock!(self.topic_value_map);
        let sub_idx: SubIdx = lock!(self.counters).next_sub_idx();

        let mut announce_msgs = Vec::with_capacity(interested_topics.len());
        let mut send_values = Vec::new();

        let topics = lock!(self.topics);

        for topic in interested_topics {
            let topic_idx = topic.0;
            let topic_name = topic.1;
            if let Some(vectr) = topic_sub_map.get_mut(&topic_idx) {
                vectr.push(sub_idx);
            } else {
                tracing::error!("topic_sub_map missing topic_idx: {}", topic_idx);
                Err(NTServerError::TopicDoesNotExist(format!(
                    "Topic id: {}",
                    topic_idx
                )))?;
            }
            let r#type = if topic_name.contains("$") {
                FrcType::Raw
            } else {
                match topic_values.get(&topic_idx) {
                    Some(timstamped_value) => timstamped_value.value.get_type(),
                    None => {
                        if let Some(topic) = topics.get(&topic_idx) {
                            topic.r#type
                        } else {
                            tracing::error!("topic_value_map missing topic_idx: {}", topic_idx);
                            continue;
                        }
                    }
                }
            };

            let props = topics.get(&topic_idx).map_or_else(
                || {
                    tracing::error!("missing topic_idx: {}", topic_idx);
                    PublishProperties::default()
                },
                |topic| topic.properties.unwrap_or_default().clone(),
            );

            let announce_msg = Announce {
                name: topic_name.to_owned(),
                id: topic_idx as i32,
                pubuid: None,
                r#type: r#type.clone(),
                properties: props,
            };
            announce_msgs.push(NTMessage::Announce(announce_msg));

            if let Some(value) = topic_values.get(&topic_idx) {
                match value.as_bytes(topic_idx as i32) {
                    Ok(buf) => send_values.extend(buf),
                    Err(e) => {
                        tracing::error!("Error getting binary value: {}", e);
                    }
                }
            } else {
                tracing::debug!("{} has yet to have a value posted for it", topic_idx);
            }
        }

        lock!(self.sub_send_timestamps).insert(sub_idx, Instant::now());
        if subscription
            .options
            .as_ref()
            .unwrap_or(&SubscriptionOptions::default())
            .all
            .unwrap_or(false)
        {
            lock!(self.sub_values).insert(sub_idx, StoredSubValue::new_all());
        } else {
            lock!(self.sub_values).insert(sub_idx, StoredSubValue::new_one());
        }

        lock!(self.subscriptions).insert(sub_idx, (subscription, client_idx));

        client.subuid_to_subidx.insert(sub.subuid as u32, sub_idx);

        client.send_nt_messages(&announce_msgs).await?;

        client.send_binary(send_values).await?;

        Ok(())
    }

    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn unsubscribe(
        &self,
        unsub: Unsubscribe,
        client_idx: ClientIdx,
    ) -> Result<(), NTServerError> {
        let mut clients = lock!(self.clients);

        let client = match clients.get_mut(&client_idx) {
            Some(client) => client,
            None => {
                return Err(NTServerError::ClientDoesNotExist(format!(
                    "Client id: {}",
                    client_idx
                )))
            }
        };

        let subidx = match client
            .subuid_to_subidx
            .remove_by_left(&(unsub.subuid as u32))
        {
            Some(sub_idx) => sub_idx,
            _ => {
                return Err(NTServerError::SubscriptionDoesNotExist(format!(
                    "Subuid: {}",
                    unsub.subuid
                )))
            }
        };

        let mut sub_map = lock!(self.topic_sub_map);
        for vectr in sub_map.values_mut() {
            vectr.retain_mut(|elem| *elem != subidx);
        }

        lock!(self.subscriptions).remove(&subidx);
        lock!(self.sub_values).remove(&subidx);
        lock!(self.sub_send_timestamps).remove(&subidx);

        Ok(())
    }

    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn set_properties(
        &self,
        set_properties: SetProperties,
    ) -> Result<(), NetworkTablesError> {
        let topic_idx = match lock!(self.topic_map).get(&set_properties.name) {
            Some(topic_idx) => *topic_idx,
            None => return Err(NTServerError::TopicDoesNotExist(set_properties.name))?,
        };

        match lock!(self.topics).get_mut(&topic_idx) {
            Some(topic) => {
                if topic.properties.is_none() {
                    topic.properties = Some(set_properties.update);
                } else {
                    let properties = topic.properties.as_mut().unwrap();
                    if let Some(ret) = set_properties.update.retained {
                        properties.retained = Some(ret);
                    }
                    if let Some(persistent) = set_properties.update.persistent {
                        properties.persistent = Some(persistent);
                    }
                }
            }
            None => return Err(NTServerError::TopicDoesNotExist(set_properties.name))?,
        }

        Ok(())
    }

    #[async_recursion::async_recursion]
    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn add_topic(&self, topic: Topic) -> Result<TopicIdx, NetworkTablesError> {
        if let Some(topic_idx) = lock!(self.topic_map).get(&topic.name) {
            return Ok(*topic_idx);
        }

        let mut topics = lock!(self.topics);

        let topic_idx = lock!(self.counters).next_topic_idx();
        let topic_name = topic.name.clone();

        let count = if topic.has_implicit_publisher() { 2 } else { 1 };

        topics.insert(topic_idx, topic);

        drop(topics);

        lock!(self.pub_count).insert(topic_idx, count);

        lock!(self.topic_map).insert(topic_name.clone(), topic_idx);

        lock!(self.topic_sub_map).insert(topic_idx, Vec::new());

        tracing::debug!("Added topic: {}", topic_name);

        //metatopics don't make it past this point
        if topic_name.contains("$") {
            return Ok(topic_idx);
        }

        let meta_pub = Topic::new_meta(format!("$pub${}", topic_name.clone()));
        let meta_sub = Topic::new_meta(format!("$sub${}", topic_name));

        self.announce_topic(self.add_topic(meta_pub).await?, None)
            .await?;

        self.announce_topic(self.add_topic(meta_sub).await?, None)
            .await?;

        Ok(topic_idx)
    }

    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn remove_topic(&self, topic_idx: TopicIdx) -> Result<(), NetworkTablesError> {
        let mut topics = lock!(self.topics);

        let topic = topics
            .remove(&topic_idx)
            .ok_or(NTServerError::TopicDoesNotExist(format!(
                "topic idx {} could not be removed [did not exist]",
                topic_idx
            )))?;

        let meta_pub_name = format!("$pub${}", topic.name);
        let meta_sub_name = format!("$sub${}", topic.name);

        let mut topic_map = lock!(self.topic_map);

        let meta_pub_idx =
            topic_map
                .remove(&meta_pub_name)
                .ok_or(NTServerError::TopicDoesNotExist(format!(
                    "metatopic idx {} could not be removed [did not exist]",
                    topic_idx
                )))?;

        let meta_sub_idx =
            topic_map
                .remove(&meta_sub_name)
                .ok_or(NTServerError::TopicDoesNotExist(format!(
                    "metatopic idx {} could not be removed [did not exist]",
                    topic_idx
                )))?;

        topic_map.remove(&topic.name);

        topics.remove(&meta_pub_idx);
        topics.remove(&meta_sub_idx);

        drop(topics);
        drop(topic_map);

        //cleanse topic sub map
        let mut topic_sub_map = lock!(self.topic_sub_map);
        topic_sub_map.remove(&topic_idx);
        topic_sub_map.remove(&meta_pub_idx);
        topic_sub_map.remove(&meta_sub_idx);

        //cleanse topic value map
        let mut topic_value_map = lock!(self.topic_value_map);
        let mut metatopic_value_map = lock!(self.metatopic_value_map);
        topic_value_map.remove(&topic_idx);
        metatopic_value_map.remove(&meta_pub_idx);
        metatopic_value_map.remove(&meta_sub_idx);

        Ok(())
    }

    /// Used when a topic is published.
    /// If `response_opt` is Some, then it will respond the the supplied client idx with pubuid field populated
    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn announce_topic(
        &self,
        topic_idx: TopicIdx,
        response_opt: Option<(PubUID, ClientIdx)>,
    ) -> Result<NtSuccess, NetworkTablesError> {
        let topics = lock!(self.topics);
        let topic = topics
            .get(&topic_idx)
            .ok_or(NTServerError::TopicDoesNotExist(format!(
                "topic idx {} could not be announced [did not exist]",
                topic_idx
            )))?;

        let announce_msg = Announce {
            name: topic.name.to_owned(),
            id: topic_idx as i32,
            pubuid: None,
            r#type: topic.r#type.clone(),
            properties: topic.properties.clone().unwrap_or_default(),
        };

        if response_opt.is_some() {
            let (pubuid, client_idx) = response_opt.unwrap();
            let mut new_msg = announce_msg.clone();
            new_msg.pubuid = Some(pubuid as i32);

            match lock!(self.clients).get_mut(&client_idx) {
                Some(client) => {
                    if !client.announced_topics.contains(&topic_idx) {
                        client
                            .send_nt_messages(&vec![NTMessage::Announce(new_msg)])
                            .await?;
                        client.announced_topics.insert(topic_idx);
                    }
                }
                None => Err(NTServerError::ClientDoesNotExist(format!(
                    "client idx {} could not be announced to [did not exist]",
                    client_idx
                )))?,
            }
        }

        let announce_msgs = vec![NTMessage::Announce(announce_msg)];

        let topic_name = topic.name.clone();

        drop(topics);

        let mut clients_to_notify = IntSet::default();

        let mut sub_map = lock!(self.subscriptions);
        for sub in sub_map.iter_mut() {
            let (subscription, client_idx) = sub.1;

            if !subscription.cares_about(&topic_name) {
                continue;
            }

            clients_to_notify.insert(*client_idx);
        }

        drop(sub_map);

        let mut clients = lock!(self.clients);
        for client_idx in clients_to_notify.into_iter() {
            let client = match clients.get_mut(&client_idx) {
                Some(client) => client,
                None => continue,
            };

            client.announced_topics.insert(topic_idx);

            client.send_nt_messages(&announce_msgs).await?;
        }

        Ok(NtSuccess)
    }

    ///Best used for a new subscription
    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn announce_all_topics_for(
        &self,
        sub_idx: SubIdx,
    ) -> Result<NtSuccess, NetworkTablesError> {
        let subs = lock!(self.subscriptions);

        let (subscription, client_idx) = match subs.get(&sub_idx) {
            Some(subscrip) => subscrip,
            None => Err(NTServerError::SubscriptionDoesNotExist(format!(
                "subscription idx {} could not be announced to [did not exist]",
                sub_idx
            )))?,
        };

        let mut announce_msgs = vec![];
        let mut announce_idxs = vec![];

        let topic_name_map = lock!(self.topic_map);
        let topics = lock!(self.topics);

        for (topic_name, topic_idx) in topic_name_map.iter() {
            if !subscription.cares_about(topic_name) {
                continue;
            }

            let topic = match topics.get(topic_idx) {
                Some(topic) => topic,
                None => continue,
            };

            let announce_msg = Announce {
                name: topic_name.to_owned(),
                id: *topic_idx as i32,
                pubuid: None,
                r#type: topic.r#type.clone(),
                properties: topic.properties.clone().unwrap_or_default(),
            };

            announce_msgs.push(NTMessage::Announce(announce_msg));
            announce_idxs.push(*topic_idx);
        }

        drop(topic_name_map);
        drop(topics);

        let mut clients = lock!(self.clients);

        let client = match clients.get_mut(client_idx) {
            Some(client) => client,
            None => {
                return Err(NTServerError::ClientDoesNotExist(format!(
                    "client idx {} could not be announced to [did not exist]",
                    client_idx
                ))
                .into())
            }
        };

        client.announced_topics.extend(announce_idxs);

        client.send_nt_messages(&announce_msgs).await?;

        Ok(NtSuccess)
    }

    #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn un_announce_topics(
        &self,
        topic_name: String,
        topic_idx: TopicIdx,
    ) -> Result<NtSuccess, NetworkTablesError> {
        let unannounce = UnAnnounce {
            name: topic_name,
            id: topic_idx,
        };

        let unannounce_msgs = vec![NTMessage::UnAnnounce(unannounce)];

        let mut clients = lock!(self.clients);

        for client in clients.iter_mut() {
            let client = client.1;

            if !client.announced_topics.remove(&topic_idx) {
                continue;
            }

            client.send_nt_messages(&unannounce_msgs).await?;
        }

        Ok(NtSuccess)
    }

    // #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn set_value(
        &self,
        value: FrcTimestampedValue,
        topic_idx: TopicIdx,
    ) -> Result<NtSuccess, NetworkTablesError> {
        //make sure the topic exists and is not a meta topic
        if let Some(topic) = lock!(self.topics).get(&topic_idx) {
            if topic.name.contains("$") {
                Err(NTServerError::NotNormalTopic(topic.name.clone()))?;
            }
        } else {
            Err(NTServerError::TopicDoesNotExist(format!(
                "topic idx {} could not be set [did not exist]",
                topic_idx
            )))?;
        }

        let mut sub_values = lock!(self.sub_values);
        for sub_idx in lock!(self.topic_sub_map).get(&topic_idx).unwrap() {
            sub_values
                .get_mut(sub_idx)
                .unwrap()
                .add_value(topic_idx, ServerSendableValue::new_normal(value.clone()))
        }
        drop(sub_values);
        let mut topic_value_map = lock!(self.topic_value_map);
        if let Some(old_value) = topic_value_map.get_mut(&topic_idx) {
            if old_value.timestamp < value.timestamp {
                old_value.replace_inplace(value);
            }
        } else {
            topic_value_map.insert(topic_idx, value);
        };

        Ok(NtSuccess)
    }

    // #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn get_topic_idx(
        &self,
        topic_name: &String,
    ) -> Result<TopicIdx, NetworkTablesError> {
        lock!(self.topic_map).get(topic_name).cloned().ok_or(Err(
            NTServerError::TopicDoesNotExist(format!(
                "topic name {} could not be found [did not exist]",
                topic_name
            )),
        )?)
    }

    // #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn get_value(
        &self,
        topic_idx: TopicIdx,
    ) -> Result<FrcTimestampedValue, NetworkTablesError> {
        lock!(self.topic_value_map).get(&topic_idx).cloned().ok_or(
            NTServerError::TopicDoesNotExist(format!(
                "topic idx {} could not be found [did not exist]",
                topic_idx
            ))
            .into(),
        )
    }

    // #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn flush_all(&self) -> Result<(), NetworkTablesError> {
        for (_, client) in lock!(self.clients).iter_mut() {
            client.flush().await?;
        }
        Ok(())
    }

    // #[cfg_attr(not(frc_comp), tracing::instrument(level = "debug", skip_all))]
    pub(super) async fn flush_client(
        &self,
        client_idx: ClientIdx,
    ) -> Result<(), NetworkTablesError> {
        let mut clients = lock!(self.clients);
        let client = clients
            .get_mut(&client_idx)
            .ok_or(NTServerError::ClientDoesNotExist(format!(
                "Client id: {}",
                client_idx
            )))?;

        client.flush().await?;

        Ok(())
    }
}

impl Drop for InnerServer {
    fn drop(&mut self) {
        tracing::debug!("Stopping NT4 Server");
    }
}

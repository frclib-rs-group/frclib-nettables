use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    ops::Div,
    sync::{Arc, Weak},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{log_result, spec::extensions::FrcTypeExt};

use super::{config::ClientConfig, util::UnsignedIntOrNegativeOne};
use crate::spec::{
    messages::Announce, messages::NTMessage, messages::PublishTopic, messages::Subscribe,
    subscription::InternalSub, subscription::MessageData, topic::PublishedTopic, topic::Topic,
};
use crate::WebSocket;
use frclib_core::value::FrcType;
use futures_util::{SinkExt, TryStreamExt};
use tokio::{
    select,
    sync::{mpsc, oneshot, Mutex},
    task::yield_now,
};

use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message, protocol::CloseFrame};

#[derive(Debug)]
pub(super) struct InnerClient {
    pub(super) server_addr: SocketAddr,
    // Keys are subuid, value is a handle to sub data and a sender to the sub's mpsc
    pub(super) subscriptions: Mutex<HashMap<i32, InternalSub>>,
    pub(super) announced_topics: Mutex<HashMap<i32, Topic>>,
    pub(super) client_published_topics: Mutex<HashMap<u32, PublishedTopic>>,
    pub(super) socket_sender: mpsc::Sender<Message>,
    pub(super) socket_panic_receiver: Mutex<oneshot::Receiver<crate::NetworkTablesError>>,
    pub(super) server_time_offset: parking_lot::Mutex<u64>,
    pub(super) sub_counter: parking_lot::Mutex<i32>,
    pub(super) topic_counter: parking_lot::Mutex<u32>,
    pub(super) config: ClientConfig,
    // Has to be mutable to prevent overflow if it becomes too long ago
    pub(super) start_instant: parking_lot::Mutex<Instant>,
    pub(super) start_time: parking_lot::Mutex<SystemTime>,
    pub(super) identity: String,
}

impl InnerClient {
    /// Returns err if the socket task has ended
    async fn check_task_panic(&self) -> Result<(), crate::NetworkTablesError> {
        match self.socket_panic_receiver.lock().await.try_recv() {
            Ok(err) => Err(err),
            Err(_) => Ok(()),
        }
    }

    /// Sends message to websocket task, which handles reconnection if necessary
    pub(crate) async fn send_message(
        &self,
        message: Message,
    ) -> Result<(), crate::NetworkTablesError> {
        self.check_task_panic().await?;
        tracing::trace!("Sending message: {message:?}");

        // Should never be dropped before a send goes off
        self.socket_sender.send(message).await.unwrap();
        Ok(())
    }

    #[inline]
    pub(crate) fn client_time(&self) -> u64 {
        Instant::now()
            .duration_since(*self.start_instant.lock())
            .as_micros() as u64
    }

    pub(crate) fn server_time(&self) -> u64 {
        self.client_time() + *self.server_time_offset.lock()
    }

    pub(crate) fn real_world_server_time(&self) -> u64 {
        self.start_time
            .lock()
            .to_owned()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
            + self.server_time()
    }

    pub(crate) fn to_real_time(&self, server_time: u64) -> u64 {
        self.start_time
            .lock()
            .to_owned()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
            + server_time
    }

    /// Takes new timestamp value and updates this client's offset
    /// Returns `None` if the math failed
    pub(crate) fn handle_new_timestamp(
        &self,
        server_timestamp: u64,
        client_timestamp: Option<i64>,
    ) -> Option<()> {
        if let Some(client_timestamp) = client_timestamp {
            let receive_time = self.client_time();
            let round_trip_time = receive_time.checked_sub(client_timestamp as u64)?;
            let server_time_at_receive = server_timestamp.checked_sub(round_trip_time.div(2))?;

            // Checked sub because if start_time was too long ago, it will overflow and panic
            let offset = server_time_at_receive.checked_sub(receive_time)?;
            *self.server_time_offset.lock() = offset;
        }

        Some(())
    }

    pub(crate) fn new_topic_id(&self) -> u32 {
        let mut current_id = self.topic_counter.lock();
        let new_id = current_id.checked_add(1).unwrap_or(1);
        *current_id = new_id;
        new_id
    }

    pub(crate) fn new_sub_id(&self) -> i32 {
        let mut current_id = self.sub_counter.lock();
        let new_id = current_id.checked_add(1).unwrap_or(1);
        *current_id = new_id;
        new_id
    }

    pub(crate) async fn publish_value_w_timestamp(
        &self,
        id: UnsignedIntOrNegativeOne,
        r#type: FrcType,
        timestamp: u32,
        value: &rmpv::Value,
    ) -> Result<(), crate::NetworkTablesError> {
        self.check_task_panic().await?;
        let mut buf = Vec::<u8>::with_capacity(19);

        rmp::encode::write_array_len(&mut buf, 4)?;
        // Client side topic is guaranteed to have a uid
        id.write_to_buf(&mut buf).unwrap();
        rmp::encode::write_uint(&mut buf, timestamp as u64)?;
        rmp::encode::write_u8(&mut buf, r#type.as_u8())?;
        rmpv::encode::write_value(&mut buf, value)?;

        tracing::debug!("Sending Value: {value:?}");

        Ok(self.send_message(Message::Binary(buf)).await?)
    }

    /// Value should match topic type
    pub(crate) async fn publish_value(
        &self,
        id: UnsignedIntOrNegativeOne,
        r#type: FrcType,
        value: &rmpv::Value,
    ) -> Result<(), crate::NetworkTablesError> {
        self.publish_value_w_timestamp(id, r#type, self.server_time() as u32, value)
            .await
    }

    fn reset_time(&self) {
        *self.server_time_offset.lock() = 0;
        *self.start_instant.lock() = Instant::now();
    }

    pub(crate) async fn update_time(&self) -> Result<(), crate::NetworkTablesError> {
        let announced_topics = self.announced_topics.lock().await;
        let time_topic = announced_topics.get(&-1);

        if let Some(time_topic) = time_topic {
            tracing::trace!("Updating timestamp.");

            return self
                .publish_value_w_timestamp(
                    UnsignedIntOrNegativeOne::NegativeOne,
                    time_topic.r#type,
                    0,
                    &rmpv::Value::Integer(self.client_time().into()),
                )
                .await;
        }

        Ok(())
    }

    // Called on connection open, must not fail!
    pub(crate) async fn on_open(&self) {
        let mut announced = self.announced_topics.lock().await;
        let client_published = self.client_published_topics.lock().await;
        let mut subscriptions = self.subscriptions.lock().await;
        announced.clear();
        announced.insert(
            -1,
            Topic {
                name: "Time".into(),
                r#type: FrcType::Int,
                properties: None,
            },
        );

        // One allocation
        let mut messages: Vec<NTMessage> =
            Vec::with_capacity(client_published.len() + subscriptions.len());

        // Add publish messages
        for topic in client_published.values() {
            messages.push(NTMessage::Publish(PublishTopic {
                name: topic.name.clone(),
                properties: topic.properties,
                // Client published is guaranteed to have a uid
                pubuid: topic.pubuid,
                r#type: topic.r#type,
            }));
        }

        // Remove invalid subs (user has dropped them)
        subscriptions.retain(|_, sub| sub.is_valid());

        // Add subscribe messages
        messages.extend(subscriptions.values().filter_map(|sub| {
            if let Some(data) = sub.data.upgrade() {
                return Some(NTMessage::Subscribe(Subscribe {
                    subuid: data.subuid,
                    // Somehow get rid of cloning here?
                    topics: data.topics.clone(),
                    options: data.options.clone(),
                }));
            }
            None
        }));

        // Reset our time stuff & send all messages at once (please don't fail 🥺)
        self.reset_time();
        drop(announced);
        self.update_time().await.ok();
        self.send_message(Message::Text(serde_json::to_string(&messages).unwrap()))
            .await
            .ok();

        tracing::info!("Prepared new connection.");
    }
}

/// Handles messages from the server
pub(super) async fn handle_message(client: Arc<InnerClient>, message: Message) {
    match message {
        Message::Text(message) => {
            // Either announce, unannounce, or properties
            let messages: Vec<NTMessage> = match log_result(
                serde_json::from_str(&message).map_err(Into::<crate::NetworkTablesError>::into),
            ) {
                Ok(messages) => messages,
                Err(_) => {
                    tracing::error!("Server sent an invalid message: {message:?}");
                    return;
                }
            };

            for message in messages {
                match message {
                    NTMessage::Announce(Announce {
                        name,
                        id,
                        pubuid: _,
                        properties,
                        r#type,
                    }) => {
                        let mut announced = client.announced_topics.lock().await;

                        tracing::debug!("Server announced: {name}");

                        if announced.contains_key(&id) {
                            tracing::debug!("Received announcement for already announced topic")
                        } else {
                            announced.insert(
                                id,
                                Topic {
                                    name: name.to_owned(),
                                    properties: Some(properties),
                                    r#type,
                                },
                            );
                        }

                        // Call user provided on announce fn
                        (client.config.on_announce)(announced.get(&id).unwrap()).await;
                    }
                    NTMessage::UnAnnounce(un_announce) => {
                        tracing::debug!("Server un_announced: {}", un_announce.name);

                        let removed = client
                            .announced_topics
                            .lock()
                            .await
                            .remove(&(un_announce.id as i32));
                        (client.config.on_unannounce)(removed).await;
                    }
                    NTMessage::Properties(_) => {
                        tracing::debug!("Server sent properties.");
                    }
                    _ => {
                        tracing::error!("Server sent an invalid message: {message:?}");
                    }
                }
            }
        }
        Message::Binary(msgpack) => {
            // Message pack value, update

            // Put the raw data in a VecDeque because its read impl removes bytes from it
            // so we keep deserializing msgpack from the VecDeque until it is emptied out
            let mut msgpack = VecDeque::from(msgpack);
            while let Ok(data) = rmp_serde::decode::from_read(&mut msgpack) {
                match data {
                    rmpv::Value::Array(array) => handle_value(array, Arc::clone(&client)).await,
                    _ => {
                        tracing::error!("Server sent an invalid msgpack data, not an array.");
                    }
                }
            }
        }
        _ => {}
    }
}

pub(super) async fn handle_value(array: Vec<rmpv::Value>, client: Arc<InnerClient>) {
    if array.len() != 4 {
        tracing::error!("Server sent an invalid msgpack data, wrong length.");
        return;
    }

    let id = array[0].as_i64().map(|n| n as i32);
    let timestamp_micros = array[1].as_u64();
    let type_idx = array[2].as_u64();
    let data = &array[3];

    if let Some(id) = id {
        if let Some(timestamp_micros) = timestamp_micros {
            if id >= 0 {
                if let Some(type_idx) = type_idx {
                    let r#type = FrcType::from_u8(type_idx as u8);
                    if let Some(r#type) = r#type {
                        if let Some(topic) = client.announced_topics.lock().await.get(&id) {
                            tracing::trace!("Received Value: {topic:?} {type:?} {data:?}");
                            send_value_to_subscriber(
                                client.clone(),
                                topic,
                                timestamp_micros as u32,
                                r#type,
                                data,
                            )
                            .await;
                        } else {
                            tracing::error!("Received a topic before it was announced!");
                        }
                    } else {
                        tracing::error!("Server sent an invalid type id");
                    }
                }
            } else if id == -1 {
                // Timestamp update
                match client.handle_new_timestamp(timestamp_micros, data.as_i64()) {
                    Some(_) => {}
                    None => {
                        // Math failed, update most recent time
                        *client.start_instant.lock() = Instant::now();
                        client.update_time().await.ok();
                        client.handle_new_timestamp(timestamp_micros, data.as_i64());
                    }
                };
            } else {
                tracing::error!("Server sent an invalid topic id, less than -1");
            };

            return;
        }

        return;
    }
}

pub(super) async fn send_value_to_subscriber(
    client: Arc<InnerClient>,
    topic: &Topic,
    timestamp_micros: u32,
    r#type: FrcType,
    data: &rmpv::Value,
) {
    // Allows sent values to be handled by subs, cause there hasnt been an await for a while
    yield_now().await;

    client.subscriptions.lock().await.retain(|_, sub| {
        if !sub.is_valid() {
            println!("invalid sub");
            false
        } else {
            if sub.matches_topic(topic) {
                sub.sender
                    .try_send(MessageData {
                        topic_name: topic.name.clone(),
                        timestamp: timestamp_micros,
                        r#type: r#type.clone(),
                        data: data.to_owned(),
                    })
                    .is_ok()
            } else {
                true
            }
        }
    });
}

pub(super) async fn setup_socket(
    weak_client: Weak<InnerClient>,
    mut receiver: mpsc::Receiver<Message>,
    panic_sender: oneshot::Sender<crate::NetworkTablesError>,
) -> Result<(), crate::NetworkTablesError> {
    let client = weak_client.upgrade().expect("Client dropped before socket setup");

    let mut request = format!(
        "ws://{}/nt/{}",
        client.server_addr,
        client.identity
    )
    .into_client_request()?;

    // Add sub-protocol header
    request.headers_mut().append(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_static("networktables.first.wpi.edu"),
    );
    let uri = request.uri().clone();

    tracing::debug!("Connecting to {}", uri);

    let (mut socket, _) = tokio::time::timeout(
        Duration::from_millis(client.config.connect_timeout),
        tokio_tungstenite::connect_async(request),
    )
    .await??;

    tracing::info!("Connected to {}", uri);

    tokio::spawn(async move {
        loop {
            let maybe_err: Result<(), crate::NetworkTablesError> = select! {
                message = socket.try_next() => {
                    // Message from server
                    tracing::trace!("Received Message: {:?}", message);
                    match message {
                        Ok(Some(message)) => {
                            handle_message(upgrade_brk!(weak_client), message).await;
                            Ok(())
                        },
                        Ok(None) => {
                            // If this happens we likely just need to reconnect
                            handle_disconnect(Err::<(), _>(tokio_tungstenite::tungstenite::Error::AlreadyClosed), upgrade_brk!(weak_client), &mut socket).await.map_err(Into::into)
                        },
                        Err(err) => handle_disconnect(Err::<(), _>(err), upgrade_brk!(weak_client), &mut socket).await.map_err(Into::into),
                    }
                },
                message = receiver.recv() => {
                    // Message from client
                    if let Some(message) = message {
                        handle_disconnect(
                            socket.send(message).await,
                            upgrade_brk!(weak_client),
                            &mut socket
                        ).await.map_err(Into::into)
                    } else {
                        // should typically not be hit due to the arc upgrade checks
                        break;
                    }
                },
            };

            if let Err(err) = maybe_err {
                tracing::error!("Socket handle task ended on because: {err:?}");
                panic_sender.send(err).ok();
                break;
            }
        }
        socket.close(Some(CloseFrame {
            code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
            reason: std::borrow::Cow::Borrowed("Client dropped"),
        })).await.ok();
        tracing::info!("Client dropped, ending socket handle task.");
    });

    Ok(())
}

pub(super) async fn handle_disconnect<T>(
    result: Result<T, tokio_tungstenite::tungstenite::Error>,
    client: Arc<InnerClient>,
    socket: &mut WebSocket,
) -> Result<(), tokio_tungstenite::tungstenite::Error> {
    // Reuse for dif branches
    let reconnect_client = client.clone();

    let reconnect = move || async move {
        tracing::info!("Disconnected from server, attempting to reconnect.");
        (reconnect_client.config.on_disconnect)().await;

        loop {
            tokio::time::sleep(Duration::from_millis(
                reconnect_client.config.disconnect_retry_interval,
            ))
            .await;

            let mut request = format!(
                "ws://{}/nt/{}",
                reconnect_client.server_addr, reconnect_client.identity
            )
            .into_client_request()
            .unwrap();
            // Add sub-protocol header
            request.headers_mut().append(
                "Sec-WebSocket-Protocol",
                HeaderValue::from_static("networktables.first.wpi.edu"),
            );

            match tokio::time::timeout(
                Duration::from_millis(reconnect_client.config.connect_timeout),
                tokio_tungstenite::connect_async(request),
            )
            .await
            {
                Ok(connect_result) => match connect_result {
                    Ok((new_socket, _)) => {
                        *socket = new_socket;
                        reconnect_client.on_open().await;
                        (reconnect_client.config.on_reconnect)().await;

                        tracing::info!("Successfully reestablished connection.");

                        break Ok(());
                    }
                    Err(_) => {}
                },
                Err(_) => {}
            }
        }
    };

    match result {
        Ok(_) => Ok(()),
        Err(err) => {
            if (client.config.should_reconnect)(&err) {
                reconnect().await
            } else {
                tracing::error!("Handle socket dying on {err:?}");
                Err(err)
            }
        }
    }
}

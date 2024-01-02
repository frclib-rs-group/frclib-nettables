#![allow(clippy::too_many_lines)]

use std::{
    collections::VecDeque,
    convert::Infallible,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Weak,
    },
};

use frclib_core::value::{FrcTimestampedValue, FrcType, FrcValue};
use futures_util::{pin_mut, SinkExt, StreamExt, TryStreamExt};
use hyper::{
    header::{
        CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_PROTOCOL, UPGRADE,
    },
    http::HeaderValue,
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    upgrade::Upgraded,
    Body, Method, Request as HRequest, Response as HResponse, Server as HServer, StatusCode,
    Version,
};
use tokio::{
    select,
    sync::mpsc::{channel, Receiver, Sender},
    time::Instant,
};
use tokio_tungstenite::{
    tungstenite::{handshake::derive_accept_key, Message},
    WebSocketStream,
};

use crate::{
    now,
    server::util::ClientIdentity,
    spec::{extensions::FrcTypeExt, messages::NTMessage},
};

use super::{
    inner::InnerServer,
    util::{ClientIdx, ClientMsg},
};

#[allow(clippy::unused_async)]
pub(super) async fn run_server(weak_server: Weak<InnerServer>) {
    start_host_websocket(weak_server).await;
}

/// init func for main loop
#[allow(clippy::unused_async)]
async fn start_host_websocket(weak_server: Weak<InnerServer>) {
    let (incoming_sender, incoming_receiver) = channel::<ClientMsg>(127);
    tokio::spawn(websocket_connection_handler(
        weak_server.clone(),
        incoming_sender,
    ));
    message_parser(weak_server, incoming_receiver);
}

fn message_parser(cloneable_weak_server: Weak<InnerServer>, mut receiver: Receiver<ClientMsg>) {
    let parse_in_weak_server = cloneable_weak_server.clone();
    let parse_in = async move {
        let weak_server = parse_in_weak_server;
        loop {
            if let Some(cmsg) = receiver.recv().await {
                if weak_server.strong_count() == 0 {
                    break;
                }
                let client_idx = cmsg.client_idx;
                let msg = cmsg.msg;
                match msg {
                    //Test response, always in json
                    Message::Text(text) => {
                        let nt_msg_vec = match serde_json::from_str::<Vec<NTMessage>>(&text) {
                            Ok(nt_msg_vec) => nt_msg_vec,
                            Err(e) => {
                                tracing::debug!("Error parsing json: {} \n content: {}", e, text);
                                continue;
                            }
                        };
                        for nt_msg in nt_msg_vec {
                            //makes sure the server is still alive
                            let Some(server) = weak_server.upgrade() else {
                                tracing::debug!("Server dropped, client failed to subscribe");
                                break;
                            };
                            match nt_msg {
                                NTMessage::Publish(pub_topic) => {
                                    tracing::trace!("Client publishing topic");
                                    if let Err(e) =
                                        server.inner_publish_topic(pub_topic, client_idx).await
                                    {
                                        tracing::warn!("Client failed to publish topic: {e}");
                                    }
                                }
                                NTMessage::Unpublish(pub_topic) => {
                                    tracing::trace!("Client unpublishing topic");
                                    if let Err(e) =
                                        server.inner_unpublish_topic(pub_topic, client_idx).await
                                    {
                                        tracing::warn!("Client failed to unpublish topic: {e}");
                                    }
                                }
                                NTMessage::SetProperties(set_props) => {
                                    tracing::trace!("Client setting properties");
                                    if let Err(e) = server.set_properties(set_props).await {
                                        tracing::warn!("Client failed to set properties: {e}");
                                    }
                                }
                                NTMessage::Subscribe(sub) => {
                                    tracing::trace!("Client subscribing to topic");
                                    if let Err(e) = server.subscribe(sub, client_idx).await {
                                        tracing::warn!("Client failed to subscribe to topic: {e}");
                                    }
                                }
                                NTMessage::Unsubscribe(unsub) => {
                                    tracing::trace!("Client unsubscribing from topic");
                                    if let Err(e) = server.unsubscribe(unsub, client_idx).await {
                                        tracing::warn!("Client failed to unsubscribe from topic: {e}");
                                    }
                                }
                                NTMessage::Close(close) => {
                                    let ident = ClientIdentity::new(close.addr, close.name);
                                    if let Err(e) = server.disconnect_client(ident).await {
                                        tracing::warn!("Client failed to disconnect: {e}");
                                    }
                                }
                                _ => {
                                    tracing::error!("Client sent: {:?}", nt_msg);
                                }
                            }
                        }
                    }
                    Message::Binary(msgpack) => {
                        //casts msgpack to a VecDeque for Read trait
                        let mut msgpack = VecDeque::from(msgpack);
                        let server = upgrade_brk!(weak_server);
                        let clients = lock!(server.clients);
                        while let Ok(data) = rmp_serde::decode::from_read(&mut msgpack) {
                            match data {
                                rmpv::Value::Array(array) => {
                                    let mut array = VecDeque::from(array);
                                    //these are safe because binaries are verified before they reach here
                                    let topic_uid = u32::try_from(array
                                        .pop_front()
                                        .expect("Array was too short")
                                        .as_u64()
                                        .expect("MsgPack value could not be represented as u64"))
                                        .expect("MsgPack value could not be represented as u32");

                                    let timestamp = array
                                        .pop_front()
                                        .expect("Array was too short")
                                        .as_u64()
                                        .expect("MsgPack value could not be represented as u64");

                                    let value = FrcValue::try_from(
                                        array.pop_back().expect("Array was too short"),
                                    )
                                    .expect("Invalid msgpack value");

                                    if let Some(client) = clients.get(&client_idx) {
                                        if let Some(topic_idx) =
                                            client.pubuid_to_topicidx.get_by_left(&topic_uid)
                                        {
                                            let timestamped_value =
                                                FrcTimestampedValue { timestamp, value };
                                            server
                                                .set_value(timestamped_value, *topic_idx)
                                                .await
                                                .ok();
                                        } else {
                                            tracing::warn!("pubuid was not registered on client");
                                        }
                                    }
                                }
                                _ => {
                                    tracing::error!(
                                        "Server sent an invalid msgpack data, not an array."
                                    );
                                }
                            }
                        }
                    }
                    _ => {
                        tracing::warn!("Unknown message: {:?}", msg);
                    }
                }
            } else {
                tracing::error!("Message parser channel closed");
                break;
            }
        }
    };
    tokio::spawn(parse_in);

    let send_out_weak_server = cloneable_weak_server;
    let send_out = async move {
        let weak_server = send_out_weak_server;
        //in seconds the soonest the loop will need to next run
        let mut soonest_offset = 0.1f64;
        loop {
            let server = upgrade_brk!(weak_server);
            let subscriptions = lock!(server.subscriptions);

            if subscriptions.len() == 0 {
                drop(subscriptions);
                tokio::time::sleep(tokio::time::Duration::from_secs_f64(soonest_offset)).await;
                continue;
            }

            let mut sub_values = lock!(server.sub_values);
            let mut sub_timestamps = lock!(server.sub_send_timestamps);
            let mut clients = lock!(server.clients);

            #[allow(clippy::significant_drop_in_scrutinee)]
            for sub_entry in subscriptions.iter() {
                let sub_idx = sub_entry.0;
                let client_idx = sub_entry.1 .1;
                let subscription = &sub_entry.1 .0;
                let options = subscription
                    .options
                    .unwrap_or_default();

                if options.topics_only.unwrap_or(false) {
                    continue;
                }

                let send_instant = sub_timestamps.get(sub_idx).expect("Sub timestamp missing");
                if (send_instant.elapsed().as_secs_f64() - options.periodic.unwrap_or(0.1)).abs()
                    < 0.001
                {
                    soonest_offset =
                        options.periodic.unwrap_or(0.1) - send_instant.elapsed().as_secs_f64();
                    continue;
                }
                sub_timestamps.insert(*sub_idx, Instant::now());

                let Some(client) = clients.get_mut(&client_idx) else {
                    continue;
                };

                let values_to_send = sub_values.get_mut(sub_idx)
                    .expect("Sub values did not exists").take_values();
                for value in values_to_send {
                    let topic_idx = value.0;
                    let timestamped_values = value.1;
                    let mut buf = Vec::new();
                    for val in timestamped_values {
                        buf.extend(
                            val.as_bytes(
                                i32::try_from(topic_idx).expect("Topic idx overflowing i32")
                            ).expect("Failed to encode value"));
                    }
                    client.send_binary(buf).await.ok();
                }
            }
            drop(subscriptions);
            drop(sub_timestamps);
            drop(sub_values);
            drop(clients);
            drop(server);

            //can never sleep for more than 0.1 second, this means hypothetically a new sub could wait up to 0.1 second to get its first message
            tokio::time::sleep(tokio::time::Duration::from_secs_f64(
                (soonest_offset - 0.0005).clamp(0.0, 0.1),
            ))
            .await;
        }
    };
    tokio::spawn(send_out);
}

/// The main loop for the server, this is where all the connections are handled
async fn websocket_connection_handler(
    weak_server: Weak<InnerServer>,
    incoming_sender: Sender<ClientMsg>,
) {

    #[allow(clippy::unused_async)]
    async fn request(
        req: HRequest<Body>,
        addr: SocketAddr,
        weak_server: Weak<InnerServer>,
        incoming_sender: Sender<ClientMsg>,
    ) -> Result<HResponse<Body>, hyper::Error> {
        tracing::debug!("Request: {:?}", req);

        let mut req = req;

        let upgrade = HeaderValue::from_static("upgrade");
        let websocket = HeaderValue::from_static("websocket");
        let headers = req.headers();
        let version = req.version();
        let key = headers.get(SEC_WEBSOCKET_KEY);
        let derived = key.map(|k| derive_accept_key(k.as_bytes()));
        if req.method() != Method::GET
            || req.version() != Version::HTTP_11
            || !headers
                .get(UPGRADE)
                .and_then(|val| val.to_str().ok())
                .is_some_and(|val| val.to_ascii_lowercase().contains("websocket"))
            || key.is_none()
        {
            tracing::warn!("Invalid websocket request");
            return Ok(HResponse::new(Body::from("Hello World!")));
        }

        let header_str = headers
            .get(SEC_WEBSOCKET_PROTOCOL)
            .and_then(|val| val.to_str().ok());

        let mut is_rtt = false;

        match header_str {
            Some(hdr)
                if hdr
                    .to_ascii_lowercase()
                    .contains("rtt.networktables.first.wpi.edu") =>
            {
                is_rtt = true;
            }
            Some(hdr) if hdr.to_ascii_lowercase().contains("networktables") => {}
            _ => {
                tracing::warn!("Invalid websocket protocol");
                return Ok(HResponse::new(Body::from("Hello World!")));
            }
        }

        let name = req.uri().path().trim_start_matches("/nt/").to_string();

        //HAS TO BE UPGRADED IN A DIFF TASK FOR SOME REASON
        tokio::spawn(async move {
            let stream;
            if let Ok(upgraded) = hyper::upgrade::on(&mut req).await {
                stream = WebSocketStream::from_raw_socket(
                    upgraded,
                    tokio_tungstenite::tungstenite::protocol::Role::Server,
                    None,
                )
                .await;
            } else {
                tracing::warn!("Failed to upgrade");
                return;
            }

            if !is_rtt {
                let mut client_iden = ClientIdentity::new(addr, name.clone());

                let connect_result = upgrade_ret!(weak_server)
                    .connect_client(&mut client_iden)
                    .await;

                let Ok((kill_switch, outgoing_receiver, idx)) = connect_result else {
                    tracing::error!("{}@{} is already a connected client", &name, &addr);
                    return;
                };

                handle_connection(
                    stream,
                    client_iden,
                    idx,
                    incoming_sender,
                    outgoing_receiver,
                    kill_switch,
                )
                .await;
            }
        });

        let response = HResponse::builder()
            .status(StatusCode::SWITCHING_PROTOCOLS)
            .version(version)
            .header(UPGRADE, websocket)
            .header(CONNECTION, upgrade)
            .header(SEC_WEBSOCKET_ACCEPT, derived.unwrap())
            .body(Body::empty())
            .unwrap();
        Ok(response)
    }

    let server_addr = weak_server
        .upgrade()
        .unwrap()
        .server_addr;

    let make_svc = make_service_fn(move |connconn: &AddrStream| {
        let addr = connconn.remote_addr();
        let weak_clone = weak_server.clone();
        let sender_clone = incoming_sender.clone();
        let service = service_fn(move |req: HRequest<Body>| {
            tracing::debug!("Got request at {}", addr);
            request(req, addr, weak_clone.clone(), sender_clone.clone())
        });
        async { Ok::<_, Infallible>(service) }
    });

    let server = HServer::bind(&server_addr).serve(make_svc);

    if let Err(e) = server.await {
        tracing::error!("server error: {}", e);
    }
}

/// The main loop for a connection, this is where all the messages are handled
async fn handle_connection(
    websocket: WebSocketStream<Upgraded>,
    identity: ClientIdentity,
    idx: ClientIdx,
    in_channel: Sender<ClientMsg>,
    mut out_channel: Receiver<Message>,
    kill_switch: Arc<AtomicBool>,
) {
    let (mut outgoing, incoming) = websocket.split();

    let (time_sender, mut time_receiver) = channel::<Message>(1);

    let addr = identity.addr();

    // A clone of killswitch to move into the async block
    let ks_1 = kill_switch.clone();

    // The incoming message loop
    let broadcast_incoming_inner = incoming.try_for_each_concurrent(None, |msg| async {
        tracing::trace!("Received message from {}: {:?}", &addr, &msg);

        if let Message::Binary(data) = &msg {
            if data.len() < 2 {
                tracing::warn!("Received too short of a message");
                return Ok(());
            }
            //len of mp array check
            if data[0] != 148u8 {
                tracing::warn!("Received a non array binary msg");
                return Ok(());
            }
            //if id of -1 reroute as a time sync message for minimal latency
            if data[1] == 210u8 {
                //simply checks if id is negative, there should be no other reason id is negative
                let mut msgpack = VecDeque::from(data.clone());
                let arr: Result<rmpv::Value, rmp_serde::decode::Error> =
                    rmp_serde::decode::from_read(&mut msgpack);

                #[allow(clippy::all)]
                if let Ok(array) = arr {
                    if let rmpv::Value::Array(mut arr) = array {
                        if let Some(last) = arr.pop() {
                            // let time = match last {
                            //     // rmpv::Value::F32(val) => val as FrcTimestamp,
                            //     // rmpv::Value::F64(val) => val as FrcTimestamp,
                            //     rmpv::Value::Integer(val) => val.as_u64().unwrap_or_default(),
                            //     _ => {
                            //         tracing::warn!("Received a non timestamp value");
                            //         return Ok(());
                            //     }
                            // };
                            let time = if let rmpv::Value::Integer(val) = last {
                                val.as_u64().unwrap_or_default() 
                            } else {
                                tracing::warn!("Received a non timestamp value");
                                return Ok(());
                            };

                            let mut buf = Vec::with_capacity(24);
                            rmp::encode::write_array_len(&mut buf, 4).unwrap();
                            rmp::encode::write_i32(&mut buf, -1).unwrap();
                            rmp::encode::write_uint(&mut buf, now()).unwrap();
                            rmp::encode::write_u8(&mut buf, FrcType::Int.as_u8()).unwrap();
                            rmp::encode::write_uint(&mut buf, time).unwrap();

                            let message = Message::Binary(buf);

                            tracing::trace!("Responded to time sync");

                            if let Err(err) = time_sender.send(message).await {
                                tracing::warn!(
                                    "Failed to send message to internal time channel: {}",
                                    err
                                );
                            };
                            return Ok(());
                        }
                    }
                }
            }
        }
        if let Message::Ping(_) = &msg {
            tracing::debug!("Responded to ping");
            if let Err(err) = time_sender.send(Message::Pong(vec![])).await {
                tracing::warn!("Failed to send message to internal time channel: {}", err);
            };
            return Ok(());
        }
        if let Message::Pong(_) = &msg {
            tracing::debug!("Responded to pong");
            return Ok(());
        }
        if let Message::Close(_) = &msg {
            tracing::debug!("{} disconnected", &addr);
            ks_1.store(true, Ordering::Relaxed);
            return Ok(());
        }
        if let Err(err) = in_channel
            .send(ClientMsg {
                client_idx: idx,
                msg,
            })
            .await
        {
            tracing::warn!("Failed to send message to internal channel: {}", &err);
            ks_1.store(true, Ordering::Relaxed);
        };
        Ok(())
    });

    let broadcast_incoming = async {
        if let Err(err) = broadcast_incoming_inner.await {
            tracing::warn!("Failed to receive message from {}: {}", &addr, err);
        }
        kill_switch.store(true, Ordering::Relaxed);
    };

    let ks_2 = kill_switch.clone();
    //create a 10ms interval that can be used for flushing
    let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
    let broadcast_outgoing = async {
        loop {
            select! {
                _ = interval.tick() => {
                    if let Err(err) = SinkExt::flush(&mut outgoing).await {
                        tracing::warn!("Failed to flush outgoing messages to {}: {}", &addr, err);
                    }
                },
                msg = out_channel.recv() => {
                    if let Some(msg) = msg {
                        //using an empty binary message as a flush
                        if let Message::Binary(bin) = &msg {
                            if bin.len() == 5 && bin[0..5] == [b'f', b'l', b'u', b's', b'h'] {
                                tracing::trace!("Force flushing messages to {}", &addr);
                                if let Err(err) = SinkExt::flush(&mut outgoing).await {
                                    tracing::warn!("Failed to flush outgoing messages to {}: {}", &addr, err);
                                }
                            }
                        } else {
                            tracing::trace!("Sending message to {}: {:?}", &addr, &msg);
                            //feed will not flush the sink, so it will not clog up the network
                            if let Err(err) = SinkExt::feed(&mut outgoing, msg).await {
                                tracing::warn!("Failed to send message to {}: {}", &addr, err);
                            }
                        }
                    } else {
                        tracing::debug!("{} disconnected", &addr);
                        ks_2.store(true, Ordering::Relaxed);
                    }
                },
                msg = time_receiver.recv() => {
                    if let Some(msg) = msg {
                        tracing::trace!("Sending time message to {}: {:?}", &addr, &msg);
                        //send will flush but it is wanted for quickest rtt
                        if let Err(err) = SinkExt::send(&mut outgoing, msg).await {
                            tracing::warn!("Failed to send time message to {}: {}", &addr, err);
                        }
                    } else {
                        tracing::debug!("{} disconnected", &addr);
                        ks_2.store(true, Ordering::Relaxed);
                    }
                }
            }
            if ks_2.load(Ordering::Relaxed) {
                break;
            }
        }
    };

    pin_mut!(broadcast_incoming, broadcast_outgoing);

    select! {
        () = broadcast_incoming => {},
        () = broadcast_outgoing => {}
    }

    tracing::debug!("Connection closed for {}", &identity);

    let nt_message = identity.as_close();

    //will be parsed and update inner state
    in_channel
        .send(ClientMsg {
            client_idx: idx,
            msg: Message::Text(serde_json::to_string(&nt_message).unwrap()),
        })
        .await
        .expect("Failed to send client close message to internal channel");
}

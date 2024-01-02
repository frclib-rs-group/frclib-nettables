use std::net::{Ipv4Addr, SocketAddrV4};

use frclib_core::value::FrcType;

use crate::{
    client::{config::ClientConfig, AsyncClientHandle},
    server::{AsyncServerHandle, BlockingServerHandle, config::ServerConfig},
    spec::subscription::SubscriptionOptions,
};

#[tokio::test]
async fn async_client_and_server() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_line_number(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default tracing subscriber");

    let socket_addr =
        std::net::SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 5810));

    let server = AsyncServerHandle::start(socket_addr, ServerConfig::default())
        .await
        .expect("Server failed to start");

    let cfg = ClientConfig {
        connect_timeout: 100_000,
        should_reconnect: Box::new(|_| false),
        ..Default::default()
    };

    let client = AsyncClientHandle::start(socket_addr, cfg, "test".to_string())
        .await
        .expect("Client failed to start");

    let pub_topic = client
        .publish_topic("/test_topic", FrcType::Double, None)
        .await
        .expect("CLIENT: Failed to publish topic");

    client
        .publish_value(&pub_topic, &rmpv::Value::F64(10.0))
        .await
        .expect("CLIENT: Failed to publish value");

    let sub_str = vec!["/test"];
    let sub = client
        .subscribe_w_options(
            &sub_str,
            Some(SubscriptionOptions {
                all: Some(true),
                prefix: Some(true),
                ..Default::default()
            }),
        )
        .await
        .expect("CLIENT: Failed to subscribe");


    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    server.set_topic_value("/test_topic", 0.0f64)
        .await
        .expect("SERVER: Failed to set topic value");

    for _ in 0..5 {
        client
            .publish_value(&pub_topic, &rmpv::Value::F64(10.0))
            .await
            .expect("CLIENT: Failed to publish value");
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
    }

    client.unpublish(pub_topic)
        .await
        .expect("CLIENT: Failed to unpublish topic");
    client.unsubscribe(sub)
        .await
        .expect("CLIENT: Failed to unsubscribe");

    drop(client);

    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
}

#[tokio::test]
async fn robot_sim_server_test() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_line_number(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let socket_addr =
        std::net::SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 5810));

    let cfg = ClientConfig {
        connect_timeout: 100_000,
        should_reconnect: Box::new(|_| false),
        ..Default::default()
    };

    let chandle = AsyncClientHandle::start(socket_addr, cfg, "test".to_string())
        .await
        .unwrap();

    let sub_str = vec!["/RobotState"];

    let sub = chandle
        .subscribe_w_options(
            &sub_str,
            Some(SubscriptionOptions {
                all: Some(false),
                prefix: Some(true),
                ..Default::default()
            }),
        )
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(50)).await;

    let _ = sub.as_unsubscribe();
}

#[test]
fn blocking_server() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_line_number(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let socket_addr =
        std::net::SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 5810));

    let _shandle = BlockingServerHandle::start(socket_addr, ServerConfig::default()).unwrap();
}

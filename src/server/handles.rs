use std::{net::SocketAddr, sync::Arc};

use frclib_core::value::{FrcTimestampedValue, FrcValue, IntoFrcValue};

use crate::{
    error::NtSuccess,
    generate_uid, now,
    spec::{
        messages::{PublishTopic, SetProperties},
        topic::PublishProperties,
    },
    NetworkTablesError,
};

use super::{config::ServerConfig, inner::InnerServer};

#[derive(Debug, Clone)]
pub struct AsyncServerHandle {
    uid: u64,
    inner: Arc<InnerServer>,
}

impl AsyncServerHandle {
    //async to ensure being called from an async context
    pub async fn start(
        server_addr: impl Into<SocketAddr> + Send,
        config: ServerConfig,
    ) -> Result<Self, crate::NetworkTablesError> {
        let server_addr = server_addr.into();
        let inner = InnerServer::try_new(server_addr, config).await?;

        tracing::debug!("Server started on {}", server_addr);

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&server_addr, &mut hasher);
        let uid = std::hash::Hasher::finish(&hasher);

        let handle = Self {
            uid,
            inner: Arc::new(inner),
        };
        super::runner::run_server(Arc::downgrade(&handle.inner)).await;

        tracing::debug!("Server runner started");

        Ok(handle)
    }
}
impl PartialEq for AsyncServerHandle {
    fn eq(&self, other: &Self) -> bool {
        self.uid == other.uid
    }
}

#[derive(Debug, Clone)]
pub struct BlockingServerHandle {
    uid: u64,
    inner: Arc<InnerServer>,
    rt: Arc<tokio::runtime::Runtime>,
}

impl BlockingServerHandle {
    /// Starts a new server on the given address and spins up a new tokio runtime to run it
    /// 
    /// # Panics
    /// Panics if the runtime fails to start
    pub fn start(
        server_addr: impl Into<std::net::SocketAddr>,
        config: ServerConfig,
    ) -> Result<Self, crate::NetworkTablesError> {
        let server_addr = server_addr.into();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .thread_name("nt-server")
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("Failed to start tokio runtime");
        let inner = rt.block_on(InnerServer::try_new(server_addr, config))?;

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&server_addr, &mut hasher);
        let uid = std::hash::Hasher::finish(&hasher);

        let handle = Self {
            uid,
            inner: Arc::new(inner),
            rt: Arc::new(rt),
        };
        handle
            .rt
            .block_on(super::runner::run_server(Arc::downgrade(&handle.inner)));

        tracing::debug!("Server started on {}", server_addr);
        Ok(handle)
    }
}
impl PartialEq for BlockingServerHandle {
    fn eq(&self, other: &Self) -> bool {
        self.uid == other.uid
    }
}

async_blocking_bridger::async_inner!(
    impl Server {
        /// This creates a new topic with the given name and type
        ///
        /// The value will be timestamped with current server time
        async fn publish_topic_w_props(
            &inner,
            topic_name: impl ToString + Send,
            value: impl IntoFrcValue,
            props: Option<PublishProperties>
        ) -> Result<NtSuccess, NetworkTablesError> {
            let val = value.into_frc_value();
            let topic_idx = inner.inner_publish_topic(PublishTopic {
                name: topic_name.to_string(),
                pubuid: generate_uid() as u32,
                r#type: val.get_type(),
                properties: props
            }, 0).await?;

            inner.set_value(
                FrcTimestampedValue::new(now(), val),
                topic_idx
            ).await?;

            Ok(NtSuccess)
        }

        /// This creates a new topic with the given name and type and assuming null properties
        async fn publish_topic(
            &inner,
            topic_name: impl ToString + Send,
            value: impl IntoFrcValue,
        ) -> Result<NtSuccess, NetworkTablesError> {
            inner.publish_topic_w_props(topic_name, value, None).await
        }

        /// Sets the value of a topic using the supplied timestamp
        ///
        /// Note: if this timestamp is earlier than an already set timestamp, this will have no effect
        async fn set_topic_value_w_timestamp(
            &inner,
            topic_name: impl ToString + Send,
            value: impl IntoFrcValue,
            timestamp: u64
        ) -> Result<NtSuccess, NetworkTablesError> {
            let topic_idx = inner.get_topic_idx(&topic_name.to_string()).await?;
            inner.set_value(
                FrcTimestampedValue::new(timestamp, value.into_frc_value()),
                topic_idx
            ).await?;

            Ok(NtSuccess)
        }

        /// Sets the value of a topic using the current server time
        async fn set_topic_value(
            &inner,
            topic_name: impl ToString + Send,
            value: impl IntoFrcValue,
        ) -> Result<NtSuccess, NetworkTablesError> {
            inner.set_topic_value_w_timestamp(topic_name, value, now()).await
        }

        /// Sets the properties of a topic with the matching name
        async fn set_topic_props(
            &inner,
            topic_name: impl ToString + Send,
            props: PublishProperties
        ) -> Result<NtSuccess, NetworkTablesError> {
            inner.set_properties(SetProperties {
                name: topic_name.to_string(),
                update: props
            }).await?;

            Ok(NtSuccess)
        }

        async fn get_topic_value(
            &inner,
            topic_name: &String,
        ) -> Result<FrcValue, NetworkTablesError> {
            Ok(inner.get_value(
                inner.get_topic_idx(topic_name).await?
            ).await?.value)
        }

        async fn get_topic_names(
            &inner,
        ) -> Result<Vec<String>, NetworkTablesError> {
            let mut names = Vec::with_capacity(64);
            lock!(inner.topic_map).iter().for_each(|(name, _)| {
                names.push(name.clone());
            });

            Ok(names)
        }
    }
);

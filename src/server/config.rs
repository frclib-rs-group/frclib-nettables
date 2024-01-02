use std::{fmt::Debug, path::PathBuf};

use futures_util::future::BoxFuture;

use crate::spec::topic::Topic;

// use crate::spec::topic::Topic;

#[allow(clippy::type_complexity)]
pub struct ServerConfig {
    pub persistent_path: PathBuf,
    //callbacks
    pub on_client_connect: Box<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
    pub on_client_disconnect: Box<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
    pub on_topic_announce: Box<dyn Fn(&Topic) -> BoxFuture<'static, ()> + Send + Sync>,
    pub on_topic_unannounce: Box<dyn Fn(Option<Topic>) -> BoxFuture<'static, ()> + Send + Sync>,
}

impl Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfig")
            .field("persistent_path", &self.persistent_path)
            .finish_non_exhaustive()
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            persistent_path: PathBuf::from("networktables_persistent.json"),
            on_client_connect: Box::new(|| Box::pin(async {})),
            on_client_disconnect: Box::new(|| Box::pin(async {})),
            on_topic_announce: Box::new(|_| Box::pin(async {})),
            on_topic_unannounce: Box::new(|_| Box::pin(async {})),
        }
    }
}

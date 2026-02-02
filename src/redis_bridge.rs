use actix::prelude::*;
use bytes::Bytes;
use futures_util::StreamExt;

use crate::hub::{Broadcast, ServerMessage};

#[derive(Debug, Clone)]
pub struct RedisBridgeConfig {
    pub url: String,
    pub channel: String,
}

pub async fn spawn_redis_subscriber(
    cfg: RedisBridgeConfig,
    hub: Addr<crate::Hub>,
) -> redis::RedisResult<tokio::task::JoinHandle<()>> {
    let client = redis::Client::open(cfg.url)?;
    let mut pubsub = client.get_async_pubsub().await?;
    pubsub.subscribe(cfg.channel.clone()).await?;

    Ok(tokio::spawn(async move {
        let mut stream = pubsub.on_message();
        while let Some(msg) = stream.next().await {
            let payload: Vec<u8> = match msg.get_payload() {
                Ok(p) => p,
                Err(_) => continue,
            };
            hub.do_send(Broadcast(ServerMessage::Binary(Bytes::from(payload))));
        }
    }))
}

pub async fn publish_bytes(cfg: &RedisBridgeConfig, payload: Bytes) -> redis::RedisResult<()> {
    let client = redis::Client::open(cfg.url.as_str())?;
    let mut conn = redis::aio::ConnectionManager::new(client).await?;
    redis::cmd("PUBLISH")
        .arg(&cfg.channel)
        .arg(payload.as_ref())
        .query_async::<_, ()>(&mut conn)
        .await?;
    Ok(())
}

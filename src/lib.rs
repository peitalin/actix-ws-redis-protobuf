//! Actix Web websocket broadcast hub with optional Redis and Protobuf helpers.
//!
//! Start with [`Hub`] + [`WsSession`] and mount a websocket route in your Actix Web app.
pub mod hub;
pub mod ws;

#[cfg(feature = "protobuf")]
pub mod proto;

#[cfg(feature = "redis")]
pub mod redis_bridge;

pub use hub::{Broadcast, Hub, HubCommand, ServerMessage};
pub use ws::WsSession;

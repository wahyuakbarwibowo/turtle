//! Meeting chat: persisted in PostgreSQL, delivered live over LiveKit's data channel.
//! Sending goes through the API so the sender identity is authenticated and every message is stored.

pub mod http;
pub mod service;

/// Topic clients subscribe to with `useDataChannel`.
pub const CHAT_TOPIC: &str = "turtlemeet.chat";

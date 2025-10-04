pub mod http;
// pub mod udp;
// pub mod https;

use std::net::SocketAddr;

// re-export
pub use http::HttpTracker;

use async_trait::async_trait;
use anyhow::Result;
use serde::Deserialize;

#[async_trait]
pub trait Tracker {
    async fn announce(&self, info_hash: &[u8]) -> Result<TrackerResponse>;
    fn url(&self) -> &str;
}   

#[derive(Deserialize, Debug)]
pub struct TrackerResponse {
    pub interval: u64,
    pub peers: serde_bencode::value::Value,
}
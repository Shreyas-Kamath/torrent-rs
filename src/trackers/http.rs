use crate::trackers::TrackerResponse;

use super::Tracker;

use anyhow::Result;
use async_trait::async_trait;
use url::{Url, form_urlencoded};

pub struct HttpTracker {
    pub url: String,
    client: reqwest::Client,
}

#[async_trait]
impl Tracker for HttpTracker {
    async fn announce(&self, info_hash: &[u8]) -> Result<TrackerResponse> {
        println!("Announcing to HTTP tracker at {}", self.url);
        let mut url = Url::parse(&self.url)?;
        let encoded_info_hash: String = form_urlencoded::byte_serialize(info_hash).collect();

        url.query_pairs_mut()
            .append_pair("info_hash", &encoded_info_hash)
            .append_pair("peer_id", "-TR2940-123456789012") // Example peer ID
            .append_pair("port", "6881") // Example port
            .append_pair("uploaded", "0")
            .append_pair("downloaded", "0")
            .append_pair("left", "0")
            .append_pair("compact", "1");

        let response_bytes = self.client.get(url).send().await?.bytes().await?;

        Ok(serde_bencode::from_bytes(&response_bytes)?)
    }

    fn url(&self) -> &str {
        &self.url
    }
}

impl HttpTracker {
    pub fn new(url: &str) -> Self {
        Self { 
            url: url.to_string(),
            client: reqwest::Client::new(),
         }
    }
}

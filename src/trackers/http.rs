use crate::trackers::TrackerResponse;
use super::Tracker;

use anyhow::Result;
use async_trait::async_trait;
use url::form_urlencoded::{self};

pub struct HttpTracker {
    pub url: String,
    client: reqwest::Client,
}

#[async_trait]
impl Tracker for HttpTracker {
    async fn announce(&self, info_hash: &[u8]) -> Result<TrackerResponse> {
        println!("Announcing to HTTP tracker at {}", self.url);
        let encoded_info_hash: String = form_urlencoded::byte_serialize(info_hash).collect();

        let url = format!(
            "{}?info_hash={}&peer_id={}&port={}&uploaded=0&downloaded=0&left=0&compact=1&event=started",
            self.url,
            encoded_info_hash,
            "-TR1012-123456789012",
            6881
        );

        let response_bytes = self.client.get(url).send().await?.bytes().await?;

        Ok(serde_bencode::from_bytes::<TrackerResponse>(&response_bytes)?)
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

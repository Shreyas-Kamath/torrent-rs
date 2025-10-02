use anyhow::Result;
use tokio::time::{sleep, Duration};
use sha1::{self, Digest};
use std::sync::Arc;

mod trackers;
mod torrent;
mod peers;

use trackers::{Tracker, HttpTracker, TrackerResponse};
use peers::parse_peers;

async fn run_tracker(tracker: Box<dyn Tracker + Send + Sync>, info_hash: Arc<Vec<u8>>) -> Result<()> {
    loop {
        let resp: TrackerResponse = tracker.announce(&info_hash).await?;
        let peers = parse_peers(&resp.peers);
        println!(
            "Tracker {} returned {} peers, will re-announce in {}s",
            tracker.url(),
            peers.len(),
            resp.interval
        );

        // Async sleep for this tracker
        sleep(Duration::from_secs(resp.interval)).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Example flattened announce list

    let torrent = torrent::load_torrent("torrents/Multiple-InstanceLearningofReal-ValuedData.pdf-936a92932c01c3f5e9994ae8bd2115f4ccb4adc9.torrent")?;

    let info_bytes = serde_bencode::to_bytes(&torrent.info)?;
    let info_hash: [u8; 20] = sha1::Sha1::digest(&info_bytes).into();
    let info_hash_vec = Arc::new(info_hash.to_vec());

    let tracker_objs: Vec<Box<dyn Tracker + Send + Sync>> = torrent.announce_list
        .iter()
        .flatten()
        .flatten()
        .map(|url| {
                Box::new(HttpTracker::new(url)) as Box<dyn Tracker + Send + Sync>
        })
        .collect();

    // Spawn tasks and keep handles
    let handles: Vec<_> = tracker_objs
        .into_iter()
        .map(|tracker| tokio::spawn(run_tracker(tracker, info_hash_vec.clone())))
        .collect();

    // Await all tasks (they run indefinitely)
    for handle in handles {
        handle.await??; // propagate any errors
    }

    Ok(())
}

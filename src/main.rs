use tokio::time::{sleep, Duration};
use sha1::{self, Digest};
use std::{collections::HashSet, net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;

mod trackers;
mod torrent;
mod peers;
mod pieces;

use trackers::{Tracker, HttpTracker, TrackerResponse};
use peers::{parse_peers, PeerConnection};

use crate::pieces::piece_manager::PieceManager;

type PeerPool = Arc<Mutex<HashSet<SocketAddr>>>;

// update peer pool

async fn run_tracker(
    tracker: Box<dyn Tracker + Send + Sync>,
    info_hash: Arc<Vec<u8>>,
    peer_pool: PeerPool,
    pm: Arc<Mutex<PieceManager>>
) -> anyhow::Result<()> {
        loop {
        let resp: TrackerResponse = tracker.announce(&info_hash).await?;
        let peers = parse_peers(&resp.peers);

        let mut pool = peer_pool.lock().await;

        for peer_addr in peers {
            if pool.insert(peer_addr) {
                let pm_clone = pm.clone();
                tokio::spawn({
                    let info_clone = info_hash.clone();
                    async move {
                        if let Ok(conn) = PeerConnection::new(peer_addr, info_clone, pm_clone).await {
                            if let Err(e) = conn.start().await {
                                eprintln!("Peer {} failed: {:?}", peer_addr, e);
                            }
                        }
                    }
                });
            }
        }
        
        sleep(Duration::from_secs(resp.interval)).await;
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let torrent = torrent::load_torrent("torrents/Multiple-InstanceLearningofReal-ValuedData.pdf-936a92932c01c3f5e9994ae8bd2115f4ccb4adc9.torrent")?;
    let info_bytes = serde_bencode::to_bytes(&torrent.info)?;
    let info_hash: [u8; 20] = sha1::Sha1::digest(&info_bytes).into();
    let info_hash_vec = Arc::new(info_hash.to_vec());

    let peer_pool: PeerPool = Arc::new(Mutex::new(HashSet::new()));

    let pm = Arc::new(Mutex::new(PieceManager::new(torrent.info.piece_length, torrent.total_length(), &torrent.info.pieces)));

    let tracker_objs: Vec<Box<dyn Tracker + Send + Sync>> = torrent
        .announce_list
        .iter()
        .flatten()
        .flatten()
        .filter(|url| url.starts_with("http"))
        .map(|url| Box::new(HttpTracker::new(url)) as Box<dyn Tracker + Send + Sync>)
        .collect();

    for tracker in tracker_objs {
        let info_clone = Arc::clone(&info_hash_vec);
        let pool_clone = Arc::clone(&peer_pool);

        let pm_clone = pm.clone();
        tokio::spawn(async move {
            if let Err(e) = run_tracker(tracker, info_clone, pool_clone, pm_clone).await {
                eprintln!("Tracker task failed: {:?}", e);
            }
        });
    }

    // Optional: monitor peer pool
    loop {
        {
            let pool = peer_pool.lock().await;
            println!("Currently {} peers", pool.len());
        }
        sleep(Duration::from_secs(30)).await;
    }
}
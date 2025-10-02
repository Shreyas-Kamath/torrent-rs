mod bencode;
mod torrent;

fn main() -> anyhow::Result<()> {
    println!("Starting...");

    let torrent = torrent::load_torrent("torrents/Multiple-InstanceLearningofReal-ValuedData.pdf-936a92932c01c3f5e9994ae8bd2115f4ccb4adc9.torrent")?;
    println!("Torrent name: {}", torrent.info.name);
    
    Ok(())
}

use std::net::{Ipv4Addr, SocketAddrV4, SocketAddr};

use serde_bencode::value::Value;

// #[derive(Deserialize, Debug)]
// pub struct PeerDict {
//     pub ip: String,
//     pub port: u16,
//     #[serde(rename = "peer id")]
//     pub peer_id: String,
// }

pub struct Peer {
    pub addr: std::net::SocketAddr,
    pub peer_id: Option<String>,
    pub downloaded: u64,
    pub uploaded: u64,
    pub left: u64,
}

impl Peer {
    pub fn new(addr: std::net::SocketAddr) -> Self {
        Self {
            addr,
            peer_id: None,
            downloaded: 0,
            uploaded: 0,
            left: 0,
        }
    }
}

pub fn parse_peers(value: &Value) -> Vec<SocketAddr> {
    match value {
        Value::Bytes(bytes) => {
            // compact peers
            bytes.chunks(6)
                .filter_map(|chunk| {
                    if chunk.len() != 6 { return None; }
                    let ip = std::net::Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
                    let port = u16::from_be_bytes([chunk[4], chunk[5]]);
                    Some(SocketAddr::V4(std::net::SocketAddrV4::new(ip, port)))
                })
                .collect()
        }
        Value::List(list) => {
            // dictionary peers
            list.iter().filter_map(|item| {
                if let Value::Dict(dict     ) = item {
                    // extract "ip" as bytes
                    let ip_bytes = dict.get(&b"ip".to_vec())?;
                    let ip_str = if let Value::Bytes(b) = ip_bytes {
                        std::str::from_utf8(b).ok()?
                    } else { return None; };

                    // extract "port" as integer
                    let port = if let Value::Int(i) = dict.get(&b"port".to_vec())? {
                        *i as u16
                    } else { return None; };
                    // parse ip string into Ipv4Addr
                    let ip_parsed: Ipv4Addr = ip_str.parse().ok()?;
                    Some(SocketAddr::V4(SocketAddrV4::new(ip_parsed, port)))
                } else {
                    None
                }
            }).collect()
        }
        _ => vec![],
    }
}

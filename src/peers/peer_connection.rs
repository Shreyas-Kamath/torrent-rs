use crate::peers::peer::Peer;
use crate::pieces::piece_manager::PieceManager;
use std::sync::Arc;
use anyhow::Ok;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

pub struct PeerConnection {
    peer: Peer,
    stream: tokio::net::TcpStream,
    bitfield: Vec<bool>,
    info_hash: Arc<Vec<u8>>,
    in_flight_blocks: u32,
    peer_id: String,

    // buffers
    length_buf: [u8; 4],
    msg_buf: Vec<u8>,
    handshake_buf: [u8; 68],

    // states
    am_choked: bool,
    peer_choked: bool,
    am_interested: bool,

    piece_manager: Arc<Mutex<PieceManager>>
}

impl PeerConnection {
    pub async fn new(peer_addr: std::net::SocketAddr, info_hash: Arc<Vec<u8>>, pm: Arc<Mutex<PieceManager>>) -> anyhow::Result<Self> {
        let peer = Peer::new(peer_addr);
        let stream = tokio::net::TcpStream::connect(peer_addr).await?;
        let bitfield = vec![];
        let in_flight_blocks = 0;
        let peer_id: String = String::from("-TR1012-123456789012");

        Ok(PeerConnection {
            peer,
            stream,
            bitfield,
            info_hash,
            in_flight_blocks,
            peer_id,
            length_buf: [0; 4],
            msg_buf: Vec::new(),
            handshake_buf: [0; 68],
            am_choked: true,
            peer_choked: true,
            am_interested: false,
            piece_manager: pm.clone(),
        })
    }

    pub async fn start(mut self) -> anyhow::Result<()> {
        if let Err(e) = self.perform_handshake().await {
            eprintln!("Handshake failed for {}: {:?}", self.peer.addr, e);
        }
        println!("Handshake successful for {}", self.peer.addr);

        loop {
            match self.read_message_length().await? {
                None => {
                    // keep conn alive
                    continue;
                }
                Some(len) => {
                    self.read_message_body(len).await?;
                }
            }
        }
    }

    async fn perform_handshake(&mut self) -> anyhow::Result<()> {
        self.handshake_buf[0] = 19; // pstrlen 
        self.handshake_buf[1..20].copy_from_slice(b"BitTorrent protocol");
        self.handshake_buf[20..28].fill(0);
        self.handshake_buf[28..48].copy_from_slice(&self.info_hash);
        self.handshake_buf[48..68].copy_from_slice(self.peer_id.as_bytes());

        self.stream.write_all(&self.handshake_buf).await?;
        self.stream.read_exact(&mut self.handshake_buf).await?;

        if self.handshake_buf[28..48] != self.info_hash[..] {
            return Err(anyhow::anyhow!("Info hash mismatch"));
        }
        Ok(())
    }

    async fn read_message_length(&mut self) -> anyhow::Result<Option<u32>> {
        self.stream.read_exact(&mut self.length_buf).await?;
        let length = u32::from_be_bytes(self.length_buf);

        if length == 0 { Ok(None) }
        else { Ok(Some(length)) }
    }

    async fn read_message_body(&mut self, length: u32) -> anyhow::Result<()> {
        // read 4-byte length prefix
        
        self.msg_buf.resize(length as usize, 0);
        self.stream.read_exact(&mut self.msg_buf).await?;
        self.handle_message().await?;
        Ok(())
    }

    async fn handle_message(&mut self) -> anyhow::Result<()> {
        if self.msg_buf.is_empty() { return Ok(()) }

        match self.msg_buf[0] {
            0 => {
                self.am_choked = true;
                println!("{} choked us", self.peer.addr);
            }

            1 => {
                self.am_choked = false;
                println!("{} unchoked us", self.peer.addr);
                if self.am_choked { todo!("implement requests"); }
            }

            2 => {
                println!("{} is interested", self.peer.addr);
                todo!("implement seeding");
            }

            3 => {
                println!("{} is not interested", self.peer.addr);
                todo!("implement seeding");
            }

            4 => {
                println!("{} has a piece", self.peer.addr);
                self.handle_have()?;
            }

            5 => {
                println!("{} sent bitfield", self.peer.addr);
                self.handle_bitfield().await?;
            }

            6 => {
                println!("{} is requesting a piece", self.peer.addr);
                todo!("implement seeding");
            }

            7 => {
                println!("{} sent a piece", self.peer.addr);
                todo!("handle piece");
            }

            8 => {
                println!("{} sent cancel", self.peer.addr);
            }

            9 => {
                println!("{} sent port", self.peer.addr);
            }

            _ => {
                print!("unknown message id: {}", self.msg_buf[0]);
            }
        }
        Ok(())
    }

    fn handle_have(&mut self) -> anyhow::Result<()> {
        if self.msg_buf.len() < 5 { return Ok (()); }

        let piece_index_network_order: [u8; 4] = self.msg_buf[1..5]
        .try_into()?;

        let piece_index = u32::from_be_bytes(piece_index_network_order) as usize;
        println!("{} has piece: {}", self.peer.addr, piece_index);

        self.bitfield[piece_index] = true;

        Ok(())
    }

    async fn handle_bitfield(&mut self) -> anyhow::Result<()> {
        // Scoped lock
        let has_piece_we_need = {
            let pm = self.piece_manager.lock().await;
            let num_pieces = pm.num_pieces;

            self.bitfield.resize(num_pieces, false);

            // set bitfield
            for (i, &byte) in self.msg_buf[1..].iter().enumerate() {
                for bit in 0..8 {
                    let piece_index = i * 8 + bit;
                    if piece_index >= self.bitfield.len() { break; }
                    self.bitfield[piece_index] = (byte & (1 << (7 - bit))) != 0;
                }
            } 

            // check if peer has any piece we need
            !self.am_interested && pm.peer_has_piece_we_dont(&self.bitfield)
        };

        if has_piece_we_need {
            self.am_interested = true;
            self.send_interested().await?;
        }

        Ok(())
    }

    async fn send_interested(&mut self) -> anyhow::Result<()> {
        let mut buf = [0u8; 5];
        buf[..4].copy_from_slice(&1u32.to_be_bytes());
        buf[4] = 2; // message ID = interested
        self.stream.write_all(&buf).await?;
        Ok(())
    }

}


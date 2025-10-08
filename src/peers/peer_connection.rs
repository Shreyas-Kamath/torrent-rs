use crate::peers::peer::Peer;
use crate::pieces::piece_manager::PieceManager;
use std::sync::Arc;
use anyhow::Ok;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

const BLOCK_SIZE: usize = 16384;
const MAX_OUTGOING: u8 = 255;

pub struct PeerConnection {
    peer: Peer,
    stream: tokio::net::TcpStream,
    bitfield: Vec<bool>,
    info_hash: Arc<Vec<u8>>,
    peer_id: String,

    // buffers
    length_buf: [u8; 4],
    msg_buf: Vec<u8>,
    handshake_buf: [u8; 68],

    // states
    am_choked: bool,
    peer_choked: bool,
    am_interested: bool,

    in_flight: u8,
    piece_manager: Arc<Mutex<PieceManager>>,
}

impl PeerConnection {
    pub async fn new(peer_addr: std::net::SocketAddr, info_hash: Arc<Vec<u8>>, pm: Arc<Mutex<PieceManager>>) -> anyhow::Result<Self> {
        let peer = Peer::new(peer_addr);
        let stream = tokio::net::TcpStream::connect(peer_addr).await?;
        let num_pieces = {
            pm.lock().await.num_pieces
        };
        let bitfield = vec![false; num_pieces];

        let peer_id: String = String::from("-TR1012-123456789012");

        Ok(PeerConnection {
            peer,
            stream,
            bitfield,
            info_hash,
            peer_id,
            length_buf: [0; 4],
            msg_buf: Vec::new(),
            handshake_buf: [0; 68],
            am_choked: true,
            peer_choked: true,
            am_interested: false,
            in_flight: 0,
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
                // println!("{} choked us", self.peer.addr);
            }

            1 => {
                self.am_choked = false;
                // println!("{} unchoked us", self.peer.addr);
                if self.am_interested { self.maybe_request_next().await?; }
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
                // println!("{} has a piece", self.peer.addr);
                self.handle_have().await?;
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
                self.handle_piece().await?;
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

    async fn handle_have(&mut self) -> anyhow::Result<()> {
        if self.msg_buf.len() < 5 { return Ok (()); }

        let piece_index_network_order: [u8; 4] = self.msg_buf[1..5]
        .try_into()?;

        let piece_index = u32::from_be_bytes(piece_index_network_order) as usize;
        // println!("{} has piece: {}", self.peer.addr, piece_index);

        self.bitfield[piece_index] = true;

        self.maybe_request_next().await?;
        Ok(())
    }

    async fn handle_piece(&mut self) -> anyhow::Result<()> {
        if self.msg_buf.len() < 9 { return Ok(()) }

        self.in_flight -= 1;
        let piece_index = u32::from_be_bytes(self.msg_buf[1..5].try_into()?) as usize;
        let begin = u32::from_be_bytes(self.msg_buf[5..9].try_into()?) as usize;
        let block_data = &self.msg_buf[9..];

        // println!("Received piece {piece_index}, begin: {begin}, length: {}", block_data.len());
        let _ = {
            let mut pm = self.piece_manager.lock().await;
            pm.add_block(piece_index, begin, block_data)?;
        };

        // self.maybe_request_next().await?;
        Ok(())
    }

    async fn handle_bitfield(&mut self) -> anyhow::Result<()> {
        // Scoped lock
        let has_piece_we_need = {
            let pm = self.piece_manager.lock().await;

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

        self.maybe_request_next().await?;
        Ok(())
    }

    async fn send_interested(&mut self) -> anyhow::Result<()> {
        let mut buf = [0u8; 5];
        buf[..4].copy_from_slice(&1u32.to_be_bytes());
        buf[4] = 2; // message ID = interested
        self.stream.write_all(&buf).await?;
        Ok(())
    }

    async fn maybe_request_next(&mut self) -> anyhow::Result<()> {
       loop {
            if self.am_choked {
               return Ok(());
            }

            let next_block = {
                let mut pm = self.piece_manager.lock().await;
                pm.next_block(&self.bitfield)?
            };

            let (piece_index, begin, curr_len) = match next_block {
                Some(b) => b,
                None => return Ok(()),
            };

            let req_len = (curr_len - begin).min(BLOCK_SIZE);

            self.send_request(piece_index, begin, req_len).await?;
            // self.in_flight += 1;
        }

    }

    async fn send_request(&mut self, piece_index: usize, begin: usize, length: usize) -> anyhow::Result<()> {

        let mut buf = Vec::with_capacity(17);
        buf.extend_from_slice(&(13u32.to_be_bytes()));   // length prefix
        buf.push(6u8);                                   // message ID (request)
        buf.extend_from_slice(&(piece_index as u32).to_be_bytes()); 
        buf.extend_from_slice(&(begin as u32).to_be_bytes()); 
        buf.extend_from_slice(&(length as u32).to_be_bytes()); 
        
        self.stream.write_all(&buf).await?;
        Ok(())
    }

}


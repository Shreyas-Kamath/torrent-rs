use sha1::{self, Digest};

use crate::pieces::file_manager::FileManager;

pub struct Piece {
    index: usize,
    data: Vec<u8>,
    block_received: Vec<bool>,
    block_requested: Vec<bool>,
    bytes_written: u32,
    is_complete: bool,
}

impl Piece {
    fn maybe_init(&mut self, piece_len: usize, block_size: usize) {
        if self.data.is_empty() {
            let num_blocks = (piece_len + block_size - 1) / block_size;
            self.data = vec![0u8; piece_len];
            self.block_requested = vec![false; num_blocks];
            self.block_received = vec![false; num_blocks];
        }
    }
}

const BLOCK_SIZE: usize = 16384;

pub struct PieceManager {
    pub num_pieces: usize,
    pub piece_length: usize,
    pub total_length: usize,

    piece_hashes: Vec<[u8; 20]>,
    pieces: Vec<Piece>,

    tx: tokio::sync::mpsc::Sender<(usize, Vec<u8>)>,
}

impl PieceManager {
    pub fn new(piece_length: usize, total_length: usize, piece_hashes: &[u8], fm: FileManager) -> Self {
        // split hashes into 20-byte arrays
        let hashes: Vec<[u8; 20]> = piece_hashes
            .chunks(20)
            .map(|chunk| {
                let mut arr = [0u8; 20];
                arr.copy_from_slice(chunk);
                arr
            })
            .collect();

        let num_pieces = hashes.len();

        // initialize each piece
        let pieces_vec: Vec<Piece> = (0..num_pieces)
            .map(|i| Piece {
                index: i,
                data: Vec::new(),
                block_received: Vec::new(),
                block_requested: Vec::new(),
                bytes_written: 0,
                is_complete: false,
            })
            .collect();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(usize, Vec<u8>)>(200);

        tokio::spawn(async move {
            let fm = fm;
            while let Some((piece_index, data)) = rx.recv().await {
                if let Err(e) = fm.write_piece(piece_index, &data, piece_length) {
                    eprintln!("Error writing piece {}: {:?}", piece_index, e);
                }
            }
        });

        Self {
            num_pieces,
            piece_length,
            total_length,
            piece_hashes: hashes,
            pieces: pieces_vec,
            tx: tx,
        }
    }

    pub fn peer_has_piece_we_dont(&self, peer_bitfield: &[bool]) -> bool {
        for (index, piece) in self.pieces.iter().enumerate() {
            if !piece.is_complete && peer_bitfield[index] { return true; }
        }
        false
    }

    pub fn next_piece(&mut self, bitfield: &[bool]) -> anyhow::Result<Option<(usize, usize)>> {
        for id in 0..self.pieces.len() {
            if !self.pieces[id].is_complete {
                let curr_len = self.piece_length_of_index(id);
                self.pieces[id].maybe_init(curr_len, BLOCK_SIZE);
                
                if bitfield[id] && self.pieces[id].block_requested.iter().all(|b| !(*b)) {
                    self.pieces[id].block_requested.fill(true);
                    return Ok(Some((id, curr_len)));
                }
            }
        }
        Ok(None)
    }

    pub fn piece_length_of_index(&self, index: usize) -> usize {
        if index < self.num_pieces - 1 { self.piece_length }
        else { self.total_length - self.piece_length * (self.num_pieces - 1) }
    }

    pub fn add_block(&mut self, piece_index: usize, begin: usize, block_data: &[u8]) -> anyhow::Result<()> {
        let piece = &mut self.pieces[piece_index];

        let block_index = begin / BLOCK_SIZE;
        if !piece.block_received[block_index] {
            piece.data[begin..begin + block_data.len()].copy_from_slice(block_data);
            piece.block_received[block_index] = true;
        }

        let all_received = piece.block_received.iter().all(|&b| b);
        if all_received && !piece.is_complete {
            let hash_matches = {
                let data_slice = &piece.data;
                let computed: [u8; 20] = sha1::Sha1::digest(data_slice).into();
                computed == self.piece_hashes[piece_index]
            };

            if hash_matches {
                piece.is_complete = true;
                let data = std::mem::take(&mut piece.data);

                let tx = self.tx.clone();

                println!("{piece_index} received");
                tokio::spawn(async move {
                    if let Err(e) = tx.send((piece_index, data)).await {
                        eprintln!("Failed to send {} to disk: {:?}", piece_index, e);
                    }
                });
                piece.block_received.clear();
                piece.block_received.shrink_to_fit();
                piece.block_requested.clear();
                piece.block_requested.shrink_to_fit();
            } else {
                // reset the blocks so they can be requested again
                piece.block_received.fill(false);
                piece.block_requested.fill(false);
                piece.is_complete = false;
                println!("Piece {piece_index} failed hash, will retry.");
            }
        }

        Ok(())
    }

}
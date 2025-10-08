use sha1::{self, Digest};

use crate::pieces::file_manager::FileManager;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BlockState {
    NotRequested,
    Requested,
    Received,
}

pub struct Piece {
    index: usize,
    data: Vec<u8>,
    block_status: Vec<BlockState>,
    bytes_written: u32,
    is_complete: bool,
}

impl Piece {
    fn maybe_init(&mut self, piece_len: usize, block_size: usize) {
        if self.data.is_empty() {
            let num_blocks = (piece_len + block_size - 1) / block_size;
            self.data = vec![0u8; piece_len];
            self.block_status = vec![BlockState::NotRequested; num_blocks];
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
                block_status: Vec::new(),
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

    pub fn next_block(&mut self, bitfield: &[bool]) -> anyhow::Result<Option<(usize, usize, usize)>> {
        for id in 0..self.pieces.len() {
            if !self.pieces[id].is_complete && bitfield[id] {
                let curr_len = self.piece_length_of_index(id);
                self.pieces[id].maybe_init(curr_len, BLOCK_SIZE);
                
                for (block_index, state) in self.pieces[id].block_status.iter_mut().enumerate() {
                    if *state == BlockState::NotRequested {
                        *state = BlockState::Requested; 
                        let offset = block_index * BLOCK_SIZE;
                        return Ok(Some((id, offset, curr_len)));
                    }
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
        piece.data[begin..begin + block_data.len()].copy_from_slice(block_data);
        piece.block_status[block_index] = BlockState::Received;

        let all_received = piece.block_status.iter().all(|b| *b == BlockState::Received);
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
                piece.block_status.clear();
                piece.block_status.shrink_to_fit();
            } else {
                // reset the blocks so they can be requested again
                piece.block_status.fill(BlockState::NotRequested);
                piece.is_complete = false;
                println!("Piece {piece_index} failed hash, will retry.");
            }
        }

        Ok(())
    }

}
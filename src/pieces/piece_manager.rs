use tokio::sync::Mutex;

pub struct Piece {
    index: usize,
    data: Vec<u8>,
    block_received: Vec<bool>,
    block_requested: Vec<bool>,
    bytes_written: u32,
    is_complete: bool,
}

pub struct PieceManager {
    pub num_pieces: usize,
    pub piece_length: usize,
    pub total_length: usize,

    piece_hashes: Vec<[u8; 20]>,
    pieces: Vec<Piece>,
}

impl PieceManager {
    pub fn new(piece_length: usize, total_length: usize, piece_hashes: &[u8]) -> Self {
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

        Self {
            num_pieces,
            piece_length,
            total_length,
            piece_hashes: hashes,
            pieces: pieces_vec,
        }
    }

    pub fn peer_has_piece_we_dont(&self, peer_bitfield: &[bool]) -> bool {
        for (index, piece) in self.pieces.iter().enumerate() {
            if !piece.is_complete && peer_bitfield[index] { return true; }
        }
        false
    }
}
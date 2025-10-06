use std::fs::{self, OpenOptions};
use std::io::{SeekFrom, Seek, Write};
use std::path::Path;

pub struct OutputFile {
    pub path: String,
    pub start: usize,   // start offset in the torrent
    pub length: usize,
}

pub struct FileManager {
    pub files: Vec<OutputFile>,
    pub total_length: usize,
}

impl FileManager {
    pub fn new(info: &crate::torrent::Info) -> anyhow::Result<Self> {
        let mut files = Vec::new();
        let mut offset = 0;

        if let Some(torrent_files) = &info.files {
            for f in torrent_files {
                let path = f.path.join("/"); // join the Vec<String> path
                let parent = Path::new(&path).parent();
                if let Some(p) = parent {
                    fs::create_dir_all(p)?;
                }

                // Ensure file exists
                if !Path::new(&path).exists() {
                    OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(&path)?;
                }

                files.push(OutputFile {
                    path,
                    start: offset,
                    length: f.length as usize,
                });

                offset += f.length as usize;
            }
        } else {
            // single-file torrent
            let path = info.name.clone();
            if let Some(p) = Path::new(&path).parent() {
                fs::create_dir_all(p)?;
            }

            if !Path::new(&path).exists() {
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&path)?;
            }

            files.push(OutputFile {
                path,
                start: 0,
                length: info.length.unwrap_or(0) as usize,
            });

            offset = info.length.unwrap_or(0) as usize;
        }

        Ok(Self {
            files,
            total_length: offset,
        })
    }

    pub fn write_piece(&self, piece_index: usize, data: &[u8], piece_length: usize) -> anyhow::Result<()> {
        let mut remaining = data;
        let mut global_offset = piece_index * piece_length;

        for file_info in &self.files {
            if global_offset >= file_info.length {
                global_offset -= file_info.length;
                continue;
            }

            let write_len = remaining.len().min(file_info.length - global_offset);

            let mut file = OpenOptions::new()
                .write(true)
                .open(&file_info.path)?;

            file.seek(SeekFrom::Start(global_offset as u64))?;
            file.write_all(&remaining[..write_len])?;
            // println!("Piece {piece_index} written");

            remaining = &remaining[write_len..];
            global_offset = 0;

            if remaining.is_empty() {
                break;
            }
        }

        Ok(())
    }
}

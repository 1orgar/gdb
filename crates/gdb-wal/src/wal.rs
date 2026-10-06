use crc32fast::Hasher;
use gdb_core::{GdbError, GdbResult};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const WAL_MAGIC: &[u8; 4] = b"GWAL";

/// High-throughput Append-Only Write-Ahead Log with CRC32 checksums.
pub struct WriteAheadLog {
    path: PathBuf,
    writer: Mutex<BufWriter<File>>,
}

impl WriteAheadLog {
    pub fn open(path: impl AsRef<Path>) -> GdbResult<Self> {
        let path = path.as_ref().to_path_buf();
        let file_exists = path.exists();

        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|e| GdbError::Storage(format!("Failed to open WAL: {}", e)))?;

        let mut writer = BufWriter::new(file);

        if !file_exists || path.metadata().map(|m| m.len()).unwrap_or(0) == 0 {
            writer.write_all(WAL_MAGIC).map_err(|e| GdbError::Storage(e.to_string()))?;
            writer.flush().map_err(|e| GdbError::Storage(e.to_string()))?;
        }

        Ok(Self {
            path,
            writer: Mutex::new(writer),
        })
    }

    /// Appends a Raft log entry / mutation payload to the WAL file.
    pub fn append(&self, index: u64, payload: &[u8]) -> GdbResult<()> {
        let mut hasher = Hasher::new();
        hasher.update(&index.to_le_bytes());
        hasher.update(&(payload.len() as u32).to_le_bytes());
        hasher.update(payload);
        let checksum = hasher.finalize();

        let mut writer = self.writer.lock().unwrap();
        writer.write_all(&index.to_le_bytes()).map_err(|e| GdbError::Storage(e.to_string()))?;
        writer.write_all(&(payload.len() as u32).to_le_bytes()).map_err(|e| GdbError::Storage(e.to_string()))?;
        writer.write_all(payload).map_err(|e| GdbError::Storage(e.to_string()))?;
        writer.write_all(&checksum.to_le_bytes()).map_err(|e| GdbError::Storage(e.to_string()))?;
        writer.flush().map_err(|e| GdbError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Replays and verifies all committed records from the WAL.
    pub fn replay(&self) -> GdbResult<Vec<(u64, Vec<u8>)>> {
        let file = File::open(&self.path).map_err(|e| GdbError::Storage(e.to_string()))?;
        let mut reader = BufReader::new(file);

        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic).map_err(|e| GdbError::Storage(format!("Invalid WAL header: {}", e)))?;
        if &magic != WAL_MAGIC {
            return Err(GdbError::Storage("Corrupt WAL magic bytes".into()));
        }

        let mut records = Vec::new();
        loop {
            let mut index_buf = [0u8; 8];
            if let Err(e) = reader.read_exact(&mut index_buf) {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    break;
                }
                return Err(GdbError::Storage(format!("WAL read index error: {}", e)));
            }
            let index = u64::from_le_bytes(index_buf);

            let mut len_buf = [0u8; 4];
            reader.read_exact(&mut len_buf).map_err(|e| GdbError::Storage(e.to_string()))?;
            let len = u32::from_le_bytes(len_buf) as usize;

            let mut payload = vec![0u8; len];
            reader.read_exact(&mut payload).map_err(|e| GdbError::Storage(e.to_string()))?;

            let mut checksum_buf = [0u8; 4];
            reader.read_exact(&mut checksum_buf).map_err(|e| GdbError::Storage(e.to_string()))?;
            let expected_checksum = u32::from_le_bytes(checksum_buf);

            let mut hasher = Hasher::new();
            hasher.update(&index.to_le_bytes());
            hasher.update(&(len as u32).to_le_bytes());
            hasher.update(&payload);
            let actual_checksum = hasher.finalize();

            if actual_checksum != expected_checksum {
                return Err(GdbError::Storage(format!("WAL checksum mismatch at index {}", index)));
            }

            records.push((index, payload));
        }

        Ok(records)
    }

    /// Truncates the WAL after snapshot is taken.
    pub fn truncate_to_empty(&self) -> GdbResult<()> {
        let mut writer = self.writer.lock().unwrap();
        let file = writer.get_mut();
        file.set_len(4).map_err(|e| GdbError::Storage(e.to_string()))?;
        file.seek(SeekFrom::Start(4)).map_err(|e| GdbError::Storage(e.to_string()))?;
        Ok(())
    }
}

use std::collections::HashMap;
use std::fs::{self,File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

/// Represents the type of operation performed on the storage engine.
/// We use #[repr(u8)] to force Rust to store this enum as a single raw byte (0x01 or 0x02).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum OpCode {
    Set = 0x01,
    Delete = 0x02,
}

impl From<u8> for OpCode {
    fn from(v: u8) -> Self {
        match v {
            0x01 => OpCode::Set,
            0x02 => OpCode::Delete,
            _ => panic!("Unknown OpCode found in WAL binary log: {:#X}", v),
        }
    }
}

/// The core embedded Key-Value Storage Engine.
pub struct StorageEngine {
    /// In-memory hash map for ultra-fast O(1) lookups.
    memtable: HashMap<String, String>,
    /// Active file handle for the Write-Ahead Log (WAL) on disk.
    wal_file: File,
}

impl StorageEngine {
    /// Initializes the storage engine. 
    /// If an existing log file is found, it automatically triggers crash recovery.
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path_ref = path.as_ref();
        let mut memtable = HashMap::new();

        // 1. Crash Recovery phase: Rebuild memory state if log already exists
        if path_ref.exists() {
            println!("[StorageEngine] Existing WAL detected. Replaying log for recovery...");
            memtable = Self::recover_from_wal(path_ref)?;
        }

        // 2. Open file in append-only mode with read capabilities
        let wal_file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path_ref)?;

        Ok(StorageEngine { memtable, wal_file })
    }

    /// Inserts or updates a key-value pair with durability guarantees.
    pub fn put(&mut self, key: String, value: String) -> io::Result<()> {
        // Step 1: Serialize the command into a raw binary frame
        let mut frame_bytes = Vec::new();
        
        // Push OpCode (1 byte)
        frame_bytes.push(OpCode::Set as u8);
        
        // Push Key length & Value length as 4-byte little-endian integers
        frame_bytes.extend_from_slice(&(key.len() as u32).to_le_bytes());
        frame_bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
        
        // Push actual string contents as raw UTF-8 bytes
        frame_bytes.extend_from_slice(key.as_bytes());
        frame_bytes.extend_from_slice(value.as_bytes());

        // Step 2: Write bytes to disk log and force a physical hardware sync (fsync)
        self.wal_file.write_all(&frame_bytes)?;
        self.wal_file.sync_all()?;

        // Step 3: Update the in-memory map so future GETs are instant
        self.memtable.insert(key, value);
        Ok(())
    }

    /// Deletes a key by logging a deletion frame and updating memory.
    pub fn remove(&mut self, key: &str) -> io::Result<bool> {
        let mut frame_bytes = Vec::new();
        
        frame_bytes.push(OpCode::Delete as u8);
        frame_bytes.extend_from_slice(&(key.len() as u32).to_le_bytes());
        frame_bytes.extend_from_slice(&0u32.to_le_bytes()); // 0 length for value
        frame_bytes.extend_from_slice(key.as_bytes());

        self.wal_file.write_all(&frame_bytes)?;
        self.wal_file.sync_all()?;

        let existed = self.memtable.remove(key).is_some();
        Ok(existed)
    }

    /// Looks up a key directly from the fast in-memory map.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.memtable.get(key)
    }

    /// Reads the raw binary log file byte-by-byte to reconstruct the database state after a crash.
    fn recover_from_wal(path: &Path) -> io::Result<HashMap<String, String>> {
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        let mut map = HashMap::new();
        let mut cursor = 0;
        let total_bytes = buffer.len();
        let mut recovered_count = 0;

        // Traverse the byte buffer sequentially
        while cursor < total_bytes {
            // A minimal frame header requires at least 1 (opcode) + 4 (k_len) + 4 (v_len) = 9 bytes
            if cursor + 9 > total_bytes {
                break; // Stop if we hit corrupted or incomplete trailing bytes from a sudden crash
            }

            // 1. Read OpCode
            let op = OpCode::from(buffer[cursor]);
            cursor += 1;

            // 2. Read Key Length (4 bytes)
            let k_len = u32::from_le_bytes(buffer[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            // 3. Read Value Length (4 bytes)
            let v_len = u32::from_le_bytes(buffer[cursor..cursor + 4].try_into().unwrap()) as usize;
            cursor += 4;

            // Ensure buffer has enough data remaining for the payloads
            if cursor + k_len + v_len > total_bytes {
                break;
            }

            // 4. Extract Key string
            let key = String::from_utf8_lossy(&buffer[cursor..cursor + k_len]).into_owned();
            cursor += k_len;

            // 5. Extract Value string
            let value = String::from_utf8_lossy(&buffer[cursor..cursor + v_len]).into_owned();
            cursor += v_len;

            // Apply operation to our restored state
            match op {
                OpCode::Set => {
                    map.insert(key, value);
                }
                OpCode::Delete => {
                    map.remove(&key);
                }
            }
            recovered_count += 1;
        }

        println!("[StorageEngine] Recovery complete. Restored {} entries.", recovered_count);
        Ok(map)
    }
}

fn main() -> io::Result<()> {
    let db_file = "wal_storage.log";

    // Clean up previous log for testing purposes
    let _ = fs::remove_file(db_file);

    println!("=== SIMULATION START ===");
    
    // Block 1: Write operations and abrupt closure simulation
    {
        println!("\n[Phase 1] Opening engine and writing data...");
        let mut engine = StorageEngine::new(db_file)?;
        
        engine.put("sys:hostname".to_string(), "node-alpha-01".to_string())?;
        engine.put("sys:status".to_string(), "active".to_string())?;
        engine.remove("sys:status")?; // Delete test
        
        println!("Data written. Closing engine scope (Simulating process exit).");
    } // Engine variable goes out of scope here (files close)

    // Block 2: Restart engine and verify state recovery from disk
    {
        println!("\n[Phase 2] Restarting engine to verify crash recovery...");
        let engine = StorageEngine::new(db_file)?;

        if let Some(val) = engine.get("sys:hostname") {
            println!("GET 'sys:hostname' -> {} [SUCCESS]", val);
        }

        if engine.get("sys:status").is_none() {
            println!("GET 'sys:status' -> NOT FOUND [SUCCESS - Deletion persisted across restarts]");
        }
    }

    println!("\n=== SIMULATION COMPLETE ===");
    Ok(())
}
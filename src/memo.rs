//! Memo-file support for fastdbf.
//!
//! DBF files that use Memo, General, or Picture fields store the actual
//! content in a companion file:
//!
//! * **DBase III / Clipper** – `<stem>.dbt`, fixed 512-byte blocks.
//! * **FoxPro 2 / dBase IV** – `<stem>.dbt`, fixed 512-byte blocks
//!   (FoxPro variant with 4-byte block length prefix).
//! * **Visual FoxPro** – `<stem>.fpt`, variable-length records in
//!   512-byte blocks with a 8-byte record header.
//!
//! The public API is deliberately simple:
//!
//! ```rust,ignore
//! let memo = MemoFile::open_alongside(&dbf_path, dbf_kind)?;
//! let text = memo.read(block_pointer)?;
//! ```

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::header::DbfKind;

/// How many bytes per block this memo file uses.
const BLOCK_SIZE: u64 = 512;

/// Memo-file format discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoFormat {
    /// dBase III / Clipper: `\x1A\x1A` terminator, no length prefix.
    DBase3,
    /// dBase IV / FoxPro 2: 4-byte LE length prefix, then content.
    DBase4,
    /// Visual FoxPro: 8-byte record header (type u32 + length u32), then content.
    VisualFoxPro,
}

impl MemoFormat {
    pub fn for_kind(kind: DbfKind) -> Self {
        match kind {
            DbfKind::DBase3 | DbfKind::DBase3WithMemo => Self::DBase3,
            DbfKind::FoxPro2WithMemo | DbfKind::DBase4WithMemo => Self::DBase4,
            DbfKind::VisualFoxPro
            | DbfKind::VisualFoxProAutoIncrement
            | DbfKind::VisualFoxProVar => Self::VisualFoxPro,
        }
    }

    /// File extension for this format.
    pub fn extension(self) -> &'static str {
        match self {
            Self::DBase3 | Self::DBase4 => "dbt",
            Self::VisualFoxPro => "fpt",
        }
    }
}

#[derive(Debug)]
pub struct MemoFile {
    file: File,
    format: MemoFormat,
    /// Next free block (read from the file header).
    next_free_block: u32,
}

impl MemoFile {
    // ── Opening ──────────────────────────────────────────────────────

    /// Open an existing memo file that lives alongside a DBF file.
    /// Tries both `.dbt` and `.fpt` extensions (case-insensitive on
    /// platforms that support it) so callers don't have to guess.
    pub fn open_alongside(dbf_path: &Path, kind: DbfKind) -> Result<Option<Self>> {
        let format = MemoFormat::for_kind(kind);
        if let Some(path) = companion_path(dbf_path, format.extension()) {
            let file = File::open(&path)?;
            return Ok(Some(Self::from_file(file, format)?));
        }
        // Try the other extension as a fallback.
        let alt_ext = match format.extension() {
            "dbt" => "fpt",
            _ => "dbt",
        };
        if let Some(path) = companion_path(dbf_path, alt_ext) {
            let alt_format = if alt_ext == "fpt" {
                MemoFormat::VisualFoxPro
            } else {
                MemoFormat::DBase3
            };
            let file = File::open(&path)?;
            return Ok(Some(Self::from_file(file, alt_format)?));
        }
        Ok(None)
    }

    /// Create a new empty memo file alongside `dbf_path`.
    pub fn create_alongside(dbf_path: &Path, kind: DbfKind) -> Result<Self> {
        let format = MemoFormat::for_kind(kind);
        let ext = format.extension();
        let stem = dbf_path.file_stem().unwrap_or_default();
        let path = dbf_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(stem)
            .with_extension(ext);
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        // Write an empty header block (512 bytes).
        let mut header = vec![0u8; BLOCK_SIZE as usize];
        // Next free block starts at block 1 (block 0 = header).
        match format {
            MemoFormat::DBase3 | MemoFormat::DBase4 => {
                header[0..4].copy_from_slice(&1u32.to_le_bytes());
            }
            MemoFormat::VisualFoxPro => {
                header[0..4].copy_from_slice(&1u32.to_be_bytes()); // VFP uses BE here
                header[6..8].copy_from_slice(&(BLOCK_SIZE as u16).to_be_bytes());
            }
        }
        file.write_all(&header)?;
        file.flush()?;
        Ok(Self {
            file,
            format,
            next_free_block: 1,
        })
    }

    fn from_file(mut file: File, format: MemoFormat) -> Result<Self> {
        let mut header = [0u8; 4];
        file.read_exact(&mut header)?;
        let next_free_block = match format {
            MemoFormat::DBase3 | MemoFormat::DBase4 => u32::from_le_bytes(header),
            MemoFormat::VisualFoxPro => u32::from_be_bytes(header),
        };
        Ok(Self {
            file,
            format,
            next_free_block,
        })
    }

    // ── Reading ──────────────────────────────────────────────────────

    /// Read the memo record at `block_pointer` (1-based block index as
    /// stored in the DBF record field).  Returns raw bytes – callers
    /// should apply the appropriate text encoding themselves.
    pub fn read(&mut self, block_pointer: u32) -> Result<Vec<u8>> {
        if block_pointer == 0 {
            return Ok(Vec::new());
        }
        let offset = block_pointer as u64 * BLOCK_SIZE;
        self.file.seek(SeekFrom::Start(offset))?;

        match self.format {
            MemoFormat::DBase3 => self.read_dbase3(),
            MemoFormat::DBase4 => self.read_dbase4(),
            MemoFormat::VisualFoxPro => self.read_vfp(),
        }
    }

    fn read_dbase3(&mut self) -> Result<Vec<u8>> {
        // DBase III memos are terminated by two consecutive 0x1A bytes.
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        let mut last = 0u8;
        loop {
            match self.file.read_exact(&mut byte) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(err) => return Err(err.into()),
            }
            if last == 0x1A && byte[0] == 0x1A {
                buf.pop(); // remove the first 0x1A
                break;
            }
            buf.push(byte[0]);
            last = byte[0];
        }
        Ok(buf)
    }

    fn read_dbase4(&mut self) -> Result<Vec<u8>> {
        // dBase IV / FoxPro 2: 4-byte LE length prefix.
        let mut len_bytes = [0u8; 4];
        self.file.read_exact(&mut len_bytes)?;
        let length = u32::from_le_bytes(len_bytes) as usize;
        let mut buf = vec![0u8; length];
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn read_vfp(&mut self) -> Result<Vec<u8>> {
        // VFP: 4-byte record type (1 = memo, 2 = picture/object) + 4-byte BE length.
        let mut hdr = [0u8; 8];
        self.file.read_exact(&mut hdr)?;
        // record_type = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
        let length = u32::from_be_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
        let mut buf = vec![0u8; length];
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    // ── Writing ──────────────────────────────────────────────────────

    /// Append a new memo record and return the block pointer to store
    /// in the DBF field.
    pub fn append(&mut self, content: &[u8]) -> Result<u32> {
        let block = self.next_free_block;
        let offset = block as u64 * BLOCK_SIZE;
        self.file.seek(SeekFrom::Start(offset))?;

        match self.format {
            MemoFormat::DBase3 => {
                self.file.write_all(content)?;
                // Terminate with two 0x1A bytes.
                self.file.write_all(&[0x1A, 0x1A])?;
                // Pad to next block boundary.
                let total = content.len() + 2;
                let remainder = BLOCK_SIZE as usize - (total % BLOCK_SIZE as usize);
                if remainder < BLOCK_SIZE as usize {
                    self.file.write_all(&vec![0u8; remainder])?;
                }
                let blocks_used = (content.len() + 2).div_ceil(BLOCK_SIZE as usize) as u32;
                self.next_free_block += blocks_used;
            }
            MemoFormat::DBase4 => {
                let len = content.len() as u32;
                self.file.write_all(&len.to_le_bytes())?;
                self.file.write_all(content)?;
                let total = 4 + content.len();
                let remainder = BLOCK_SIZE as usize - (total % BLOCK_SIZE as usize);
                if remainder < BLOCK_SIZE as usize {
                    self.file.write_all(&vec![0u8; remainder])?;
                }
                let blocks_used = total.div_ceil(BLOCK_SIZE as usize) as u32;
                self.next_free_block += blocks_used;
            }
            MemoFormat::VisualFoxPro => {
                let record_type: u32 = 1; // 1 = memo text
                let length = content.len() as u32;
                self.file.write_all(&record_type.to_be_bytes())?;
                self.file.write_all(&length.to_be_bytes())?;
                self.file.write_all(content)?;
                let total = 8 + content.len();
                let remainder = BLOCK_SIZE as usize - (total % BLOCK_SIZE as usize);
                if remainder < BLOCK_SIZE as usize {
                    self.file.write_all(&vec![0u8; remainder])?;
                }
                let blocks_used = total.div_ceil(BLOCK_SIZE as usize) as u32;
                self.next_free_block += blocks_used;
            }
        }

        // Update the next-free-block in the header.
        self.file.seek(SeekFrom::Start(0))?;
        match self.format {
            MemoFormat::DBase3 | MemoFormat::DBase4 => {
                self.file.write_all(&self.next_free_block.to_le_bytes())?;
            }
            MemoFormat::VisualFoxPro => {
                self.file.write_all(&self.next_free_block.to_be_bytes())?;
            }
        }
        self.file.flush()?;
        Ok(block)
    }

    pub fn next_free_block(&self) -> u32 {
        self.next_free_block
    }
}

// ── Path helpers ─────────────────────────────────────────────────────────────

/// Build the companion path (`.dbt` / `.fpt`) from a `.dbf` path.
/// Tries exact case first, then uppercase, then lowercase.
fn companion_path(dbf_path: &Path, ext: &str) -> Option<PathBuf> {
    let stem = dbf_path.file_stem()?;
    let parent = dbf_path.parent().unwrap_or(Path::new("."));

    // Try lower, UPPER, Title for the extension – callers on
    // case-sensitive file systems (Linux) may have any capitalisation.
    for candidate_ext in [ext, &ext.to_ascii_uppercase(), &title_case(ext)] {
        let candidate = parent.join(stem).with_extension(candidate_ext);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_stem() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        temp_dir().join(format!("fastdbf-memo-test-{nanos}"))
    }

    #[test]
    fn vfp_round_trip() {
        let dbf_path = tmp_stem().with_extension("dbf");
        // Create an empty DBF so companion_path has a directory reference.
        std::fs::write(&dbf_path, b"").unwrap();

        let mut memo = MemoFile::create_alongside(&dbf_path, DbfKind::VisualFoxPro).unwrap();
        let content = b"Hello, VFP memo world!";
        let ptr = memo.append(content).unwrap();
        assert_eq!(ptr, 1);

        let read_back = memo.read(ptr).unwrap();
        assert_eq!(read_back, content);

        let _ = std::fs::remove_file(&dbf_path);
        let _ = std::fs::remove_file(dbf_path.with_extension("fpt"));
    }

    #[test]
    fn dbase3_round_trip() {
        let dbf_path = tmp_stem().with_extension("dbf");
        std::fs::write(&dbf_path, b"").unwrap();

        let mut memo = MemoFile::create_alongside(&dbf_path, DbfKind::DBase3WithMemo).unwrap();
        let content = b"DBase III memo test.";
        let ptr = memo.append(content).unwrap();
        let read_back = memo.read(ptr).unwrap();
        assert_eq!(read_back, content);

        let _ = std::fs::remove_file(&dbf_path);
        let _ = std::fs::remove_file(dbf_path.with_extension("dbt"));
    }

    #[test]
    fn multi_memo_sequential_blocks() {
        let dbf_path = tmp_stem().with_extension("dbf");
        std::fs::write(&dbf_path, b"").unwrap();

        let mut memo = MemoFile::create_alongside(&dbf_path, DbfKind::VisualFoxPro).unwrap();
        let ptr1 = memo.append(b"First memo").unwrap();
        let ptr2 = memo.append(b"Second memo, a bit longer").unwrap();

        assert!(ptr2 > ptr1);
        assert_eq!(memo.read(ptr1).unwrap(), b"First memo");
        assert_eq!(memo.read(ptr2).unwrap(), b"Second memo, a bit longer");

        let _ = std::fs::remove_file(&dbf_path);
        let _ = std::fs::remove_file(dbf_path.with_extension("fpt"));
    }
}

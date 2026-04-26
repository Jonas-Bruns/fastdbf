// ── lib.rs (updated) ──────────────────────────────────────────────────────────

mod codepage; // NEW
mod error;
mod header;
mod memo; // NEW
mod python_api;
mod record;
mod spec;
mod table;
mod value;

pub use codepage::{decode_bytes, encode_str, encoding_for_mark, label_for_mark, mark_for_name};
pub use error::{Error, Result};
pub use header::{CodePageMark, DbfKind, FieldDescriptor, FieldType, Header};
pub use memo::{MemoFile, MemoFormat};
pub use record::Record;
pub use spec::{FieldName, FieldSpec};
pub use table::{Table, CLOSED, IN_MEMORY, ON_DISK, READ_ONLY, READ_WRITE};
pub use value::{Date, DateTime, Value};

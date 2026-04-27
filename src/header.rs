use crate::error::{Error, Result};
use crate::value::{Date, Value};

pub const FIELD_FLAG_NULLABLE: u8 = 0x02;
pub const FIELD_FLAG_BINARY: u8 = 0x04;

/// Visual FoxPro tables have 263 extra bytes after the header terminator
/// for the DBC (database container) backlink path. Non-VFP tables have 0.
pub const VFP_BACKLINK_SIZE: u16 = 263;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbfKind {
    DBase3,
    DBase3WithMemo,
    FoxPro2WithMemo,
    VisualFoxPro,
    VisualFoxProAutoIncrement,
    VisualFoxProVar,
    DBase4WithMemo,
}

impl DbfKind {
    pub fn from_version(version: u8) -> Result<Self> {
        match version {
            0x03 => Ok(Self::DBase3),
            0x83 => Ok(Self::DBase3WithMemo),
            0xF5 => Ok(Self::FoxPro2WithMemo),
            0x30 => Ok(Self::VisualFoxPro),
            0x31 => Ok(Self::VisualFoxProAutoIncrement),
            0x32 => Ok(Self::VisualFoxProVar),
            0x8B => Ok(Self::DBase4WithMemo),
            _ => Err(Error::Unsupported(format!(
                "unsupported DBF version byte: 0x{version:02X}"
            ))),
        }
    }

    pub const fn version_byte(self) -> u8 {
        match self {
            Self::DBase3 => 0x03,
            Self::DBase3WithMemo => 0x83,
            Self::FoxPro2WithMemo => 0xF5,
            Self::VisualFoxPro => 0x30,
            Self::VisualFoxProAutoIncrement => 0x31,
            Self::VisualFoxProVar => 0x32,
            Self::DBase4WithMemo => 0x8B,
        }
    }

    /// Returns `true` for Visual FoxPro table variants.
    pub const fn is_vfp(self) -> bool {
        matches!(
            self,
            Self::VisualFoxPro | Self::VisualFoxProAutoIncrement | Self::VisualFoxProVar
        )
    }

    /// Extra bytes written between the header terminator and the first record.
    pub const fn backlink_size(self) -> u16 {
        if self.is_vfp() { VFP_BACKLINK_SIZE } else { 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodePageMark(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    NullFlags,
    Character,
    Date,
    Logical,
    Memo,
    Numeric,
    Float,
    Integer,
    Double,
    DateTime,
    Currency,
    General,
    Picture,
}

impl FieldType {
    pub fn from_byte(byte: u8) -> Result<Self> {
        match byte {
            b'0' => Ok(Self::NullFlags),
            b'C' => Ok(Self::Character),
            b'D' => Ok(Self::Date),
            b'L' => Ok(Self::Logical),
            b'M' => Ok(Self::Memo),
            b'N' => Ok(Self::Numeric),
            b'F' => Ok(Self::Float),
            b'I' => Ok(Self::Integer),
            b'B' => Ok(Self::Double),
            b'T' | b'@' => Ok(Self::DateTime),
            b'Y' => Ok(Self::Currency),
            b'G' => Ok(Self::General),
            b'P' => Ok(Self::Picture),
            other => Err(Error::Unsupported(format!(
                "unsupported field type byte: 0x{other:02X}"
            ))),
        }
    }

    pub const fn symbol(self) -> u8 {
        match self {
            Self::NullFlags => b'0',
            Self::Character => b'C',
            Self::Date => b'D',
            Self::Logical => b'L',
            Self::Memo => b'M',
            Self::Numeric => b'N',
            Self::Float => b'F',
            Self::Integer => b'I',
            Self::Double => b'B',
            Self::DateTime => b'T',
            Self::Currency => b'Y',
            Self::General => b'G',
            Self::Picture => b'P',
        }
    }

    pub const fn default_value(self) -> Value {
        match self {
            Self::NullFlags => Value::Binary(Vec::new()),
            Self::Character => Value::Character(String::new()),
            Self::Date => Value::Date(None),
            Self::Logical => Value::Logical(None),
            Self::Memo | Self::General | Self::Picture => Value::Memo(Vec::new()),
            Self::Numeric | Self::Float => Value::Numeric(0.0),
            Self::Integer => Value::Integer(0),
            Self::Double => Value::Double(0.0),
            Self::DateTime => Value::DateTime(None),
            Self::Currency => Value::Currency(0),
        }
    }

    pub const fn fixed_length(self) -> Option<u8> {
        match self {
            Self::NullFlags => None,
            Self::Date => Some(8),
            Self::Logical => Some(1),
            Self::Integer => Some(4),
            Self::Double => Some(8),
            Self::DateTime => Some(8),
            Self::Currency => Some(8),
            Self::Memo | Self::General | Self::Picture => Some(10),
            _ => None,
        }
    }

    pub const fn requires_vfp(self) -> bool {
        matches!(
            self,
            Self::Integer | Self::Double | Self::DateTime | Self::Currency | Self::NullFlags
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDescriptor {
    pub name: String,
    pub field_type: FieldType,
    pub offset: u16,
    pub length: u8,
    pub decimals: u8,
    pub flags: u8,
    pub nullable_index: Option<usize>,
}

impl FieldDescriptor {
    pub fn empty_value(&self) -> Value {
        self.field_type.default_value()
    }

    pub fn is_nullable(&self) -> bool {
        self.flags & FIELD_FLAG_NULLABLE != 0
    }

    pub fn is_binary(&self) -> bool {
        self.flags & FIELD_FLAG_BINARY != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub kind: DbfKind,
    pub last_update: Option<Date>,
    pub record_count: u32,
    pub header_length: u16,
    pub record_length: u16,
    pub code_page: CodePageMark,
}

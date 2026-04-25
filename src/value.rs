use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Date {
    pub const fn new(year: u16, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }

    pub fn parse_ymd(raw: &str) -> Result<Option<Self>> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if trimmed.len() != 8 || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(Error::InvalidFormat(format!(
                "invalid DBF date payload: {raw:?}"
            )));
        }
        let year = trimmed[0..4]
            .parse::<u16>()
            .map_err(|_| Error::InvalidFormat(format!("invalid year in {raw:?}")))?;
        let month = trimmed[4..6]
            .parse::<u8>()
            .map_err(|_| Error::InvalidFormat(format!("invalid month in {raw:?}")))?;
        let day = trimmed[6..8]
            .parse::<u8>()
            .map_err(|_| Error::InvalidFormat(format!("invalid day in {raw:?}")))?;
        Ok(Some(Self { year, month, day }))
    }

    pub fn to_ymd_string(self) -> String {
        format!("{:04}{:02}{:02}", self.year, self.month, self.day)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateTime {
    pub julian_day: i32,
    pub millis_since_midnight: i32,
}

impl DateTime {
    pub const fn new(julian_day: i32, millis_since_midnight: i32) -> Self {
        Self {
            julian_day,
            millis_since_midnight,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Character(String),
    Numeric(f64),
    Logical(Option<bool>),
    Date(Option<Date>),
    Integer(i32),
    Double(f64),
    DateTime(Option<DateTime>),
    Currency(i64),
    MemoRef(u32),
    Binary(Vec<u8>),
}

impl Value {
    pub fn null_for_logical() -> Self {
        Self::Logical(None)
    }

    pub fn null_for_date() -> Self {
        Self::Date(None)
    }

    pub fn null_for_datetime() -> Self {
        Self::DateTime(None)
    }
}

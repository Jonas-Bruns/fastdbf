use crate::error::{Error, Result};
use crate::header::{FieldDescriptor, FieldType, FIELD_FLAG_NULLABLE};

pub type FieldName = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSpec {
    pub name: FieldName,
    pub field_type: FieldType,
    pub length: u8,
    pub decimals: u8,
    pub nullable: bool,
}

impl FieldSpec {
    pub fn parse_many(specs: &str) -> Result<Vec<Self>> {
        specs
            .split(';')
            .map(str::trim)
            .filter(|spec| !spec.is_empty())
            .map(Self::parse_one)
            .collect()
    }

    pub fn parse_one(raw: &str) -> Result<Self> {
        let mut parts = raw.split_whitespace();
        let name = parts
            .next()
            .ok_or_else(|| Error::InvalidFieldSpec(format!("missing field name in {raw:?}")))?;
        let type_part = parts
            .next()
            .ok_or_else(|| Error::InvalidFieldSpec(format!("missing field type in {raw:?}")))?;
        let mut nullable = false;
        for modifier in parts {
            match modifier.to_ascii_lowercase().as_str() {
                "null" | "nullable" => nullable = true,
                other => {
                    return Err(Error::InvalidFieldSpec(format!(
                        "unsupported field modifier {other:?} in {raw:?}"
                    )))
                }
            }
        }
        let name = normalize_name(name)?;
        let (field_type, length, decimals) = parse_type_part(type_part)?;
        Ok(Self {
            name,
            field_type,
            length,
            decimals,
            nullable,
        })
    }

    pub fn to_descriptor(&self, offset: u16) -> FieldDescriptor {
        FieldDescriptor {
            name: self.name.clone(),
            field_type: self.field_type,
            offset,
            length: self.length,
            decimals: self.decimals,
            flags: if self.nullable {
                FIELD_FLAG_NULLABLE
            } else {
                0
            },
            nullable_index: None,
        }
    }
}

fn normalize_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidFieldSpec(
            "field name cannot be empty".to_string(),
        ));
    }
    if trimmed.len() > 11 {
        return Err(Error::InvalidFieldSpec(format!(
            "field name {trimmed:?} exceeds DBF 11-byte limit"
        )));
    }
    if !trimmed
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(Error::InvalidFieldSpec(format!(
            "field name {trimmed:?} contains unsupported characters"
        )));
    }
    Ok(trimmed.to_ascii_uppercase())
}

fn parse_type_part(raw: &str) -> Result<(FieldType, u8, u8)> {
    let raw = raw.trim();
    let type_code = raw
        .as_bytes()
        .first()
        .copied()
        .ok_or_else(|| Error::InvalidFieldSpec("field type cannot be empty".to_string()))?;
    let field_type = FieldType::from_byte(type_code.to_ascii_uppercase())?;
    let dimensions = raw[1..].trim();
    let (mut length, mut decimals) = match field_type.fixed_length() {
        Some(length) => (length, 0),
        None => match field_type {
            FieldType::Character => (1, 0),
            FieldType::Numeric | FieldType::Float => (18, 0),
            _ => (0, 0),
        },
    };
    if !dimensions.is_empty() {
        if !(dimensions.starts_with('(') && dimensions.ends_with(')')) {
            return Err(Error::InvalidFieldSpec(format!(
                "invalid field size syntax in {raw:?}"
            )));
        }
        let inner = &dimensions[1..dimensions.len() - 1];
        let mut items = inner.split(',').map(str::trim);
        length = items
            .next()
            .ok_or_else(|| Error::InvalidFieldSpec(format!("missing field length in {raw:?}")))?
            .parse::<u8>()
            .map_err(|_| Error::InvalidFieldSpec(format!("invalid field length in {raw:?}")))?;
        decimals = match items.next() {
            Some(value) if !value.is_empty() => value.parse::<u8>().map_err(|_| {
                Error::InvalidFieldSpec(format!("invalid decimal count in {raw:?}"))
            })?,
            _ => 0,
        };
        if items.next().is_some() {
            return Err(Error::InvalidFieldSpec(format!(
                "too many numeric modifiers in {raw:?}"
            )));
        }
    }
    if length == 0 {
        return Err(Error::InvalidFieldSpec(format!(
            "field length must be greater than zero in {raw:?}"
        )));
    }
    Ok((field_type, length, decimals))
}

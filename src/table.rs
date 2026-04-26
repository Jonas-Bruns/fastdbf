use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::header::{CodePageMark, DbfKind, FieldDescriptor, FieldType, Header};
use crate::memo::MemoFile;
use crate::record::Record;
use crate::spec::FieldSpec;
use crate::value::{Date, DateTime, Value};

pub const CLOSED: &str = "closed";
pub const READ_ONLY: &str = "read_only";
pub const READ_WRITE: &str = "read_write";
pub const IN_MEMORY: &str = "in_memory";
pub const ON_DISK: &str = "on_disk";

pub struct Table {
    path: Option<PathBuf>,
    header: Header,
    fields: Vec<FieldDescriptor>,
    records: Vec<Record>,
    null_flags: Option<NullFlagLayout>,
    pub memo_file: Option<MemoFile>,
}

#[derive(Debug, Clone, Copy)]
struct NullFlagLayout {
    offset: u16,
    length: u8,
}

impl Table {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;
        let mut raw_header = [0u8; 32];
        file.read_exact(&mut raw_header)?;

        let kind = DbfKind::from_version(raw_header[0])?;
        let update = decode_update_date(raw_header[1], raw_header[2], raw_header[3]);
        let record_count =
            u32::from_le_bytes([raw_header[4], raw_header[5], raw_header[6], raw_header[7]]);
        let header_length = u16::from_le_bytes([raw_header[8], raw_header[9]]);
        let record_length = u16::from_le_bytes([raw_header[10], raw_header[11]]);
        let code_page = CodePageMark(raw_header[29]);

        if header_length < 33 {
            return Err(Error::InvalidFormat(format!(
                "DBF header too short: {header_length}"
            )));
        }

        let header = Header {
            kind,
            last_update: update,
            record_count,
            header_length,
            record_length,
            code_page,
        };

        let mut memo_file = MemoFile::open_alongside(&path, header.kind)?;
        let encoding = crate::codepage::encoding_for_mark(header.code_page.0);

        let (fields, null_flags) = read_field_descriptors(&mut file, &header)?;
        let records = read_records(
            &mut file,
            &header,
            &fields,
            null_flags,
            memo_file.as_mut(),
            encoding,
        )?;

        // The header's record_count may not match the actual number of records in
        // truncated or otherwise corrupt files. Sync it so callers get a consistent
        // view and so write_to_path() doesn't encode a stale value.
        let mut header = header;
        header.record_count = records.len() as u32;

        Ok(Self {
            path: Some(path),
            header,
            fields,
            records,
            null_flags,
            memo_file,
        })
    }

    pub fn new(field_specs: &str) -> Result<Self> {
        let specs = FieldSpec::parse_many(field_specs)?;
        Self::from_specs(specs, None)
    }

    pub fn from_specs(specs: Vec<FieldSpec>, kind: Option<DbfKind>) -> Result<Self> {
        let mut offset = 1u16;
        let mut fields = Vec::with_capacity(specs.len());
        let mut inferred_kind = kind.unwrap_or(DbfKind::DBase3);
        let mut nullable_index = 0usize;
        for spec in specs {
            let mut descriptor = spec.to_descriptor(offset);
            offset += descriptor.length as u16;
            if descriptor.field_type.requires_vfp() || spec.nullable {
                inferred_kind = DbfKind::VisualFoxPro;
            }
            if spec.nullable {
                descriptor.nullable_index = Some(nullable_index);
                nullable_index += 1;
            }
            fields.push(descriptor);
        }
        if nullable_index > 0
            && !matches!(
                inferred_kind,
                DbfKind::VisualFoxPro
                    | DbfKind::VisualFoxProAutoIncrement
                    | DbfKind::VisualFoxProVar
            )
        {
            return Err(Error::Unsupported(
                "nullable fields require Visual FoxPro table type".to_string(),
            ));
        }
        let null_flags = if nullable_index > 0 {
            let length = nullable_len(nullable_index)?;
            let layout = NullFlagLayout { offset, length };
            offset += length as u16;
            Some(layout)
        } else {
            None
        };
        let header_length = 32 + ((fields.len() as u16 + u16::from(null_flags.is_some())) * 32) + 1;
        let record_length = offset;
        let header = Header {
            kind: inferred_kind,
            last_update: None,
            record_count: 0,
            header_length,
            record_length,
            code_page: CodePageMark(0x00),
        };
        Ok(Self {
            path: None,
            header,
            fields,
            records: Vec::new(),
            null_flags,
            memo_file: None,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn structure(&self) -> String {
        self.fields
            .iter()
            .map(|field| {
                let mut spec = match field.field_type {
                    FieldType::Character => format!("{} C({})", field.name, field.length),
                    FieldType::Numeric => {
                        format!("{} N({},{})", field.name, field.length, field.decimals)
                    }
                    FieldType::Float => {
                        format!("{} F({},{})", field.name, field.length, field.decimals)
                    }
                    FieldType::Date => format!("{} D", field.name),
                    FieldType::Logical => format!("{} L", field.name),
                    FieldType::Memo => format!("{} M", field.name),
                    FieldType::Integer => format!("{} I", field.name),
                    FieldType::Double => format!("{} B", field.name),
                    FieldType::DateTime => format!("{} T", field.name),
                    FieldType::Currency => format!("{} Y", field.name),
                    FieldType::General => format!("{} G", field.name),
                    FieldType::Picture => format!("{} P", field.name),
                    FieldType::NullFlags => format!("{} 0", field.name),
                };
                if field.is_nullable() {
                    spec.push_str(" null");
                }
                spec
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub fn new_like(
        &self,
        filename: impl AsRef<str>,
        kind: Option<DbfKind>,
        on_disk: bool,
    ) -> Result<Self> {
        let specs = self
            .fields
            .iter()
            .filter(|field| field.field_type != FieldType::NullFlags)
            .map(|field| FieldSpec {
                name: field.name.clone(),
                field_type: field.field_type,
                length: field.length,
                decimals: field.decimals,
                nullable: field.is_nullable(),
            })
            .collect::<Vec<_>>();
        let mut table = Self::from_specs(specs, kind.or(Some(self.header.kind)))?;
        if on_disk {
            table.path = Some(PathBuf::from(filename.as_ref()));
        }
        Ok(table)
    }

    pub fn field_info(&self, name: &str) -> Result<&FieldDescriptor> {
        let normalized = name.trim().to_ascii_uppercase();
        self.fields
            .iter()
            .find(|field| field.name == normalized)
            .ok_or(Error::FieldNotFound(normalized))
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn fields(&self) -> &[FieldDescriptor] {
        &self.fields
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }

    pub fn records_mut(&mut self) -> &mut [Record] {
        &mut self.records
    }

    pub fn push_record(&mut self, record: Record) -> Result<()> {
        if record.values().len() != self.fields.len() {
            return Err(Error::InvalidFormat(format!(
                "record width {} does not match table width {}",
                record.values().len(),
                self.fields.len()
            )));
        }
        self.records.push(record);
        self.header.record_count = self.records.len() as u32;
        Ok(())
    }

    pub fn new_record(&self) -> Record {
        Record::new(&self.fields)
    }

    pub fn write_to_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.header.record_count = self.records.len() as u32;
        self.header.last_update = None;

        let path = path.as_ref().to_path_buf();

        let has_memo = self.fields.iter().any(|f| {
            matches!(
                f.field_type,
                FieldType::Memo | FieldType::General | FieldType::Picture
            )
        });
        if has_memo {
            self.memo_file = Some(MemoFile::create_alongside(&path, self.header.kind)?);
        } else {
            self.memo_file = None;
        }

        let mut file = File::create(&path)?;
        let header_bytes = encode_header(&self.header);
        file.write_all(&header_bytes)?;

        for descriptor in &self.fields {
            file.write_all(&encode_field_descriptor(descriptor))?;
        }
        if let Some(null_flags) = self.null_flags {
            file.write_all(&encode_field_descriptor(&FieldDescriptor {
                name: "_NULLFLAGS".to_string(),
                field_type: FieldType::NullFlags,
                offset: null_flags.offset,
                length: null_flags.length,
                decimals: 0,
                flags: 0,
                nullable_index: None,
            }))?;
        }
        file.write_all(&[0x0D])?;

        let encoding = crate::codepage::encoding_for_mark(self.header.code_page.0);

        for record in &self.records {
            file.write_all(&encode_record(
                record,
                &self.fields,
                self.null_flags,
                self.header.record_length as usize,
                self.memo_file.as_mut(),
                encoding,
            )?)?;
        }
        file.write_all(&[0x1A])?;
        file.flush()?;
        self.path = Some(path);
        Ok(())
    }

    pub fn pack(&mut self) {
        self.records.retain(|r| !r.is_deleted());
        self.header.record_count = self.records.len() as u32;
    }

    pub fn add_fields(&mut self, _specs: &str) -> Result<()> {
        Err(Error::Unsupported(
            "schema modification not yet implemented".to_string(),
        ))
    }

    pub fn remove_fields(&mut self, _names: &[&str]) -> Result<()> {
        Err(Error::Unsupported(
            "schema modification not yet implemented".to_string(),
        ))
    }

    pub fn rename_field(&mut self, _old_name: &str, _new_name: &str) -> Result<()> {
        Err(Error::Unsupported(
            "schema modification not yet implemented".to_string(),
        ))
    }
}

fn read_field_descriptors(
    file: &mut File,
    header: &Header,
) -> Result<(Vec<FieldDescriptor>, Option<NullFlagLayout>)> {
    let field_count = ((header.header_length as usize) - 33) / 32;
    let mut fields = Vec::with_capacity(field_count);
    let mut null_flags = None;
    let mut nullable_index = 0usize;
    for _ in 0..field_count {
        let mut raw = [0u8; 32];
        file.read_exact(&mut raw)?;
        if raw[0] == 0x0D {
            break;
        }
        let name_end = raw[..11].iter().position(|byte| *byte == 0).unwrap_or(11);
        let name = String::from_utf8_lossy(&raw[..name_end])
            .trim()
            .to_ascii_uppercase();
        let field_type = FieldType::from_byte(raw[11])?;
        let offset = u32::from_le_bytes([raw[12], raw[13], raw[14], raw[15]]) as u16;
        let length = raw[16];
        let decimals = raw[17];
        let flags = raw[18];
        if field_type == FieldType::NullFlags {
            null_flags = Some(NullFlagLayout { offset, length });
            continue;
        }
        let nullable_index_for_field = if flags & crate::header::FIELD_FLAG_NULLABLE != 0 {
            let current = nullable_index;
            nullable_index += 1;
            Some(current)
        } else {
            None
        };
        fields.push(FieldDescriptor {
            name,
            field_type,
            offset,
            length,
            decimals,
            flags,
            nullable_index: nullable_index_for_field,
        });
    }
    file.seek(SeekFrom::Start(header.header_length as u64))?;
    Ok((fields, null_flags))
}

fn read_records(
    file: &mut File,
    header: &Header,
    fields: &[FieldDescriptor],
    null_flags: Option<NullFlagLayout>,
    mut memo_file: Option<&mut MemoFile>,
    encoding: Option<&'static encoding_rs::Encoding>,
) -> Result<Vec<Record>> {
    let mut records = Vec::with_capacity(header.record_count as usize);
    for _ in 0..header.record_count {
        let mut raw = vec![0u8; header.record_length as usize];
        match file.read_exact(&mut raw) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error.into()),
        }
        let deleted = matches!(raw[0], b'*');
        let mut values = Vec::with_capacity(fields.len());
        let null_bits = null_flags.map(|layout| {
            let start = layout.offset as usize;
            let end = start + layout.length as usize;
            raw[start..end].to_vec()
        });
        for field in fields {
            let start = field.offset as usize;
            let end = start + field.length as usize;
            let is_null = field
                .nullable_index
                .zip(null_bits.as_ref())
                .map(|(index, bytes)| null_bit_is_set(bytes, index))
                .unwrap_or(false);
            values.push(parse_value(
                field,
                &raw[start..end],
                is_null,
                memo_file.as_deref_mut(),
                encoding,
            )?);
        }
        records.push(Record::from_values(deleted, values));
    }
    Ok(records)
}

fn parse_value(
    field: &FieldDescriptor,
    raw: &[u8],
    is_null: bool,
    memo_file: Option<&mut MemoFile>,
    encoding: Option<&'static encoding_rs::Encoding>,
) -> Result<Value> {
    if is_null {
        return Ok(Value::Null);
    }
    match field.field_type {
        FieldType::Character => {
            let text = crate::codepage::decode_bytes(raw, encoding);
            Ok(Value::Character(text.trim_end().to_string()))
        }
        FieldType::Date => {
            let text = String::from_utf8_lossy(raw);
            Ok(Value::Date(Date::parse_ymd(&text)?))
        }
        FieldType::Logical => {
            let value = match raw.first().copied().unwrap_or(b' ') {
                b'Y' | b'y' | b'T' | b't' => Some(true),
                b'N' | b'n' | b'F' | b'f' => Some(false),
                b'?' | b' ' => None,
                byte => {
                    return Err(Error::InvalidFormat(format!(
                        "invalid logical value byte: 0x{byte:02X}"
                    )))
                }
            };
            Ok(Value::Logical(value))
        }
        FieldType::Numeric | FieldType::Float => {
            let text = String::from_utf8_lossy(raw);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(Value::Null)
            } else {
                let number = trimmed.parse::<f64>().map_err(|_| {
                    Error::InvalidFormat(format!("invalid numeric payload: {trimmed:?}"))
                })?;
                Ok(Value::Numeric(number))
            }
        }
        FieldType::Integer => Ok(Value::Integer(i32::from_le_bytes([
            raw[0], raw[1], raw[2], raw[3],
        ]))),
        FieldType::Double => Ok(Value::Double(f64::from_le_bytes([
            raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
        ]))),
        FieldType::DateTime => {
            let julian_day = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            let millis = i32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
            if julian_day == 0 && millis == 0 {
                Ok(Value::DateTime(None))
            } else {
                Ok(Value::DateTime(Some(DateTime::new(julian_day, millis))))
            }
        }
        FieldType::Currency => Ok(Value::Currency(i64::from_le_bytes([
            raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
        ]))),
        FieldType::Memo => {
            let text = String::from_utf8_lossy(raw);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(Value::Memo(Vec::new()))
            } else {
                let pointer = trimmed.parse::<u32>().map_err(|_| {
                    Error::InvalidFormat(format!("invalid memo pointer payload: {trimmed:?}"))
                })?;
                if let Some(memo) = memo_file {
                    let bytes = memo.read(pointer)?;
                    Ok(Value::Memo(bytes))
                } else {
                    Ok(Value::Memo(Vec::new()))
                }
            }
        }
        FieldType::General | FieldType::Picture => {
            let text = String::from_utf8_lossy(raw);
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(Value::Binary(Vec::new()))
            } else {
                let pointer = trimmed.parse::<u32>().map_err(|_| {
                    Error::InvalidFormat(format!("invalid memo pointer payload: {trimmed:?}"))
                })?;
                if let Some(memo) = memo_file {
                    let bytes = memo.read(pointer)?;
                    Ok(Value::Binary(bytes))
                } else {
                    Ok(Value::Binary(Vec::new()))
                }
            }
        }
        FieldType::NullFlags => Ok(Value::Binary(raw.to_vec())),
    }
}

fn encode_header(header: &Header) -> [u8; 32] {
    let mut raw = [0u8; 32];
    raw[0] = header.kind.version_byte();
    if let Some(date) = header.last_update {
        raw[1] = date.year.saturating_sub(1900) as u8;
        raw[2] = date.month;
        raw[3] = date.day;
    }
    raw[4..8].copy_from_slice(&header.record_count.to_le_bytes());
    raw[8..10].copy_from_slice(&header.header_length.to_le_bytes());
    raw[10..12].copy_from_slice(&header.record_length.to_le_bytes());
    raw[29] = header.code_page.0;
    raw
}

fn encode_field_descriptor(field: &FieldDescriptor) -> [u8; 32] {
    let mut raw = [0u8; 32];
    let name = field.name.as_bytes();
    let count = name.len().min(11);
    raw[..count].copy_from_slice(&name[..count]);
    raw[11] = field.field_type.symbol();
    raw[12..16].copy_from_slice(&(field.offset as u32).to_le_bytes());
    raw[16] = field.length;
    raw[17] = field.decimals;
    raw[18] = field.flags;
    raw
}

fn encode_record(
    record: &Record,
    fields: &[FieldDescriptor],
    null_flags: Option<NullFlagLayout>,
    record_length: usize,
    mut memo_file: Option<&mut MemoFile>,
    encoding: Option<&'static encoding_rs::Encoding>,
) -> Result<Vec<u8>> {
    let mut raw = vec![b' '; record_length];
    raw[0] = if record.is_deleted() { b'*' } else { b' ' };
    if let Some(layout) = null_flags {
        let start = layout.offset as usize;
        let end = start + layout.length as usize;
        raw[start..end].fill(0);
    }
    for (value, field) in record.values().iter().zip(fields) {
        if matches!(value, Value::Null) {
            if let Some(bit) = field.nullable_index {
                if let Some(layout) = null_flags {
                    let start = layout.offset as usize;
                    set_null_bit(&mut raw[start..start + layout.length as usize], bit);
                }
            }
        }
        let encoded = encode_value(field, value, memo_file.as_deref_mut(), encoding)?;
        let start = field.offset as usize;
        let end = start + field.length as usize;
        raw[start..end].copy_from_slice(&encoded);
    }
    Ok(raw)
}

fn encode_value(
    field: &FieldDescriptor,
    value: &Value,
    memo_file: Option<&mut MemoFile>,
    encoding: Option<&'static encoding_rs::Encoding>,
) -> Result<Vec<u8>> {
    let size = field.length as usize;
    match (field.field_type, value) {
        (_, Value::Null) if field.is_nullable() => Ok(blank_bytes_for_null(field)),
        (FieldType::Character, Value::Character(text)) => {
            let bytes = crate::codepage::encode_str(text, encoding).ok_or_else(|| {
                Error::InvalidFormat("cannot encode string to field code page".to_string())
            })?;
            if bytes.len() > size {
                return Err(Error::Overflow(format!(
                    "value {text:?} exceeds width {} for field {}",
                    field.length, field.name
                )));
            }
            let mut raw = vec![b' '; size];
            raw[..bytes.len()].copy_from_slice(&bytes);
            Ok(raw)
        }
        (FieldType::Character, Value::Null) => Ok(vec![b' '; size]),
        (FieldType::Date, Value::Date(Some(date))) => Ok(date.to_ymd_string().into_bytes()),
        (FieldType::Date, Value::Date(None)) | (FieldType::Date, Value::Null) => {
            Ok(vec![b' '; size])
        }
        (FieldType::Logical, Value::Logical(Some(true))) => Ok(vec![b'T']),
        (FieldType::Logical, Value::Logical(Some(false))) => Ok(vec![b'F']),
        (FieldType::Logical, Value::Logical(None)) | (FieldType::Logical, Value::Null) => {
            Ok(vec![b'?'])
        }
        (FieldType::Numeric | FieldType::Float, Value::Numeric(number)) => {
            let rendered = if field.decimals == 0 {
                format!("{number:.0}")
            } else {
                format!("{number:.prec$}", prec = field.decimals as usize)
            };
            if rendered.len() > size {
                return Err(Error::Overflow(format!(
                    "numeric value {rendered:?} exceeds width {} for field {}",
                    field.length, field.name
                )));
            }
            let mut raw = vec![b' '; size];
            let start = size - rendered.len();
            raw[start..].copy_from_slice(rendered.as_bytes());
            Ok(raw)
        }
        (FieldType::Numeric | FieldType::Float, Value::Null) => Ok(vec![b' '; size]),
        (FieldType::Integer, Value::Integer(number)) => Ok(number.to_le_bytes().to_vec()),
        (FieldType::Double, Value::Double(number)) => Ok(number.to_le_bytes().to_vec()),
        (FieldType::DateTime, Value::DateTime(Some(datetime))) => {
            let mut raw = Vec::with_capacity(8);
            raw.extend_from_slice(&datetime.julian_day.to_le_bytes());
            raw.extend_from_slice(&datetime.millis_since_midnight.to_le_bytes());
            Ok(raw)
        }
        (FieldType::DateTime, Value::DateTime(None)) | (FieldType::DateTime, Value::Null) => {
            Ok(vec![0u8; 8])
        }
        (FieldType::Currency, Value::Currency(number)) => Ok(number.to_le_bytes().to_vec()),
        (FieldType::Memo, Value::Memo(bytes))
        | (FieldType::General | FieldType::Picture, Value::Binary(bytes)) => {
            if bytes.is_empty() {
                Ok(vec![b' '; size])
            } else if let Some(memo) = memo_file {
                let pointer = memo.append(bytes)?;
                let rendered = format!("{pointer:>width$}", width = size);
                Ok(rendered.into_bytes())
            } else {
                Err(Error::Io(std::io::Error::other(
                    "missing memo file for writing",
                )))
            }
        }
        (FieldType::Memo | FieldType::General | FieldType::Picture, Value::Null) => {
            Ok(vec![b' '; size])
        }
        (FieldType::NullFlags, Value::Binary(bytes)) if bytes.len() == size => Ok(bytes.clone()),
        (_, Value::Binary(bytes)) if bytes.len() == size => Ok(bytes.clone()),
        _ => Err(Error::InvalidFormat(format!(
            "value {value:?} is incompatible with field {} ({:?})",
            field.name, field.field_type
        ))),
    }
}

fn blank_bytes_for_null(field: &FieldDescriptor) -> Vec<u8> {
    match field.field_type {
        FieldType::Character
        | FieldType::Date
        | FieldType::Memo
        | FieldType::General
        | FieldType::Picture
        | FieldType::Numeric
        | FieldType::Float => vec![b' '; field.length as usize],
        FieldType::Logical => vec![b'?'],
        FieldType::Integer | FieldType::Double | FieldType::DateTime | FieldType::Currency => {
            vec![0u8; field.length as usize]
        }
        FieldType::NullFlags => vec![0u8; field.length as usize],
    }
}

fn nullable_len(nullable_count: usize) -> Result<u8> {
    let length = nullable_count.div_ceil(8);
    u8::try_from(length)
        .map_err(|_| Error::InvalidFieldSpec("too many nullable fields".to_string()))
}

fn null_bit_is_set(bytes: &[u8], bit_index: usize) -> bool {
    let byte_index = bit_index / 8;
    let bit = bit_index % 8;
    bytes
        .get(byte_index)
        .map(|byte| byte & (1 << bit) != 0)
        .unwrap_or(false)
}

fn set_null_bit(bytes: &mut [u8], bit_index: usize) {
    let byte_index = bit_index / 8;
    let bit = bit_index % 8;
    if let Some(byte) = bytes.get_mut(byte_index) {
        *byte |= 1 << bit;
    }
}

fn decode_update_date(year: u8, month: u8, day: u8) -> Option<Date> {
    if year == 0 || month == 0 || day == 0 {
        None
    } else {
        Some(Date::new(1900 + year as u16, month, day))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    #[test]
    fn field_spec_parser_handles_common_types() {
        let table = Table::new("name C(25); age N(3,0); birth D; qualified L").unwrap();
        assert_eq!(table.fields.len(), 4);
        assert_eq!(table.fields[0].name, "NAME");
        assert_eq!(table.fields[1].length, 3);
        assert_eq!(table.fields[2].field_type, FieldType::Date);
    }

    #[test]
    fn round_trip_nullable_vfp_fields() {
        let mut table =
            Table::new("name C(10) null; age N(3,0) null; when T null; active L null").unwrap();
        let mut record = table.new_record();
        record.insert(table.fields(), "name", Value::Null).unwrap();
        record.insert(table.fields(), "age", Value::Null).unwrap();
        record.insert(table.fields(), "when", Value::Null).unwrap();
        record
            .insert(table.fields(), "active", Value::Null)
            .unwrap();
        table.push_record(record).unwrap();

        let path = std::env::temp_dir().join(format!(
            "dbf-rs-nullable-{}.dbf",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        table.write_to_path(&path).unwrap();

        let reopened = Table::open(&path).unwrap();
        assert!(reopened.null_flags.is_some());
        assert_eq!(
            reopened.records[0].get(reopened.fields(), "NAME").unwrap(),
            &Value::Null
        );
        assert_eq!(
            reopened.records[0].get(reopened.fields(), "AGE").unwrap(),
            &Value::Null
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn round_trip_write_and_read() {
        let mut table =
            Table::new("name C(10); age N(3,0); birth D; qualified L; score B; counter I").unwrap();
        let mut record = table.new_record();
        record
            .insert(table.fields(), "name", Value::Character("Spunky".into()))
            .unwrap();
        record
            .insert(table.fields(), "age", Value::Numeric(23.0))
            .unwrap();
        record
            .insert(
                table.fields(),
                "birth",
                Value::Date(Some(Date::new(1989, 7, 23))),
            )
            .unwrap();
        record
            .insert(table.fields(), "qualified", Value::Logical(Some(true)))
            .unwrap();
        record
            .insert(table.fields(), "score", Value::Double(4.5))
            .unwrap();
        record
            .insert(table.fields(), "counter", Value::Integer(7))
            .unwrap();
        table.push_record(record).unwrap();

        let path = std::env::temp_dir().join(format!(
            "dbf-rs-roundtrip-{}.dbf",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        table.write_to_path(&path).unwrap();

        let reopened = Table::open(&path).unwrap();
        assert_eq!(reopened.records.len(), 1);
        assert_eq!(
            reopened.records[0].get(reopened.fields(), "NAME").unwrap(),
            &Value::Character("Spunky".into())
        );
        assert_eq!(
            reopened.records[0]
                .get(reopened.fields(), "QUALIFIED")
                .unwrap(),
            &Value::Logical(Some(true))
        );

        let _ = std::fs::remove_file(path);
    }
}

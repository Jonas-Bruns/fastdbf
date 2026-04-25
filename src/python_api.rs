use pyo3::exceptions::{PyIOError, PyKeyError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::header::FieldDescriptor;
use crate::record::Record;
use crate::table::{Table, CLOSED, IN_MEMORY, ON_DISK, READ_ONLY, READ_WRITE};
use crate::value::{Date, DateTime, Value};

/// Status values exposed to Python as `fastdbf.TableStatus`.
#[pyclass(eq, eq_int, name = "TableStatus")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableStatus {
    Closed,
    ReadOnly,
    ReadWrite,
}

#[pymethods]
impl TableStatus {
    fn __repr__(&self) -> &'static str {
        match self {
            Self::Closed => "TableStatus.Closed",
            Self::ReadOnly => "TableStatus.ReadOnly",
            Self::ReadWrite => "TableStatus.ReadWrite",
        }
    }

    fn __str__(&self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
        }
    }
}

/// Location values exposed to Python as `fastdbf.TableLocation`.
#[pyclass(eq, eq_int, name = "TableLocation")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableLocation {
    InMemory,
    OnDisk,
}

#[pymethods]
impl TableLocation {
    fn __repr__(&self) -> &'static str {
        match self {
            Self::InMemory => "TableLocation.InMemory",
            Self::OnDisk => "TableLocation.OnDisk",
        }
    }

    fn __str__(&self) -> &'static str {
        match self {
            Self::InMemory => "in_memory",
            Self::OnDisk => "on_disk",
        }
    }
}

#[pyclass(name = "Table")]
pub struct PyTable {
    inner: Table,
    default_filename: Option<String>,
    on_disk: bool,
    status: TableStatus,
}

#[pymethods]
impl PyTable {
    #[new]
    #[pyo3(signature = (filename, field_specs=None, on_disk=true, dbf_type=None, codepage=None))]
    fn new(
        filename: String,
        field_specs: Option<String>,
        on_disk: bool,
        dbf_type: Option<String>,
        codepage: Option<String>,
    ) -> PyResult<Self> {
        if codepage.is_some() {
            return Err(PyValueError::new_err(
                "the 'codepage' parameter is not yet supported; omit it or file a feature request",
            ));
        }
        let inner = match field_specs {
            Some(specs) => {
                let kind = dbf_type_to_kind(dbf_type.as_deref())?;
                Table::from_specs(crate::spec::FieldSpec::parse_many(&specs).map_err(to_py_error)?, kind)
                    .map_err(to_py_error)?
            }
            None => Table::open(&filename).map_err(to_py_error)?,
        };
        Ok(Self {
            inner,
            default_filename: Some(filename),
            on_disk,
            status: TableStatus::Closed,
        })
    }

    #[getter]
    fn kind(&self) -> String {
        format!("{:?}", self.inner.header().kind)
    }

    #[getter]
    fn field_names(&self) -> Vec<String> {
        self.inner
            .fields()
            .iter()
            .map(|field| field.name.clone())
            .collect()
    }

    #[getter]
    fn record_count(&self) -> usize {
        self.inner.records().len()
    }

    #[getter]
    fn status(&self) -> TableStatus {
        self.status
    }

    #[getter]
    fn filename(&self) -> Option<String> {
        self.default_filename.clone()
    }

    #[getter]
    fn location(&self) -> TableLocation {
        if self.on_disk {
            TableLocation::OnDisk
        } else {
            TableLocation::InMemory
        }
    }

    fn records<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        self.require_open()?;
        let items = self
            .inner
            .records()
            .iter()
            .map(|record| record_to_dict(py, self.inner.fields(), record))
            .collect::<PyResult<Vec<_>>>()?;
        PyList::new(py, items)
    }

    fn fields<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        self.require_open()?;
        let items = self
            .inner
            .fields()
            .iter()
            .map(|field| field_to_dict(py, field))
            .collect::<PyResult<Vec<_>>>()?;
        PyList::new(py, items)
    }

    fn row<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Bound<'py, PyDict>> {
        self.require_open()?;
        let record = self
            .inner
            .records()
            .get(index)
            .ok_or_else(|| PyKeyError::new_err(format!("record index out of range: {index}")))?;
        record_to_dict(py, self.inner.fields(), record)
    }

    fn append(&mut self, row: &Bound<'_, PyAny>) -> PyResult<()> {
        self.require_read_write()?;
        let mut record = self.inner.new_record();
        if let Ok(dict) = row.downcast::<PyDict>() {
            for field in self.inner.fields() {
                if let Some(value) = dict.get_item(field.name.as_str())? {
                    let converted = py_to_value(value.as_any(), field)?;
                    record
                        .insert(self.inner.fields(), &field.name, converted)
                        .map_err(to_py_error)?;
                }
            }
        } else if let Ok(sequence) = row.extract::<Vec<PyObject>>() {
            if sequence.len() != self.inner.fields().len() {
                return Err(PyValueError::new_err(format!(
                    "append sequence width {} does not match table width {}",
                    sequence.len(),
                    self.inner.fields().len()
                )));
            }
            Python::with_gil(|py| -> PyResult<()> {
                for (field, value) in self.inner.fields().iter().zip(sequence.iter()) {
                    let converted = py_to_value(value.bind(py), field)?;
                    record
                        .insert(self.inner.fields(), &field.name, converted)
                        .map_err(to_py_error)?;
                }
                Ok(())
            })?;
        } else {
            return Err(PyTypeError::new_err("append expects a dict or a sequence"));
        }
        self.inner.push_record(record).map_err(to_py_error)
    }

    #[pyo3(signature = (mode=None))]
    fn open(&mut self, mode: Option<&str>) {
        self.status = match mode {
            Some("r") | Some("rb") => TableStatus::ReadOnly,
            _ => TableStatus::ReadWrite,
        };
    }

    fn close(&mut self) -> PyResult<()> {
        if self.on_disk && self.status == TableStatus::ReadWrite {
            if let Some(filename) = self.default_filename.clone() {
                if filename != ":memory:" {
                    self.inner.write_to_path(filename).map_err(to_py_error)?;
                }
            }
        }
        self.status = TableStatus::Closed;
        Ok(())
    }

    fn write(&mut self, path: &str) -> PyResult<()> {
        self.inner.write_to_path(path).map_err(to_py_error)
    }

    fn __getitem__<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Bound<'py, PyDict>> {
        self.row(py, index)
    }

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        Ok(self.records(py)?.call_method0("__iter__")?.unbind().into())
    }

    fn __len__(&self) -> usize {
        self.inner.records().len()
    }

    fn __repr__(&self) -> String {
        format!(
            "Table(filename={:?}, fields={}, records={})",
            self.default_filename,
            self.inner.fields().len(),
            self.inner.records().len()
        )
    }

    #[pyo3(signature = (filename=":memory:", default_data_types=None, field_specs=None, on_disk=false, dbf_type=None))]
    #[pyo3(name = "new")]
    fn clone_like(
        &self,
        filename: &str,
        default_data_types: Option<&Bound<'_, PyAny>>,
        field_specs: Option<String>,
        on_disk: bool,
        dbf_type: Option<&str>,
    ) -> PyResult<PyTable> {
        if default_data_types.is_some() {
            return Err(PyValueError::new_err(
                "the 'default_data_types' parameter is not yet supported; omit it or file a feature request",
            ));
        }
        let table = match field_specs {
            Some(specs) => {
                let kind = dbf_type_to_kind(dbf_type)?;
                Table::from_specs(crate::spec::FieldSpec::parse_many(&specs).map_err(to_py_error)?, kind)
                    .map_err(to_py_error)?
            }
            None => self
                .inner
                .new_like(filename, dbf_type_to_kind(dbf_type)?, on_disk)
                .map_err(to_py_error)?,
        };
        Ok(PyTable {
            inner: table,
            default_filename: Some(filename.to_string()),
            on_disk,
            status: TableStatus::Closed,
        })
    }

    #[pyo3(signature = (field=None))]
    fn structure(&self, field: Option<&str>) -> PyResult<String> {
        match field {
            Some(name) => {
                let info = self.inner.field_info(name).map_err(to_py_error)?;
                let mut spec = match info.field_type {
                    crate::header::FieldType::Character => format!("{} C({})", info.name, info.length),
                    crate::header::FieldType::Numeric => format!("{} N({},{})", info.name, info.length, info.decimals),
                    crate::header::FieldType::Float => format!("{} F({},{})", info.name, info.length, info.decimals),
                    crate::header::FieldType::Date => format!("{} D", info.name),
                    crate::header::FieldType::Logical => format!("{} L", info.name),
                    crate::header::FieldType::Memo => format!("{} M", info.name),
                    crate::header::FieldType::Integer => format!("{} I", info.name),
                    crate::header::FieldType::Double => format!("{} B", info.name),
                    crate::header::FieldType::DateTime => format!("{} T", info.name),
                    crate::header::FieldType::Currency => format!("{} Y", info.name),
                    crate::header::FieldType::General => format!("{} G", info.name),
                    crate::header::FieldType::Picture => format!("{} P", info.name),
                    crate::header::FieldType::NullFlags => format!("{} 0", info.name),
                };
                if info.is_nullable() {
                    spec.push_str(" null");
                }
                Ok(spec)
            }
            None => Ok(self.inner.structure()),
        }
    }

    // ------------------------------------------------------------------
    // Guard helpers – not exposed to Python by name.
    // ------------------------------------------------------------------

    /// Returns `Ok(())` if the table has been opened in any mode.
    fn require_open(&self) -> PyResult<()> {
        if self.status == TableStatus::Closed {
            Err(PyRuntimeError::new_err(
                "table is closed; call open() before reading",
            ))
        } else {
            Ok(())
        }
    }

    /// Returns `Ok(())` only in ReadWrite mode.
    fn require_read_write(&self) -> PyResult<()> {
        match self.status {
            TableStatus::Closed => Err(PyRuntimeError::new_err(
                "table is closed; call open() before writing",
            )),
            TableStatus::ReadOnly => Err(PyRuntimeError::new_err(
                "table is open read-only; reopen without 'r' mode to write",
            )),
            TableStatus::ReadWrite => Ok(()),
        }
    }
}

#[pyfunction]
fn open_table(path: &str) -> PyResult<PyTable> {
    let table = Table::open(path).map_err(to_py_error)?;
    Ok(PyTable {
        inner: table,
        default_filename: Some(path.to_string()),
        on_disk: true,
        status: TableStatus::ReadWrite,
    })
}

#[pyfunction]
#[pyo3(signature = (field_specs, filename=":memory:", on_disk=false, dbf_type=None))]
fn create_table(
    field_specs: &str,
    filename: &str,
    on_disk: bool,
    dbf_type: Option<&str>,
) -> PyResult<PyTable> {
    let kind = dbf_type_to_kind(dbf_type)?;
    let table = Table::from_specs(
        crate::spec::FieldSpec::parse_many(field_specs).map_err(to_py_error)?,
        kind,
    )
    .map_err(to_py_error)?;
    Ok(PyTable {
        inner: table,
        default_filename: Some(filename.to_string()),
        on_disk,
        status: TableStatus::Closed,
    })
}

#[pyfunction]
fn read_dbf<'py>(py: Python<'py>, path: &str) -> PyResult<Bound<'py, PyList>> {
    let table = Table::open(path).map_err(to_py_error)?;
    let items = table
        .records()
        .iter()
        .map(|record| record_to_dict(py, table.fields(), record))
        .collect::<PyResult<Vec<_>>>()?;
    PyList::new(py, items)
}

#[pyfunction]
fn field_names(thing: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    if let Ok(table) = thing.extract::<PyRef<'_, PyTable>>() {
        Ok(table.field_names())
    } else {
        Err(PyTypeError::new_err(
            "field_names currently supports Table objects only",
        ))
    }
}

#[pyfunction]
#[pyo3(signature = (csvfile, to_disk=false, filename=None, field_names=None, dbf_type="db3"))]
fn from_csv(
    csvfile: &str,
    to_disk: bool,
    filename: Option<&str>,
    field_names: Option<Vec<String>>,
    dbf_type: &str,
) -> PyResult<PyTable> {
    let data = std::fs::read_to_string(csvfile).map_err(|err| PyIOError::new_err(err.to_string()))?;
    let mut rows = data
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.split(',')
                .map(|cell| cell.trim().trim_matches('"').to_string())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let names = match field_names {
        Some(names) => names
            .into_iter()
            .map(|name| name.trim().to_ascii_uppercase())
            .collect::<Vec<_>>(),
        None => (0..width).map(|idx| format!("F{idx}")).collect::<Vec<_>>(),
    };
    let specs = names
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let max_len = rows
                .iter()
                .filter_map(|row| row.get(idx))
                .map(|value| value.len())
                .max()
                .unwrap_or(1)
                .max(1);
            format!("{name} C({max_len})")
        })
        .collect::<Vec<_>>()
        .join("; ");
    let output_name = filename.unwrap_or(":memory:");
    let mut table = create_table(&specs, output_name, to_disk, Some(dbf_type))?;
    table.open(None);
    for row in rows.drain(..) {
        let mut record = table.inner.new_record();
        for (field, cell) in table.inner.fields().iter().zip(row.iter()) {
            record
                .insert(table.inner.fields(), &field.name, Value::Character(cell.clone()))
                .map_err(to_py_error)?;
        }
        table.inner.push_record(record).map_err(to_py_error)?;
    }
    Ok(table)
}

#[pyfunction]
fn table_type(path: &str) -> PyResult<(u8, String)> {
    let table = Table::open(path).map_err(to_py_error)?;
    let version = table.header().kind.version_byte();
    Ok((version, format!("{:?}", table.header().kind)))
}

#[pymodule]
fn fastdbf(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyTable>()?;
    module.add_class::<TableStatus>()?;
    module.add_class::<TableLocation>()?;
    module.add_function(wrap_pyfunction!(open_table, module)?)?;
    module.add_function(wrap_pyfunction!(create_table, module)?)?;
    module.add_function(wrap_pyfunction!(read_dbf, module)?)?;
    module.add_function(wrap_pyfunction!(field_names, module)?)?;
    module.add_function(wrap_pyfunction!(from_csv, module)?)?;
    module.add_function(wrap_pyfunction!(table_type, module)?)?;
    // Legacy string constants – kept for backwards compatibility.
    module.add("CLOSED", CLOSED)?;
    module.add("READ_ONLY", READ_ONLY)?;
    module.add("READ_WRITE", READ_WRITE)?;
    module.add("IN_MEMORY", IN_MEMORY)?;
    module.add("ON_DISK", ON_DISK)?;
    Ok(())
}

fn field_to_dict<'py>(py: Python<'py>, field: &FieldDescriptor) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("name", field.name.as_str())?;
    dict.set_item("type", format!("{:?}", field.field_type))?;
    dict.set_item("type_code", (field.field_type.symbol() as char).to_string())?;
    dict.set_item("length", field.length)?;
    dict.set_item("decimals", field.decimals)?;
    dict.set_item("offset", field.offset)?;
    dict.set_item("nullable", field.is_nullable())?;
    Ok(dict)
}

fn record_to_dict<'py>(
    py: Python<'py>,
    fields: &[FieldDescriptor],
    record: &Record,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for (field, value) in fields.iter().zip(record.values()) {
        dict.set_item(field.name.as_str(), value_to_py(py, value)?)?;
    }
    dict.set_item("_deleted", record.is_deleted())?;
    Ok(dict)
}

fn value_to_py(py: Python<'_>, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Character(text) => Ok(text.into_pyobject(py)?.unbind().into()),
        Value::Numeric(number) => Ok(number.into_pyobject(py)?.unbind().into()),
        Value::Logical(value) => Ok(value.into_pyobject(py)?.unbind().into()),
        Value::Date(Some(date)) => Ok(date_to_iso(*date).into_pyobject(py)?.unbind().into()),
        Value::Date(None) => Ok(py.None()),
        Value::Integer(number) => Ok(number.into_pyobject(py)?.unbind().into()),
        Value::Double(number) => Ok(number.into_pyobject(py)?.unbind().into()),
        Value::DateTime(Some(value)) => {
            Ok(datetime_to_iso(*value).into_pyobject(py)?.unbind().into())
        }
        Value::DateTime(None) => Ok(py.None()),
        Value::Currency(number) => Ok(number.into_pyobject(py)?.unbind().into()),
        Value::MemoRef(pointer) => Ok(pointer.into_pyobject(py)?.unbind().into()),
        Value::Binary(bytes) => Ok(bytes.into_pyobject(py)?.unbind().into()),
    }
}

fn py_to_value(value: &Bound<'_, PyAny>, field: &FieldDescriptor) -> PyResult<Value> {
    if value.is_none() {
        return Ok(Value::Null);
    }
    match field.field_type {
        crate::header::FieldType::Character => Ok(Value::Character(value.extract::<String>()?)),
        crate::header::FieldType::Date => {
            let raw = value.extract::<String>()?;
            let normalized = raw.replace('-', "");
            Ok(Value::Date(Date::parse_ymd(&normalized).map_err(to_py_error)?))
        }
        crate::header::FieldType::Logical => Ok(Value::Logical(Some(value.extract::<bool>()?))),
        crate::header::FieldType::Numeric | crate::header::FieldType::Float => {
            Ok(Value::Numeric(value.extract::<f64>()?))
        }
        crate::header::FieldType::Integer => Ok(Value::Integer(value.extract::<i32>()?)),
        crate::header::FieldType::Double => Ok(Value::Double(value.extract::<f64>()?)),
        crate::header::FieldType::Currency => Ok(Value::Currency(value.extract::<i64>()?)),
        crate::header::FieldType::Memo
        | crate::header::FieldType::General
        | crate::header::FieldType::Picture => Ok(Value::MemoRef(value.extract::<u32>()?)),
        crate::header::FieldType::NullFlags => Err(PyTypeError::new_err(
            "internal null-flag field cannot be assigned from Python",
        )),
        crate::header::FieldType::DateTime => {
            let raw = value.extract::<String>()?;
            let (date_part, time_part) = raw
                .split_once('T')
                .ok_or_else(|| PyValueError::new_err("datetime must be ISO-like, e.g. 2024-01-31T12:34:56.000"))?;
            let date = Date::parse_ymd(&date_part.replace('-', "")).map_err(to_py_error)?
                .ok_or_else(|| PyValueError::new_err("datetime date part cannot be empty"))?;
            let datetime = iso_datetime_parts_to_vfp(date, time_part)?;
            Ok(Value::DateTime(Some(datetime)))
        }
    }
}

fn to_py_error(error: crate::Error) -> PyErr {
    match error {
        crate::Error::Io(io) => PyIOError::new_err(io.to_string()),
        crate::Error::FieldNotFound(field) => PyKeyError::new_err(field),
        crate::Error::InvalidFieldSpec(message)
        | crate::Error::InvalidFormat(message)
        | crate::Error::Overflow(message) => PyValueError::new_err(message),
        crate::Error::Unsupported(message) => PyTypeError::new_err(message),
    }
}

fn dbf_type_to_kind(dbf_type: Option<&str>) -> PyResult<Option<crate::header::DbfKind>> {
    match dbf_type.map(|value| value.trim().to_ascii_lowercase()) {
        None => Ok(None),
        Some(value) if value.is_empty() => Ok(None),
        Some(value) => match value.as_str() {
            "db3" => Ok(Some(crate::header::DbfKind::DBase3)),
            "vfp" => Ok(Some(crate::header::DbfKind::VisualFoxPro)),
            "fp" => Ok(Some(crate::header::DbfKind::FoxPro2WithMemo)),
            "db4" => Ok(Some(crate::header::DbfKind::DBase4WithMemo)),
            other => Err(PyValueError::new_err(format!(
                "unsupported dbf_type {other:?}; use 'db3', 'vfp', 'fp', or 'db4'"
            ))),
        },
    }
}

fn date_to_iso(date: Date) -> String {
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

fn datetime_to_iso(datetime: DateTime) -> String {
    match julian_day_to_ymd(datetime.julian_day) {
        Some((year, month, day)) => {
            let total_millis = datetime.millis_since_midnight.max(0) as u32;
            let hour = total_millis / 3_600_000;
            let minute = (total_millis % 3_600_000) / 60_000;
            let second = (total_millis % 60_000) / 1_000;
            let millis = total_millis % 1_000;
            format!(
                "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}"
            )
        }
        None => format!(
            "julian:{} millis:{}",
            datetime.julian_day, datetime.millis_since_midnight
        ),
    }
}

fn iso_datetime_parts_to_vfp(date: Date, time_part: &str) -> PyResult<DateTime> {
    let mut millis = 0u32;
    let (clock, fractional) = match time_part.split_once('.') {
        Some(parts) => parts,
        None => (time_part, ""),
    };
    let parts = clock.split(':').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(PyValueError::new_err(
            "datetime time part must look like HH:MM:SS or HH:MM:SS.mmm",
        ));
    }
    let hour = parts[0]
        .parse::<u32>()
        .map_err(|_| PyValueError::new_err("invalid hour"))?;
    let minute = parts[1]
        .parse::<u32>()
        .map_err(|_| PyValueError::new_err("invalid minute"))?;
    let second = parts[2]
        .parse::<u32>()
        .map_err(|_| PyValueError::new_err("invalid second"))?;
    if !fractional.is_empty() {
        let trimmed = &fractional[..fractional.len().min(3)];
        millis = trimmed
            .parse::<u32>()
            .map_err(|_| PyValueError::new_err("invalid millisecond fraction"))?;
        if trimmed.len() == 1 {
            millis *= 100;
        } else if trimmed.len() == 2 {
            millis *= 10;
        }
    }
    let total_millis = hour * 3_600_000 + minute * 60_000 + second * 1_000 + millis;
    Ok(DateTime::new(
        gregorian_to_julian_day(date.year as i32, date.month as i32, date.day as i32),
        total_millis as i32,
    ))
}

fn gregorian_to_julian_day(year: i32, month: i32, day: i32) -> i32 {
    let a = (14 - month) / 12;
    let y = year + 4800 - a;
    let m = month + 12 * a - 3;
    day + ((153 * m + 2) / 5) + 365 * y + (y / 4) - (y / 100) + (y / 400) - 32045
}

fn julian_day_to_ymd(julian_day: i32) -> Option<(i32, u32, u32)> {
    if julian_day <= 0 {
        return None;
    }
    let a = julian_day as i64 + 32044;
    let b = (4 * a + 3) / 146_097;
    let c = a - (146_097 * b) / 4;
    let d = (4 * c + 3) / 1_461;
    let e = c - (1_461 * d) / 4;
    let m = (5 * e + 2) / 153;

    let day = e - (153 * m + 2) / 5 + 1;
    let month = m + 3 - 12 * (m / 10);
    let year = 100 * b + d - 4_800 + m / 10;
    Some((year as i32, month as u32, day as u32))
}
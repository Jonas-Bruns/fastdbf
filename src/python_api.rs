use pyo3::exceptions::{PyIOError, PyKeyError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::codepage;
use crate::header::FieldDescriptor;
use crate::record::Record;
use crate::table::{Table, CLOSED, IN_MEMORY, ON_DISK, READ_ONLY, READ_WRITE};
use crate::value::{Date, DateTime, Value};

// ─────────────────────────────────────────────────────────────────────────────
// TableStatus / TableLocation enums (unchanged)
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// PyRecord – record object with attribute access + `with` protocol
// ─────────────────────────────────────────────────────────────────────────────

/// A single DBF record exposed to Python.
///
/// Supports dict-style access (`record["NAME"]`), attribute access
/// (`record.name`), sequence access (`record[0]`), iteration, and the
/// `with` context-manager protocol (changes are written back to the
/// parent table on `__exit__`).
#[pyclass(name = "Record")]
pub struct PyRecord {
    /// A clone of the record's current values.
    record: Record,
    /// Snapshot of the field descriptors at the time the record was created.
    fields: Vec<FieldDescriptor>,
    /// Encoding for Character fields (may be None = UTF-8 / bytes-as-is).
    encoding: Option<&'static encoding_rs::Encoding>,
    /// Whether this record is currently inside a `with` block.
    in_context: bool,
    /// Pending mutations to apply on `__exit__`.
    pending: Vec<(String, Value)>,
}

#[pymethods]
impl PyRecord {
    // ── Sequence protocol ────────────────────────────────────────────

    fn __len__(&self) -> usize {
        self.fields.len()
    }

    fn __getitem__<'py>(&self, py: Python<'py>, key: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        // Support both integer indices and string keys.
        if let Ok(index) = key.extract::<isize>() {
            let len = self.fields.len() as isize;
            let real_index = if index < 0 { len + index } else { index };
            if real_index < 0 || real_index >= len {
                return Err(PyKeyError::new_err(format!(
                    "record index out of range: {index}"
                )));
            }
            value_to_py(
                py,
                &self.record.values()[real_index as usize],
                self.encoding,
            )
        } else {
            let name = key.extract::<String>()?;
            let normalized = name.trim().to_ascii_uppercase();
            let (idx, _) = self
                .fields
                .iter()
                .enumerate()
                .find(|(_, f)| f.name == normalized)
                .ok_or_else(|| PyKeyError::new_err(format!("field not found: {normalized}")))?;
            value_to_py(py, &self.record.values()[idx], self.encoding)
        }
    }

    fn __setitem__(&mut self, key: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let name = if let Ok(index) = key.extract::<isize>() {
            let len = self.fields.len() as isize;
            let real = if index < 0 { len + index } else { index };
            if real < 0 || real >= len {
                return Err(PyKeyError::new_err(format!(
                    "record index out of range: {index}"
                )));
            }
            self.fields[real as usize].name.clone()
        } else {
            key.extract::<String>()?.trim().to_ascii_uppercase()
        };

        let field = self
            .fields
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| PyKeyError::new_err(format!("field not found: {name}")))?;

        let v = py_to_value_with_encoding(value, field, self.encoding)?;
        if self.in_context {
            // Defer until __exit__.
            self.pending.push((name, v));
        } else {
            self.record
                .insert(&self.fields, field.name.as_str(), v)
                .map_err(to_py_error)?;
        }
        Ok(())
    }

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        let items: Vec<PyObject> = self
            .record
            .values()
            .iter()
            .map(|v| value_to_py(py, v, self.encoding))
            .collect::<PyResult<_>>()?;
        Ok(PyList::new(py, items)?.call_method0("__iter__")?.unbind())
    }

    // ── Attribute access (record.fieldname) ─────────────────────────

    fn __getattr__<'py>(&self, py: Python<'py>, name: &str) -> PyResult<PyObject> {
        let upper = name.to_ascii_uppercase();
        if let Some((idx, _)) = self
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == upper)
        {
            return value_to_py(py, &self.record.values()[idx], self.encoding);
        }
        // Fall back to normal attribute lookup.
        Err(pyo3::exceptions::PyAttributeError::new_err(format!(
            "no attribute or field named {name:?}"
        )))
    }

    fn __setattr__(&mut self, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let upper = name.to_ascii_uppercase();
        if let Some(field) = self.fields.iter().find(|f| f.name == upper) {
            let v = py_to_value_with_encoding(value, field, self.encoding)?;
            if self.in_context {
                self.pending.push((upper, v));
            } else {
                self.record
                    .insert(&self.fields, field.name.as_str(), v)
                    .map_err(to_py_error)?;
            }
            return Ok(());
        }
        // Allow normal Python attribute setting for non-field names.
        Err(pyo3::exceptions::PyAttributeError::new_err(format!(
            "no field named {name:?}; use record[\"{name}\"] to set a non-field attribute"
        )))
    }

    // ── Context-manager protocol (`with record: ...`) ────────────────

    fn __enter__(mut slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        slf.in_context = true;
        slf.pending.clear();
        slf
    }

    fn __exit__(
        &mut self,
        _exc_type: &Bound<'_, PyAny>,
        _exc_val: &Bound<'_, PyAny>,
        _exc_tb: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.in_context = false;
        // Apply all pending mutations.
        for (name, value) in self.pending.drain(..) {
            self.record
                .insert(&self.fields, &name, value)
                .map_err(to_py_error)?;
        }
        Ok(false) // do not suppress exceptions
    }

    // ── Helpers ──────────────────────────────────────────────────────

    #[getter]
    fn deleted(&self) -> bool {
        self.record.is_deleted()
    }

    #[setter]
    fn set_deleted(&mut self, value: bool) {
        self.record.set_deleted(value);
    }

    fn as_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        record_to_dict(py, &self.fields, &self.record, self.encoding)
    }

    fn keys(&self) -> Vec<String> {
        self.fields.iter().map(|f| f.name.clone()).collect()
    }

    fn values<'py>(&self, py: Python<'py>) -> PyResult<Vec<PyObject>> {
        self.record
            .values()
            .iter()
            .map(|v| value_to_py(py, v, self.encoding))
            .collect()
    }

    fn items<'py>(&self, py: Python<'py>) -> PyResult<Vec<(String, PyObject)>> {
        self.fields
            .iter()
            .zip(self.record.values())
            .map(|(f, v)| Ok((f.name.clone(), value_to_py(py, v, self.encoding)?)))
            .collect()
    }

    fn __repr__(&self) -> String {
        let pairs: Vec<String> = self
            .fields
            .iter()
            .zip(self.record.values())
            .map(|(f, v)| format!("{}={:?}", f.name, v))
            .collect();
        format!("Record({})", pairs.join(", "))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PyTable
// ─────────────────────────────────────────────────────────────────────────────

#[pyclass(name = "Table")]
pub struct PyTable {
    inner: Table,
    default_filename: Option<String>,
    on_disk: bool,
    status: TableStatus,
    /// Resolved encoding from the `codepage` parameter or the DBF header mark.
    encoding: Option<&'static encoding_rs::Encoding>,
}

#[pymethods]
impl PyTable {
    #[new]
    #[pyo3(signature = (filename, field_specs=None, on_disk=true, dbf_type=None, codepage=None))]
    fn new<'py>(
        filename: String,
        field_specs: Option<&Bound<'py, PyAny>>,
        on_disk: bool,
        dbf_type: Option<String>,
        codepage: Option<String>,
    ) -> PyResult<Self> {
        // Resolve encoding from the explicit `codepage` parameter.
        let explicit_encoding = match &codepage {
            Some(name) => {
                // Accept both a name like "cp1252" or an integer-string like "0xC8" / "200".
                if let Ok(num) = u8::from_str_radix(name.trim_start_matches("0x"), 16)
                    .or_else(|_| name.trim().parse::<u8>())
                {
                    codepage::encoding_for_mark(num)
                } else {
                    let mark = codepage::mark_for_name(name).ok_or_else(|| {
                        PyValueError::new_err(format!(
                            "unknown codepage {name:?}; use e.g. 'cp1252', 'windows-1251', or the numeric mark byte"
                        ))
                    })?;
                    codepage::encoding_for_mark(mark)
                }
            }
            None => None,
        };

        let inner = match field_specs {
            Some(specs) => {
                let kind = dbf_type_to_kind(dbf_type.as_deref())?;
                let parsed_specs = if let Ok(specs_str) = specs.extract::<String>() {
                    crate::spec::FieldSpec::parse_many(&specs_str).map_err(to_py_error)?
                } else if let Ok(specs_list) = specs.extract::<Bound<'py, PyList>>() {
                    let mut list = Vec::new();
                    for item in specs_list.iter() {
                        let dict = item.extract::<Bound<'py, PyDict>>()?;
                        let name = dict.get_item("name")?.unwrap().extract::<String>()?;
                        let type_code = dict.get_item("type_code")?.unwrap().extract::<String>()?;
                        let length = dict.get_item("length")?.unwrap().extract::<u8>()?;
                        let decimals = dict.get_item("decimals")?.unwrap().extract::<u8>()?;
                        let nullable = dict.get_item("nullable")?.unwrap().extract::<bool>()?;
                        let binary = dict.get_item("binary")?.unwrap().extract::<bool>()?;

                        let field_type = crate::header::FieldType::from_byte(
                            type_code.as_bytes().first().copied().unwrap_or(b'C'),
                        )
                        .map_err(to_py_error)?;

                        list.push(crate::spec::FieldSpec {
                            name,
                            field_type,
                            length,
                            decimals,
                            nullable,
                            binary,
                        });
                    }
                    list
                } else {
                    return Err(PyValueError::new_err(
                        "field_specs must be a string or a list of dicts",
                    ));
                };

                Table::from_specs(parsed_specs, kind).map_err(to_py_error)?
            }
            None => Table::open(&filename).map_err(to_py_error)?,
        };

        // If no explicit encoding was given, fall back to what's in the DBF header.
        let encoding =
            explicit_encoding.or_else(|| codepage::encoding_for_mark(inner.header().code_page.0));

        Ok(Self {
            inner,
            default_filename: Some(filename),
            on_disk,
            status: TableStatus::Closed,
            encoding,
        })
    }

    // ── Existing getters (unchanged) ──────────────────────────────────

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

    /// Return the active encoding label (e.g. `"CP1252"`) or `None` if
    /// the file has no code-page mark and no explicit encoding was given.
    #[getter]
    fn codepage(&self) -> Option<String> {
        self.encoding.map(|enc| enc.name().to_string())
    }

    // ── Records access ────────────────────────────────────────────────

    fn records<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        self.require_open()?;
        let items = self
            .inner
            .records()
            .iter()
            .map(|record| record_to_dict(py, self.inner.fields(), record, self.encoding))
            .collect::<PyResult<Vec<_>>>()?;
        PyList::new(py, items)
    }

    /// Like `records()` but returns `PyRecord` objects with attribute access.
    fn record_objects<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        self.require_open()?;
        let items = self
            .inner
            .records()
            .iter()
            .map(|record| {
                let py_record = PyRecord {
                    record: record.clone(),
                    fields: self.inner.fields().to_vec(),
                    encoding: self.encoding,
                    in_context: false,
                    pending: Vec::new(),
                };
                Py::new(py, py_record)
            })
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
        let record =
            self.inner.records().get(index).ok_or_else(|| {
                PyKeyError::new_err(format!("record index out of range: {index}"))
            })?;
        record_to_dict(py, self.inner.fields(), record, self.encoding)
    }

    /// Return a single record as a `PyRecord` object.
    fn record<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Py<PyRecord>> {
        self.require_open()?;
        let record =
            self.inner.records().get(index).ok_or_else(|| {
                PyKeyError::new_err(format!("record index out of range: {index}"))
            })?;
        Py::new(
            py,
            PyRecord {
                record: record.clone(),
                fields: self.inner.fields().to_vec(),
                encoding: self.encoding,
                in_context: false,
                pending: Vec::new(),
            },
        )
    }

    // ── Writing ───────────────────────────────────────────────────────

    fn append(&mut self, row: &Bound<'_, PyAny>) -> PyResult<()> {
        self.require_read_write()?;
        let mut record = self.inner.new_record();
        if let Ok(dict) = row.downcast::<PyDict>() {
            for field in self.inner.fields() {
                if let Some(value) = dict.get_item(field.name.as_str())? {
                    let converted =
                        py_to_value_with_encoding(value.as_any(), field, self.encoding)?;
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
                    let converted =
                        py_to_value_with_encoding(value.bind(py), field, self.encoding)?;
                    record
                        .insert(self.inner.fields(), &field.name, converted)
                        .map_err(to_py_error)?;
                }
                Ok(())
            })?;
        } else if let Ok(py_record) = row.extract::<PyRef<'_, PyRecord>>() {
            // Accept a PyRecord directly (e.g. copying between tables).
            record = py_record.record.clone();
        } else {
            return Err(PyTypeError::new_err(
                "append expects a dict, a sequence, or a Record object",
            ));
        }
        self.inner.push_record(record).map_err(to_py_error)
    }

    // ── pack() ────────────────────────────────────────────────────────

    /// Remove all records that are marked as deleted.  Call `write()` or
    /// close the table to persist the change.
    fn pack(&mut self) -> PyResult<()> {
        self.require_read_write()?;
        self.inner.pack();
        Ok(())
    }

    // ── Schema modification ───────────────────────────────────────────

    /// Add one or more fields to the table.
    ///
    /// ```python
    /// table.add_fields("score N(6,2); notes M")
    /// ```
    fn add_fields(&mut self, specs: &str) -> PyResult<()> {
        self.require_read_write()?;
        self.inner.add_fields(specs).map_err(to_py_error)
    }

    /// Remove one or more fields by name.
    ///
    /// ```python
    /// table.remove_fields(["SCORE", "NOTES"])
    /// ```
    fn remove_fields(&mut self, names: Vec<String>) -> PyResult<()> {
        self.require_read_write()?;
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        self.inner.remove_fields(&refs).map_err(to_py_error)
    }

    /// Rename a field (case-insensitive).
    fn rename_field(&mut self, old_name: &str, new_name: &str) -> PyResult<()> {
        self.require_read_write()?;
        self.inner
            .rename_field(old_name, new_name)
            .map_err(to_py_error)
    }

    // ── Open / close ──────────────────────────────────────────────────

    #[pyo3(signature = (mode=None))]
    fn open<'py>(mut slf: PyRefMut<'py, Self>, mode: Option<&str>) -> PyRefMut<'py, Self> {
        slf.status = match mode {
            Some("r") | Some("rb") | Some("read-only") | Some("read_only") => TableStatus::ReadOnly,
            _ => TableStatus::ReadWrite,
        };
        slf
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

    // ── Context manager ───────────────────────────────────────────────

    fn __enter__(mut slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        if slf.status == TableStatus::Closed {
            slf.status = TableStatus::ReadWrite;
        }
        slf
    }

    fn __exit__(
        &mut self,
        _exc_type: &Bound<'_, PyAny>,
        _exc_val: &Bound<'_, PyAny>,
        _exc_tb: &Bound<'_, PyAny>,
    ) -> PyResult<bool> {
        self.close()?;
        Ok(false)
    }

    // ── Sequence / mapping protocol ───────────────────────────────────

    fn __getitem__<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Bound<'py, PyDict>> {
        self.row(py, index)
    }

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        Ok(self.records(py)?.call_method0("__iter__")?.unbind())
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

    // ── new / clone ───────────────────────────────────────────────────

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
                "the 'default_data_types' parameter is not yet supported",
            ));
        }
        let table = match field_specs {
            Some(specs) => {
                let kind = dbf_type_to_kind(dbf_type)?;
                Table::from_specs(
                    crate::spec::FieldSpec::parse_many(&specs).map_err(to_py_error)?,
                    kind,
                )
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
            encoding: self.encoding,
        })
    }

    #[pyo3(signature = (field=None))]
    fn structure(&self, field: Option<&str>) -> PyResult<String> {
        match field {
            Some(name) => {
                let info = self.inner.field_info(name).map_err(to_py_error)?;
                let mut spec = field_spec_string(info);
                if info.is_nullable() {
                    spec.push_str(" null");
                }
                if info.is_binary() {
                    spec.push_str(" BINARY");
                }
                Ok(spec)
            }
            None => Ok(self.inner.structure()),
        }
    }

    // ── Guards ────────────────────────────────────────────────────────

    fn require_open(&self) -> PyResult<()> {
        if self.status == TableStatus::Closed {
            Err(PyRuntimeError::new_err(
                "table is closed; call open() before reading",
            ))
        } else {
            Ok(())
        }
    }

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

// ─────────────────────────────────────────────────────────────────────────────
// Module registration
// ─────────────────────────────────────────────────────────────────────────────

#[pymodule]
fn fastdbf(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyTable>()?;
    module.add_class::<PyRecord>()?;
    module.add_class::<TableStatus>()?;
    module.add_class::<TableLocation>()?;
    // Legacy string constants.
    module.add("CLOSED", CLOSED)?;
    module.add("READ_ONLY", READ_ONLY)?;
    module.add("READ_WRITE", READ_WRITE)?;
    module.add("IN_MEMORY", IN_MEMORY)?;
    module.add("ON_DISK", ON_DISK)?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────────────────────────────────────

fn field_to_dict<'py>(py: Python<'py>, field: &FieldDescriptor) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("name", field.name.as_str())?;
    dict.set_item("type", format!("{:?}", field.field_type))?;
    dict.set_item("type_code", (field.field_type.symbol() as char).to_string())?;
    dict.set_item("length", field.length)?;
    dict.set_item("decimals", field.decimals)?;
    dict.set_item("offset", field.offset)?;
    dict.set_item("nullable", field.is_nullable())?;
    dict.set_item("binary", field.is_binary())?;
    Ok(dict)
}

fn record_to_dict<'py>(
    py: Python<'py>,
    fields: &[FieldDescriptor],
    record: &Record,
    encoding: Option<&'static encoding_rs::Encoding>,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for (field, value) in fields.iter().zip(record.values()) {
        dict.set_item(field.name.as_str(), value_to_py(py, value, encoding)?)?;
    }
    dict.set_item("_deleted", record.is_deleted())?;
    Ok(dict)
}

fn value_to_py(
    py: Python<'_>,
    value: &Value,
    encoding: Option<&'static encoding_rs::Encoding>,
) -> PyResult<PyObject> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Character(text) => Ok(text.into_pyobject(py)?.unbind().into()),
        Value::Numeric(number) => Ok(number.into_pyobject(py)?.unbind().into()),
        Value::Logical(value) => Ok(value.into_pyobject(py)?.unbind()),
        Value::Date(Some(date)) => Ok(date_to_iso(*date).into_pyobject(py)?.unbind().into()),
        Value::Date(None) => Ok(py.None()),
        Value::Integer(number) => Ok(number.into_pyobject(py)?.unbind().into()),
        Value::Double(number) => Ok(number.into_pyobject(py)?.unbind().into()),
        Value::DateTime(Some(value)) => {
            Ok(datetime_to_iso(*value).into_pyobject(py)?.unbind().into())
        }
        Value::DateTime(None) => Ok(py.None()),
        Value::Currency(number) => Ok(number.into_pyobject(py)?.unbind().into()),
        Value::Memo(bytes) => {
            let text = crate::codepage::decode_bytes(bytes, encoding);
            Ok(text.into_pyobject(py)?.unbind().into())
        }
        Value::Binary(bytes) => Ok(bytes.into_pyobject(py)?.unbind()),
    }
}

/// `py_to_value` with an optional encoding for Character decoding.
fn py_to_value_with_encoding(
    value: &Bound<'_, PyAny>,
    field: &FieldDescriptor,
    encoding: Option<&'static encoding_rs::Encoding>,
) -> PyResult<Value> {
    if value.is_none() {
        return Ok(Value::Null);
    }
    match field.field_type {
        crate::header::FieldType::Character => {
            // Accept both str and bytes.
            if let Ok(text) = value.extract::<String>() {
                // Validate that the string can be encoded in the target encoding.
                if let Some(enc) = encoding {
                    let (_, _, had_errors) = enc.encode(&text);
                    if had_errors {
                        return Err(PyValueError::new_err(format!(
                            "string contains characters that cannot be encoded in {:?}",
                            enc.name()
                        )));
                    }
                }
                Ok(Value::Character(text))
            } else if let Ok(bytes) = value.extract::<Vec<u8>>() {
                let text = codepage::decode_bytes(&bytes, encoding);
                Ok(Value::Character(text))
            } else {
                Err(PyTypeError::new_err("Character field expects str or bytes"))
            }
        }
        crate::header::FieldType::Date => {
            let raw = value.extract::<String>()?;
            let normalized = raw.replace('-', "");
            Ok(Value::Date(
                Date::parse_ymd(&normalized).map_err(to_py_error)?,
            ))
        }
        crate::header::FieldType::Logical => Ok(Value::Logical(Some(value.extract::<bool>()?))),
        crate::header::FieldType::Numeric | crate::header::FieldType::Float => {
            Ok(Value::Numeric(value.extract::<f64>()?))
        }
        crate::header::FieldType::Integer => Ok(Value::Integer(value.extract::<i32>()?)),
        crate::header::FieldType::Double => Ok(Value::Double(value.extract::<f64>()?)),
        crate::header::FieldType::Currency => Ok(Value::Currency(value.extract::<i64>()?)),
        crate::header::FieldType::Memo => {
            if let Ok(text) = value.extract::<String>() {
                if let Some(enc) = encoding {
                    let (encoded, _, _) = enc.encode(&text);
                    Ok(Value::Memo(encoded.into_owned()))
                } else {
                    Ok(Value::Memo(text.into_bytes()))
                }
            } else if let Ok(bytes) = value.extract::<Vec<u8>>() {
                Ok(Value::Memo(bytes))
            } else {
                Err(PyTypeError::new_err("Memo field expects str or bytes"))
            }
        }
        crate::header::FieldType::General | crate::header::FieldType::Picture => {
            if let Ok(bytes) = value.extract::<Vec<u8>>() {
                Ok(Value::Binary(bytes))
            } else {
                Err(PyTypeError::new_err("General/Picture field expects bytes"))
            }
        }
        crate::header::FieldType::NullFlags => Err(PyTypeError::new_err(
            "internal null-flag field cannot be assigned from Python",
        )),
        crate::header::FieldType::DateTime => {
            let raw = value.extract::<String>()?;
            let (date_part, time_part) = raw.split_once('T').ok_or_else(|| {
                PyValueError::new_err("datetime must be ISO-like, e.g. 2024-01-31T12:34:56.000")
            })?;
            let date = Date::parse_ymd(&date_part.replace('-', ""))
                .map_err(to_py_error)?
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
            "db3" | "dbase3" => Ok(Some(crate::header::DbfKind::DBase3)),
            "db3_memo" | "dbase3withmemo" => Ok(Some(crate::header::DbfKind::DBase3WithMemo)),
            "fp" | "foxpro2withmemo" => Ok(Some(crate::header::DbfKind::FoxPro2WithMemo)),
            "vfp" | "visualfoxpro" => Ok(Some(crate::header::DbfKind::VisualFoxPro)),
            "vfp_auto" | "visualfoxproautoincrement" => {
                Ok(Some(crate::header::DbfKind::VisualFoxProAutoIncrement))
            }
            "vfp_var" | "visualfoxprovar" => Ok(Some(crate::header::DbfKind::VisualFoxProVar)),
            "db4" | "dbase4withmemo" => Ok(Some(crate::header::DbfKind::DBase4WithMemo)),
            other => Err(PyValueError::new_err(format!(
                "unsupported dbf_type {other:?}; use e.g. 'db3', 'vfp', 'fp', or the full type name"
            ))),
        },
    }
}

fn field_spec_string(info: &FieldDescriptor) -> String {
    use crate::header::FieldType;
    match info.field_type {
        FieldType::Character => format!("{} C({})", info.name, info.length),
        FieldType::Numeric => format!("{} N({},{})", info.name, info.length, info.decimals),
        FieldType::Float => format!("{} F({},{})", info.name, info.length, info.decimals),
        FieldType::Date => format!("{} D", info.name),
        FieldType::Logical => format!("{} L", info.name),
        FieldType::Memo => format!("{} M", info.name),
        FieldType::General => format!("{} G", info.name),
        FieldType::Picture => format!("{} P", info.name),
        FieldType::NullFlags => format!("{} 0", info.name),
        // Fixed-size types: include length+decimals only when decimals > 0
        FieldType::Integer => {
            if info.decimals > 0 {
                format!("{} I({},{})", info.name, info.length, info.decimals)
            } else {
                format!("{} I", info.name)
            }
        }
        FieldType::Double => {
            if info.decimals > 0 {
                format!("{} B({},{})", info.name, info.length, info.decimals)
            } else {
                format!("{} B", info.name)
            }
        }
        FieldType::DateTime => {
            if info.decimals > 0 {
                format!("{} T({},{})", info.name, info.length, info.decimals)
            } else {
                format!("{} T", info.name)
            }
        }
        FieldType::Currency => {
            if info.decimals > 0 {
                format!("{} Y({},{})", info.name, info.length, info.decimals)
            } else {
                format!("{} Y", info.name)
            }
        }
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
            format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}")
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

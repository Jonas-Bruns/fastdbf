# Changelog

## 0.1.0

Initial public package baseline.

Highlights:

- Rust core for DBF header parsing, record parsing, and file writing
- Python package exposure through `PyO3`, `maturin`, and `uv`
- Python `Table(...)` API for reading and writing DBF files
- field metadata inspection through `table.fields()`
- direct row loading via `read_dbf(...)`
- row append support from dictionaries and tuples
- Visual FoxPro-style nullable field support through null-flag storage
- documentation and example usage for Python and pandas workflows

Known limitations:

- no full memo file support yet
- not yet fully API-compatible with the original `dbf` package
- no advanced indexing or relation support yet

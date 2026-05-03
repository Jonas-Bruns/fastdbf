# Changelog

## 0.5.2

- **Fix**: Support for whole-number floats in `Integer` (I) and `Currency` (Y) fields. This prevents `TypeError` when Pandas converts integer columns to float due to the presence of `NaN`s.

## 0.5.1

- **Fix**: Support for `NaN` (float) and Pandas `NA` values in Character and Memo fields. These values are now correctly treated as `Null` instead of raising a `PyTypeError`.
- **Testing**: Added `tests/test_nan_handling.py` to ensure robust handling of various null-like values across field types.

## 0.5.0

- **Feature**: Native type mapping for Pandas/Arrow integration.
  - `Date` (D) fields now return `datetime.date` objects instead of strings.
  - `DateTime` (T) fields now return `datetime.datetime` objects instead of ISO strings.
  - `Numeric(N, 0)` fields (integer-like) are now mapped to Arrow `Int64` / Python `int` instead of `Float64` / `float`. This fixes the common issue of nullable integers being silently cast to `float` in Pandas.
- **Feature**: `to_arrow()` now emits correct Arrow types (`Date32`, `Timestamp(ms)`, `Int64`) for seamless `.to_pandas()` interoperability — no manual dtype-casting needed.
- **Feature**: `extend_arrow()` accepts incoming `Date32`, `Timestamp` and `Int64` columns and converts them back to DBF values reliably.
- **Feature**: `append(dict)` is now **case-insensitive** — keys like `"name"`, `"NAME"`, or `"Name"` all map to the same DBF field.
- **Feature**: `append()` natively accepts `datetime.date`, `datetime.datetime`, `pandas.Timestamp`, and `numpy` integer/float types without manual casting.

## 0.4.4

- Fix: Resolved TOML syntax error in `pyproject.toml` regarding dependencies location.

## 0.4.3


- Feature: Introduced custom exception classes (`FastDbfError`, `DbfFormatError`, `UnsupportedDbfTypeError`).
- Feature: Timestamps are now automatically written to DBF headers during modification.
- Testing: Added full integration test coverage for previously untested methods (`tests/test_full_api.py`).
- CI/CD: Enforced releases only from the `main` branch and upgraded actions to Node 24.

## 0.4.2


- CI/CD: Final verification of PyPI deployment.

## 0.4.1

- CI/CD: Enable automated GitHub Releases and PyPI deployment.

## 0.4.0

- **Breaking Change**: Removed `on_disk` parameter. Files are now always flushed to disk on close.
- Performance: Cleaned up unused code, formatted the codebase for standard CI pipelines.
- Documentation: Added comprehensive performance comparison for row-by-row, columnar, and Arrow I/O.

## 0.3.1

- Fix: Correctly map Double and Currency fields in `extend_arrow` (fixing type mismatch error during write).

## 0.3.0

- Feature: Zero-copy data transfer using Apache Arrow. Added `to_arrow()` and `extend_arrow()` for lightning-fast Arrow RecordBatches.

## 0.2.1

- Performance: Parallelized DBF reading using `memmap2` and `rayon`, enabling multi-core record parsing.

## 0.2.0

- Feature: Added `to_columns()` and `extend_columns()` methods to `PyTable` for high-performance bulk data transfer with Pandas (columnar I/O).

## 0.1.9

- Fix: Adjusted `_NullFlags` casing to match VFP specifications.

## 0.1.8

- Fix: Restored mandatory 263-byte VFP DBC backlink padding in header.

## 0.1.7

- Feature: `field_specs` now accepts the dictionary output of `Table.fields()` directly, eliminating the need to manually format strings for table copying.

## 0.1.6

- Fix: Omit the 263-byte backlink padding when writing VFP tables to ensure maximum compatibility with standard DBF viewers that expect the terminator immediately before the data records.

## 0.1.5

- Fix: Set the correct flags byte (`0x05`) for the hidden `_NULLFLAGS` field descriptor in VFP tables to ensure full compatibility with third-party tooling.
- Feature: `dbf_type` now accepts full type names like `"VisualFoxPro"` or `"DBase3"`, allowing dynamic roundtripping via `Table.kind`.

## 0.1.4

- Fix: Ensure decimals are preserved for fixed-size field types like `Double (B)` when calling `Table.structure()` and converting back to specs.

## 0.1.3

- Fix: VFP tables now correctly write the 263-byte DBC backlink area after the header terminator, fixing compatibility with the Python `dbf` package and other VFP-aware readers.
- Fix: `header_length` is now computed correctly for VFP tables (includes backlink size), preventing files from being unreadable by third-party tools.

## 0.1.2

- Feature: Parse and represent `BINARY` flag in DBF files and field specs (matching Python `dbf` behavior).
- Fix: Prevent DBF file corruption by correctly padding/sizing header files when rewriting a DBF after `Table.close()`.
- Refactor: Make Python `Table.open(mode=...)` return `self` to support context manager usages.
- Refactor: Remove unnecessary module-level Python API functions (`read_dbf`, `open_table`, `create_table`, etc) and enforce object-oriented `Table` usage.

## 0.1.1

- Setup automated GitHub Actions CI release pipeline.

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

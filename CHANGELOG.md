# Changelog

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

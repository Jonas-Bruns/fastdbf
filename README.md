# fastdbf

`fastdbf` is a Rust-based reimplementation of the core ideas from [`ethanfurman/dbf`](https://github.com/ethanfurman/dbf), exposed as a Python package through `PyO3`.

The current focus is a practical Python-first package for reading and writing `.dbf` files, including Visual FoxPro-style nullable fields.

## Status

This project is in an early but usable state.

Implemented today:

- read `.dbf` files from Python
- inspect field metadata and field types
- create new tables from DBF-style field specifications
- append rows from dictionaries or tuples
- write tables back to disk
- Visual FoxPro null-flag support for nullable fields
- a Rust core with test coverage for read/write roundtrips

Not implemented yet:

- full memo file support (`.dbt` / `.fpt`)
- full original `dbf` compatibility surface
- record objects with attribute access like `record.name`
- indexing, relations, PQL, and most advanced helpers from the original project

## Installation

Create or sync the development environment with `uv`:

```bash
uv sync
```

Install into the current environment with `uv`:

```bash
uv pip install .
```

Editable install:

```bash
uv pip install -e .
```

Run tests:

```bash
uv run pytest
```

Build a wheel:

```bash
uv build
```

## Quick Start

Read an existing DBF file:

```python
import fastdbf

table = fastdbf.Table("people.dbf")
table.open()

print(table.kind)
print(table.field_names)
print(table.record_count)
print(table.row(0))

for field in table.fields():
    print(field["name"], field["type_code"], field["nullable"])

for row in table:
    print(row)

table.close()
```

Create and write a new DBF file:

```python
import fastdbf

table = fastdbf.Table(
    "people.dbf",
    "name C(25) null; age N(3,0) null; birth D null; active L null",
    on_disk=False,
    dbf_type="vfp",
)
table.open()

table.append({
    "NAME": "Spunky",
    "AGE": 23,
    "BIRTH": "1989-07-23",
    "ACTIVE": True,
})

table.append({
    "NAME": None,
    "AGE": None,
    "BIRTH": None,
    "ACTIVE": None,
})

table.write("people.dbf")
table.close()
```

Read directly into a list of dictionaries:

```python
import fastdbf

rows = fastdbf.read_dbf("people.dbf")
for row in rows:
    print(row)
```

## Field Types

Currently supported field types:

- `C` Character
- `D` Date
- `L` Logical
- `N` Numeric
- `F` Float
- `I` Integer
- `B` Double
- `T` / `@` DateTime
- `Y` Currency
- `M` / `G` / `P` as reference values

Nullable fields are supported through `null` or `nullable` modifiers in the field specification:

```python
"name C(25) null; amount N(10,2) nullable; created T null"
```

Nullable fields should be used with `dbf_type="vfp"` for Visual FoxPro-compatible null flags.

## pandas Example

```python
import pandas as pd
import fastdbf

table = fastdbf.Table("input.dbf")
table.open()

df = pd.DataFrame(table.records())
df["NAME"] = df["NAME"].str.upper()

field_specs = []
for field in table.fields():
    code = field["type_code"]
    nullable = " null" if field["nullable"] else ""
    if code == "C":
        field_specs.append(f'{field["name"]} C({field["length"]}){nullable}')
    elif code in ("N", "F"):
        field_specs.append(
            f'{field["name"]} {code}({field["length"]},{field["decimals"]}){nullable}'
        )
    else:
        field_specs.append(f'{field["name"]} {code}{nullable}')

out = fastdbf.Table(
    "output.dbf",
    "; ".join(field_specs),
    on_disk=False,
    dbf_type="vfp" if any(f["nullable"] for f in table.fields()) else "db3",
)
out.open()

for row in df.to_dict(orient="records"):
    out.append(row)

out.write("output.dbf")
out.close()
table.close()
```

## Documentation

Full Python API documentation:

- [docs/PYTHON_API.md](docs/PYTHON_API.md)

Changelog:

- [CHANGELOG.md](CHANGELOG.md)

## Rust Example

```rust
use fastdbf::{Date, Table, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut table = Table::new("name C(25); age N(3,0); birth D; qualified L")?;

    let mut record = table.new_record();
    record.insert(table.fields(), "name", Value::Character("Spunky".into()))?;
    record.insert(table.fields(), "age", Value::Numeric(23.0))?;
    record.insert(table.fields(), "birth", Value::Date(Some(Date::new(1989, 7, 23))))?;
    record.insert(table.fields(), "qualified", Value::Logical(Some(true)))?;
    table.push_record(record)?;

    table.write_to_path("example.dbf")?;
    Ok(())
}
```

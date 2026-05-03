# fastdbf

`fastdbf` is a high-performance Python package for reading and writing `.dbf` files.

Written in **Rust** (using `PyO3`), it provides standard Python bindings designed specifically for **large datasets**, where traditional pure-Python solutions (like the standard `dbf` package) suffer from significant performance bottlenecks.


![Performance Time](benchmarks/benchmark_time.png)
![Performance Speedup](benchmarks/benchmark_speedup.png)



## Status

`fastdbf` is fully production-ready for core data exchange workloads.

### Supported Features

- **High-Performance I/O**: Lightning fast reads and writes of standard `.dbf` files.
- **Zero-Copy Bulk Transfers**: Native Apache Arrow integration (`to_arrow()`, `extend_arrow()`) for high-speed exchange with Pandas/Polars.
- **Visual FoxPro Support**: Direct handling of VFP `.dbf` flavors, including mandatory null-flag layouts.
- **Memo Fields**: Automatic management of companion `.fpt` memo files for unbounded strings.
- **Native Type Mapping**: Correct Python/Arrow types for dates, datetimes, and integers — no manual casting needed.
- **Strict Typing**: Clear data mappings with custom exception classes (`DbfFormatError`).

### Not Implemented yet

- **In-place Schema Modification**: Dynamic addition/removal of columns on pre-written tables (currently raises `UnsupportedDbfTypeError`).
- **Advanced Engine Tools**: Indexing, cross-table relationships, or built-in query paradigms.



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

with fastdbf.Table("people.dbf").open("r") as table:
    print(table.kind)
    print(table.field_names)
    print(table.record_count)
    print(table.row(0))

    for field in table.fields():
        print(field["name"], field["type_code"], field["nullable"])

    for row in table:
        print(row)
```

Create and write a new DBF file:

```python
import fastdbf
from datetime import date, datetime

specs = "name C(25) null; age N(3,0) null; birth D null; created T null; active L null"
with fastdbf.Table("people.dbf", specs, dbf_type="vfp") as table:
    table.append({
        "name": "Alice",       # case-insensitive keys
        "age": 30,
        "birth": date(1994, 5, 20),
        "created": datetime(2024, 1, 1, 12, 0),
        "active": True,
    })

    table.append({
        "NAME": None,
        "AGE": None,
        "BIRTH": None,
        "CREATED": None,
        "ACTIVE": None,
    })
```



## Type Mapping

fastdbf maps DBF field types to native Python and Arrow types, so no manual casting is needed in either direction.

### DBF → Python / Arrow

| DBF Field | DBF Type Code | Python type | Arrow type |
|:---|:---:|:---|:---|
| Character | `C` | `str` | `Utf8` |
| Numeric (integer) | `N(n,0)` | `int` | `Int64` |
| Numeric (decimal) | `N(n,k)` | `float` | `Float64` |
| Date | `D` | `datetime.date` | `Date32` |
| DateTime | `T` | `datetime.datetime` | `Timestamp(ms)` |
| Logical | `L` | `bool` | `Boolean` |
| Integer | `I` | `int` | `Int32` |
| Double | `B` | `float` | `Float64` |

### Python / Pandas → DBF (writing)

`append()` and `extend_arrow()` both accept the types listed above, **plus** Pandas-native types without any manual casting:

| Input type | DBF field written |
|:---|:---|
| `datetime.date` | `D` (Date) |
| `datetime.datetime` | `T` (DateTime) |
| `pandas.Timestamp` | `T` (DateTime) |
| `numpy.int64` / `int` | `N(n,0)` or `I` |
| `numpy.float64` / `float` | `N(n,k)` or `B` |

> **Note**: `append(dict)` is **case-insensitive** — `"name"`, `"NAME"`, and `"Name"` all resolve to the same DBF field.



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

## Pandas / Arrow Integration

### Reading a DBF into Pandas (recommended)

Using the Arrow interface gives correct types directly — no extra dtype conversion needed:

```python
import fastdbf
import pyarrow as pa

with fastdbf.Table("data.dbf").open("r") as table:
    df = pa.record_batch(table.to_arrow()).to_pandas()

# Result:
# - Date fields    → datetime64 (via object column of datetime.date)
# - DateTime fields → datetime64[ms]
# - Numeric(N,0)   → Int64 (no silent float coercion!)
# - Logical        → bool
```

### Writing a Pandas DataFrame to DBF

**Method A: Arrow (fastest, recommended for large DataFrames)**

```python
import fastdbf
import pyarrow as pa

batch = pa.RecordBatch.from_pandas(df)

with fastdbf.Table("output.dbf", field_specs="NAME C(20); AGE N(10,0); BIRTH D; CREATED T") as table:
    table.extend_arrow(batch)
```

**Method B: Row-by-row `append` (no dependencies beyond fastdbf)**

```python
import fastdbf

with fastdbf.Table("output.dbf", field_specs="NAME C(20); AGE N(10,0); BIRTH D; CREATED T") as table:
    for _, row in df.iterrows():
        table.append(row.to_dict())   # pandas.Timestamp, numpy types accepted natively
```

### The `_deleted` column

All read methods expose a `_deleted: bool` column. This reflects the **DBF soft-delete flag** — a standard DBF concept where records are logically marked as deleted (with a `*` marker byte) but remain physically in the file until a `PACK` operation removes them.

```python
# Skip deleted records when reading:
df = df[~df["_deleted"]]

# Mark a record as deleted:
with table.record(0) as rec:
    rec.set_deleted(True)

# Physically remove all deleted records:
table.pack()
```

## Performance & Columnar I/O (Arrow)

For maximum performance, especially with large datasets, `fastdbf` provides columnar read/write interfaces that avoid the high overhead of Python object allocation.

### 1. Apache Arrow Interface (Zero-Copy) — **Fastest**
Leverages the Arrow PyCapsule Interface to exchange data directly between Rust and Pandas / Polars / PyArrow without copying.

**Read into Pandas via Arrow:**
```python
import fastdbf
import pyarrow as pa

with fastdbf.Table("data.dbf").open("r") as table:
    # Arrow Batch -> Pandas DataFrame
    df = pa.record_batch(table.to_arrow()).to_pandas()
```

**Write from Pandas via Arrow:**
```python
import fastdbf
import pyarrow as pa

# Create Arrow batch from DataFrame
batch = pa.RecordBatch.from_pandas(df)

with fastdbf.Table("output.dbf", field_specs="NAME C(20); AGE N(10,2)") as table:
    table.extend_arrow(batch)
```

### 2. Columnar Interface (`to_columns`, `extend_columns`)
Reads/writes data as a dictionary of lists (one list per column). Faster than row-by-row processing but still bound by GIL limits.

**Bulk Columnar Read:**
```python
import pandas as pd
import fastdbf

with fastdbf.Table("data.dbf").open("r") as table:
    cols = table.to_columns()
    df = pd.DataFrame(cols)
```

**Bulk Columnar Write:**
```python
import pandas as pd
import fastdbf

# Drop internal meta-columns like '_deleted' if present
clean_data = {col: df[col].tolist() for col in df.columns if col != "_deleted"}

with fastdbf.Table("output.dbf", field_specs="NAME C(20); AGE N(10,2)") as table:
    table.extend_columns(clean_data)
```

---

### Overview: Read/Write Methods compared

| Method | Implementation | Pros |
| :--- | :--- | :--- |
| **Row-by-Row** | `table.row()`, `table.append()` | Easiest to use |
| **Bulk Columnar**| `to_columns()`, `extend_columns()`| No heavy dependencies |
| **Zero-Copy Arrow**| `to_arrow()`, `extend_arrow()` | Direct memory exchange |


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

## License

This project is licensed under the [Apache License 2.0](LICENSE).

# Python API

This document describes the current Python API exposed by `fastdbf`.

The package is inspired by the original [`ethanfurman/dbf`](https://github.com/ethanfurman/dbf) project, but it does not yet provide full API compatibility. Everything listed here is available in the current implementation.

## Import

```python
import fastdbf
```

## Module-Level Functions

### `fastdbf.Table(filename, field_specs=None, dbf_type=None, codepage=None)`

Create a `Table` object.

Behavior:

- If `field_specs` is omitted, the constructor opens an existing DBF file definition from `filename`.
- If `field_specs` is provided, the constructor creates a new table structure.

Parameters:

- `filename: str`
- `field_specs: str | None`
- `dbf_type: str | None`
- `codepage: str | None`

Supported `dbf_type` values:

- `"db3"`
- `"vfp"`
- `"fp"`
- `"db4"`

Examples:

```python
with fastdbf.Table("input.dbf").open("r") as table:
    pass
```

```python
specs = "name C(25); age N(3,0); birth D"
with fastdbf.Table("output.dbf", specs, dbf_type="db3") as table:
    pass
```



## `Table` Properties

### `table.kind`

String representation of the DBF table kind.

Example:

```python
print(table.kind)
```

### `table.field_names`

List of field names.

Example:

```python
print(table.field_names)
```

### `table.record_count`

Number of records currently loaded in the table.

Example:

```python
print(table.record_count)
```

### `table.status`

Return the current table status.

Current values:

- `fastdbf.CLOSED`
- `fastdbf.READ_WRITE`

Example:

```python
print(table.status)
```

### `table.filename`

Return the default filename associated with the table, if available.

Example:

```python
print(table.filename)
```

### `table.location`

Return the logical storage location.

Current values:

- `fastdbf.IN_MEMORY`
- `fastdbf.ON_DISK`

Example:

```python
print(table.location)
```

## `Table` Methods

### `table.open(mode=None)`

Open the table.

This method currently exists mainly for API familiarity with the original package. The `mode` argument is accepted but not yet used functionally.

Example:

```python
table.open()
```

### `table.close()`

Close the table.

`close()` writes the table to its default filename.

Example:

```python
table.close()
```

### `table.fields()`

Return field metadata as a list of dictionaries.

Each field dictionary contains:

- `name`
- `type`
- `type_code`
- `length`
- `decimals`
- `offset`
- `nullable`

Example:

```python
for field in table.fields():
    print(field["name"], field["type_code"], field["nullable"])
```

Sample field dictionary:

```python
{
    "name": "NAME",
    "type": "Character",
    "type_code": "C",
    "length": 25,
    "decimals": 0,
    "offset": 1,
    "nullable": True,
}
```

### `table.records()`

Return all records as a list of dictionaries.

Example:

```python
rows = table.records()
print(rows[0])
```

### `table.row(index)`

Return one record by index.

Example:

```python
row = table.row(0)
print(row["NAME"])
```

### `table.append(row)`

Append a new record.

Accepted input forms:

- dictionary
- tuple
- list

Dictionary example:

```python
table.append({
    "NAME": "Spunky",
    "AGE": 23,
    "BIRTH": "1989-07-23",
    "ACTIVE": True,
})
```

Tuple example:

```python
table.append(("Spunky", 23, "1989-07-23", True))
```

### `table.write(path)`

Write the current table to a DBF file.

Example:

```python
table.write("output.dbf")
```

### `table.structure(field=None)`

Return the field specification string for the whole table or for a single field.

Examples:

```python
print(table.structure())
print(table.structure("NAME"))
```

### `table.to_columns()`

Read all records into a dictionary of Python lists, with one key per column. Faster than row-based iteration.

Example:

```python
import pandas as pd
import fastdbf

with fastdbf.Table("data.dbf").open("r") as table:
    cols = table.to_columns()
    df = pd.DataFrame(cols)
```

### `table.extend_columns(columns)`

Append records given as a dictionary of Python lists.

Example:

```python
import pandas as pd
import fastdbf

# Assuming df ist das Pandas DataFrame
clean_data = {col: df[col].tolist() for col in df.columns if col != "_deleted"}

with fastdbf.Table("output.dbf", "NAME C(20); AGE N(10,2)") as table:
    table.extend_columns(clean_data)
```

### `table.to_arrow()`

Export data as an Arrow `RecordBatch` using zero-copy memory sharing. Extends the Arrow PyCapsule interface.

Example:

```python
import pandas as pd
import pyarrow as pa
import fastdbf

with fastdbf.Table("data.dbf").open("r") as table:
    arrow_batch = table.to_arrow()
    df = pa.Table.from_batches([pa.record_batch(arrow_batch)]).to_pandas()
```

### `table.extend_arrow(batch)`

Append records from an Apache Arrow `RecordBatch`.

Example:

```python
import pyarrow as pa
import fastdbf

batch = pa.RecordBatch.from_pandas(df)
with fastdbf.Table("output.dbf", "NAME C(20); AGE N(10,2)") as table:
    table.extend_arrow(batch)
```

### `table.new(filename=":memory:", default_data_types=None, field_specs=None, dbf_type=None)`

Create a new table using either the current table structure or an explicitly provided `field_specs` string.

Examples:

```python
with table.new("copy.dbf") as copy:
    pass
```

```python
custom = table.new(
    "custom.dbf",
    field_specs="name C(40); age N(3,0)",
    dbf_type="db3",
)
```

## Python Protocol Support

### `len(table)`

Return the number of records.

```python
print(len(table))
```

### `table[index]`

Return a record dictionary by index.

```python
print(table[0])
```

### `for row in table`

Iterate over record dictionaries.

```python
for row in table:
    print(row)
```

## Field Specification Format

Field specifications use DBF-style syntax:

```python
"name C(25); age N(3,0); birth D; active L"
```

Supported field types:

- `C` Character
- `D` Date
- `L` Logical
- `N` Numeric
- `F` Float
- `I` Integer
- `B` Double
- `T` or `@` DateTime
- `Y` Currency
- `M` Memo reference
- `G` General reference
- `P` Picture reference

Nullable modifiers:

- `null`
- `nullable`

Examples:

```python
"name C(25)"
"age N(3,0)"
"birth D null"
"created T nullable"
```

## Python to DBF Type Mapping

### Reading

Current Python return types:

- `C` -> `str`
- `D` -> ISO string `YYYY-MM-DD` or `None`
- `L` -> `True`, `False`, or `None`
- `N`, `F` -> `float` or `None`
- `I` -> `int`
- `B` -> `float`
- `T` / `@` -> ISO string `YYYY-MM-DDTHH:MM:SS.mmm` or `None`
- `Y` -> `int`
- `M`, `G`, `P` -> `int`

### Writing

Current accepted Python input types:

- `C` -> `str` or `None`
- `D` -> `YYYY-MM-DD` or `None`
- `L` -> `bool` or `None`
- `N`, `F` -> numeric value or `None`
- `I` -> `int` or `None` for nullable VFP fields
- `B` -> `float` or `None` for nullable VFP fields
- `T` / `@` -> `YYYY-MM-DDTHH:MM:SS` or `YYYY-MM-DDTHH:MM:SS.mmm` or `None`
- `Y` -> `int` or `None` for nullable VFP fields
- `M`, `G`, `P` -> `int` or `None`

## Nullable Fields

Nullable fields are implemented using Visual FoxPro-compatible null flags.

Example:

```python
specs = "name C(25) null; amount N(10,2) null; when T null; active L null"
with fastdbf.Table("nullable.dbf", specs, dbf_type="vfp") as table:
    table.append({
        "NAME": None,
        "AMOUNT": None,
        "WHEN": None,
        "ACTIVE": None,
    })
```

Check whether a field is nullable:

```python
for field in table.fields():
    print(field["name"], field["nullable"])
```

## pandas Example

```python
import pandas as pd
import fastdbf

with fastdbf.Table("input.dbf").open("r") as table:
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

dbf_type = "vfp" if any(f["nullable"] for f in table.fields()) else "db3"
specs = "; ".join(field_specs)

with fastdbf.Table("output.dbf", specs, dbf_type=dbf_type) as out:
    for row in df.to_dict(orient="records"):
        out.append(row)
```

## Known Gaps Compared to the Original `dbf` Package

- `open(mode)` accepts a mode but does not yet enforce read/write mode behavior
- `codepage` is not yet implemented in the Python layer
- memo files are not fully supported yet
- records are returned as dictionaries, not rich record objects
- helpers such as `dbf.write(...)`, `READ_WRITE`, `Process`, `Templates`, `Index`, and query helpers are not implemented yet

## Module Constants

The module currently exports these compatibility-style constants:

- `fastdbf.CLOSED`
- `fastdbf.READ_ONLY`
- `fastdbf.READ_WRITE`
- `fastdbf.IN_MEMORY`
- `fastdbf.ON_DISK`

## Recommended Usage Patterns

Read an existing file:

```python
with fastdbf.Table("input.dbf").open("r") as table:
    rows = table.records()
```

Write a new DBF file:

```python
specs = "id N(10,0); name C(50); active L"
with fastdbf.Table("output.dbf", specs, dbf_type="db3") as table:
    table.append((1, "Alice", True))
    table.append((2, "Bob", False))
```

Write a nullable Visual FoxPro DBF:

```python
specs = "id I null; name C(50) null; created T null"
with fastdbf.Table("output_nullable.dbf", specs, dbf_type="vfp") as table:
    table.append((None, None, None))
```

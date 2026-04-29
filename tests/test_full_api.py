from pathlib import Path

import pytest

import fastdbf


def test_record_dict_protocols(tmp_path: Path) -> None:
    path = tmp_path / "test.dbf"
    table = fastdbf.Table(str(path), "name C(10); age N(3,0)", dbf_type="db3")
    table.open()
    table.append({"NAME": "Alice", "AGE": 30})

    # Retrieve record object
    rec = table.record(0)

    # __len__
    assert len(rec) == 2

    # keys()
    assert rec.keys() == ["NAME", "AGE"]

    # values()
    vals = rec.values()
    assert len(vals) == 2
    assert vals[0].strip() == "Alice"
    assert vals[1] == 30.0

    # items()
    items = rec.items()
    assert items[0][0] == "NAME"
    assert items[0][1].strip() == "Alice"

    # __getitem__
    assert rec["NAME"].strip() == "Alice"
    assert rec[0].strip() == "Alice"

    table.close()


def test_record_mutations_and_context(tmp_path: Path) -> None:
    path = tmp_path / "test.dbf"
    table = fastdbf.Table(str(path), "name C(10); age N(3,0)", dbf_type="db3")
    table.open()

    # Append using PyRecord
    table.append({"NAME": "Alice", "AGE": 30})
    rec = table.record(0)

    # Context manager usage (modifies the PyRecord object)
    with rec:
        rec["NAME"] = "Bob"
        rec["AGE"] = 40

    # PyRecord updated
    assert rec["NAME"].strip() == "Bob"

    # Test deleted property getter on PyRecord
    assert not rec.deleted

    table.close()


def test_table_columnar_api(tmp_path: Path) -> None:
    path = tmp_path / "test.dbf"
    table = fastdbf.Table(str(path), "name C(10); age N(3,0)", dbf_type="db3")
    table.open()

    # extend_columns
    cols = {
        "NAME": ["Alice", "Bob"],
        "AGE": [30, 40],
    }
    table.extend_columns(cols)
    assert table.record_count == 2

    # to_columns
    out_cols = table.to_columns()
    assert "_deleted" in out_cols
    assert out_cols["NAME"][0].strip() == "Alice"
    assert out_cols["AGE"][0] == 30.0

    table.close()


def test_table_arrow_api(tmp_path: Path) -> None:
    pa = pytest.importorskip("pyarrow")

    path = tmp_path / "test.dbf"
    table = fastdbf.Table(str(path), "name C(10); age N(3,0)", dbf_type="db3")
    table.open()

    # extend_arrow
    df = {"NAME": ["Alice", "Bob"], "AGE": [30.0, 40.0]}
    batch = pa.RecordBatch.from_pydict(df)
    table.extend_arrow(batch)
    assert table.record_count == 2

    # to_arrow
    arrow_batch = pa.record_batch(table.to_arrow())
    assert isinstance(arrow_batch, pa.RecordBatch)
    assert arrow_batch.num_rows == 2

    table.close()


def test_table_pack_and_deleted(tmp_path: Path) -> None:
    path = tmp_path / "test.dbf"
    table = fastdbf.Table(str(path), "name C(10); age N(3,0)", dbf_type="db3")
    table.open()

    # Extend with one deleted record
    cols = {"NAME": ["Alice", "Bob"], "AGE": [30, 40], "_deleted": [True, False]}
    table.extend_columns(cols)
    assert table.record_count == 2

    # Pack
    table.pack()
    assert table.record_count == 1
    assert table.row(0)["NAME"].strip() == "Bob"

    table.close()


def test_table_schema_modification_stubs(tmp_path: Path) -> None:
    path = tmp_path / "test.dbf"
    table = fastdbf.Table(str(path), "name C(10)", dbf_type="db3")
    table.open()

    # Verify stubs raise exception (we check generic Exception if Custom not found)
    with pytest.raises(Exception):
        table.add_fields("age N(3,0)")

    with pytest.raises(Exception):
        table.remove_fields(["NAME"])

    with pytest.raises(Exception):
        table.rename_field("NAME", "NEWNAME")

    table.close()


def test_table_properties_and_helpers(tmp_path: Path) -> None:
    path = tmp_path / "test.dbf"
    table = fastdbf.Table(str(path), "name C(10)", dbf_type="db3")

    # Before open
    assert table.status == fastdbf.TableStatus.Closed
    assert str(table.status) == "closed"

    table.open()
    assert table.status == fastdbf.TableStatus.ReadWrite

    assert table.location == fastdbf.TableLocation.OnDisk
    assert str(table.location) == "on_disk"

    assert table.filename == str(path)

    table.close()

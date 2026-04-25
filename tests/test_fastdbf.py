from pathlib import Path

import fastdbf


def test_create_write_and_read_roundtrip(tmp_path: Path) -> None:
    path = tmp_path / "people.dbf"
    table = fastdbf.Table(
        str(path),
        "name C(25) null; age N(3,0) null; birth D null; active L null",
        on_disk=False,
        dbf_type="vfp",
    )
    table.open()
    table.append(
        {
            "NAME": "Spunky",
            "AGE": 23,
            "BIRTH": "1989-07-23",
            "ACTIVE": True,
        }
    )
    table.append(
        {
            "NAME": None,
            "AGE": None,
            "BIRTH": None,
            "ACTIVE": None,
        }
    )
    table.write(str(path))
    table.close()

    reopened = fastdbf.Table(str(path))
    reopened.open()
    assert reopened.field_names == ["NAME", "AGE", "BIRTH", "ACTIVE"]
    assert reopened.record_count == 2
    assert reopened.row(0)["NAME"] == "Spunky"
    assert reopened.row(1)["NAME"] is None
    assert reopened.fields()[0]["nullable"] is True
    reopened.close()


def test_create_table_helper_and_structure(tmp_path: Path) -> None:
    path = tmp_path / "numbers.dbf"
    table = fastdbf.create_table(
        "id N(10,0); label C(20)",
        filename=str(path),
        on_disk=False,
        dbf_type="db3",
    )
    table.open()
    table.append((1, "one"))
    assert "ID N(10,0)" in table.structure()
    assert table.structure("LABEL") == "LABEL C(20)"
    table.write(str(path))
    table.close()


def test_module_helpers(tmp_path: Path) -> None:
    csv_path = tmp_path / "sample.csv"
    csv_path.write_text("Alice,10\nBob,20\n", encoding="utf-8")
    table = fastdbf.from_csv(str(csv_path), dbf_type="db3")
    table.open()
    assert fastdbf.field_names(table) == ["F0", "F1"]
    assert len(table.records()) == 2
    table.close()

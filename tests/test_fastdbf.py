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


def test_memo_read_write(tmp_path: Path) -> None:
    path = tmp_path / "memos.dbf"
    table = fastdbf.Table(
        str(path),
        "id N(3,0); notes M null",
        on_disk=False,
        dbf_type="vfp",
    )
    table.open()
    table.append(
        {"ID": 1, "NOTES": "This is a very long memo text that will go into the FPT file!"}
    )
    table.append({"ID": 2, "NOTES": None})
    table.append({"ID": 3, "NOTES": "Another note."})
    table.write(str(path))
    table.close()

    # Verify the companion file exists
    fpt_path = tmp_path / "memos.fpt"
    assert fpt_path.exists()

    reopened = fastdbf.Table(str(path))
    reopened.open()
    assert reopened.record_count == 3
    assert (
        reopened.row(0)["NOTES"] == "This is a very long memo text that will go into the FPT file!"
    )
    assert reopened.row(1)["NOTES"] is None
    assert reopened.row(2)["NOTES"] == "Another note."
    reopened.close()

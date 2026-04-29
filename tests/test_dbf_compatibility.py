from pathlib import Path

import dbf

import fastdbf


def test_roundtrip_with_dbf_package(tmp_path: Path) -> None:
    in_path = tmp_path / "roundtrip_in.dbf"
    out_path = tmp_path / "roundtrip_out.dbf"

    # 1. Create original with dbf package
    t = dbf.Table(
        str(in_path),
        "name C(20); score B BINARY; amount N(10,3) null; active L null",
        dbf_type="vfp",
    )
    t.open(dbf.READ_WRITE)
    t.append(("Test Record", 42.0, 123.456, True))
    t.close()

    # 2. Read with fastdbf and reconstruct specs
    with fastdbf.Table(str(in_path), dbf_type="vfp").open("r") as inp:
        field_specs = []
        for field in inp.fields():
            code = field["type_code"]
            nullable = " null" if field["nullable"] else ""
            binary = " BINARY" if field["binary"] else ""
            name = field["name"]
            length = field["length"]
            decimals = field["decimals"]
            if code == "C":
                field_specs.append(f"{name} C({length}){nullable}{binary}")
            elif code in ("N", "F"):
                field_specs.append(f"{name} {code}({length},{decimals}){nullable}")
            else:
                field_specs.append(f"{name} {code}{nullable}{binary}")

        records = [r.as_dict() for r in inp.record_objects()]

    # 3. Write new file with fastdbf
    with fastdbf.Table(str(out_path), "; ".join(field_specs), dbf_type="vfp").open("rw") as out:
        for rec in records:
            # Remove _deleted if present as append might not like it
            rec.pop("_deleted", None)
            out.append(rec)

    # 4. Verify dbf package can open the fastdbf output
    t2 = dbf.Table(str(out_path))
    t2.open()
    assert len(t2) == 1
    assert t2[0].name.strip() == "Test Record"
    t2.close()


def test_header_comparison_with_dbf_package(tmp_path: Path) -> None:
    dbf_path = tmp_path / "vfp_dbf.dbf"
    fast_path = tmp_path / "vfp_fast.dbf"
    specs = "name C(20); score B BINARY; amount N(10,3) null; active L null"

    # Create with dbf package
    t_dbf = dbf.Table(str(dbf_path), specs, dbf_type="vfp")
    t_dbf.open()
    t_dbf.close()

    # Create with fastdbf
    with fastdbf.Table(str(fast_path), specs, dbf_type="vfp"):
        pass

    # Analyze headers
    def get_header_info(path):
        with open(path, "rb") as f:
            raw = f.read(512)
        return {
            "version": raw[0],
            "header_len": int.from_bytes(raw[8:10], "little"),
            "rec_len": int.from_bytes(raw[10:12], "little"),
        }

    info_dbf = get_header_info(dbf_path)
    info_fast = get_header_info(fast_path)

    assert info_fast["version"] == info_dbf["version"]
    assert info_fast["header_len"] == info_dbf["header_len"]
    assert info_fast["rec_len"] == info_dbf["rec_len"]

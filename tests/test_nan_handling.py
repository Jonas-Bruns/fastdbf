import numpy as np
import pandas as pd

import fastdbf


def test_nan_to_null_conversion(tmp_path):
    dbf_path = str(tmp_path / "test_nan.dbf")

    # DataFrame with various NaN/None values
    df = pd.DataFrame(
        {
            "NAME": ["Alice", np.nan, "Bob"],
            "AGE": [30, None, 25],
            "SCORE": [95.5, np.nan, 88.0],
            "ACTIVE": [True, False, None],
        }
    )

    # All fields must be nullable (null) for this to work correctly in VFP
    specs = "NAME C(20) null; AGE N(10,0) null; SCORE N(10,2) null; ACTIVE L null"

    # 1. Test via append (row by row)
    with fastdbf.Table(dbf_path, specs, dbf_type="vfp") as table:
        for _, row in df.iterrows():
            table.append(row.to_dict())

    # Read back and verify
    with fastdbf.Table(dbf_path).open("r") as table:
        records = list(table)
        assert len(records) == 3

        # Row 1 (NaNs) - index 1
        assert records[1]["NAME"] is None
        assert records[1]["AGE"] is None
        assert records[1]["SCORE"] is None
        assert records[1]["ACTIVE"] is False  # Index 1 is False

        # Row 2 (Bob/None) - index 2
        assert records[2]["NAME"] == "Bob"
        assert records[2]["ACTIVE"] is None  # Index 2 is None


def test_pandas_string_dtype_na(tmp_path):
    # Tests the new pandas string dtype NA value
    dbf_path = str(tmp_path / "test_string_na.dbf")

    df = pd.DataFrame({"TEXT": pd.Series(["hello", None, "world"], dtype="string")})

    specs = "TEXT C(20) null"

    with fastdbf.Table(dbf_path, specs, dbf_type="vfp") as table:
        for _, row in df.iterrows():
            table.append(row.to_dict())

    with fastdbf.Table(dbf_path).open("r") as table:
        records = list(table)
        assert records[1]["TEXT"] is None


def test_float_to_int_conversion(tmp_path):
    dbf_path = str(tmp_path / "test_float_int.dbf")

    # DataFrame where an integer column becomes float due to a single NaN
    df = pd.DataFrame(
        {
            "ID": [1.0, 2.0, np.nan]  # These are floats!
        }
    )

    specs = "ID I null"

    with fastdbf.Table(dbf_path, specs, dbf_type="vfp") as table:
        for _, row in df.iterrows():
            table.append(row.to_dict())

    with fastdbf.Table(dbf_path).open("r") as table:
        records = list(table)
        assert records[0]["ID"] == 1
        assert records[1]["ID"] == 2
        assert records[2]["ID"] is None

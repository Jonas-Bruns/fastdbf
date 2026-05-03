import fastdbf
import pandas as pd
import pyarrow as pa
import os
from datetime import date, datetime

def test_write_from_df():
    dbf_path = "test_write.dbf"
    if os.path.exists(dbf_path): os.remove(dbf_path)
    
    # 1. Prepare a DataFrame with realistic types
    df = pd.DataFrame({
        "NAME": ["Alice", "Bob"],
        "AGE": pd.array([30, 25], dtype="Int64"),   # nullable Int64
        "SCORE": [95.5, 88.0],
        "BIRTH": pd.to_datetime(["1994-05-20", "1999-11-15"]).date,
        "CREATED": pd.to_datetime(["2024-01-01 12:00:00", "2024-02-01 08:30:00"]),
        "ACTIVE": [True, False],
    })
    
    print("DataFrame Dtypes:")
    print(df.dtypes)
    print()
    print("BIRTH[0] type:", type(df["BIRTH"][0]))
    print("CREATED[0] type:", type(df["CREATED"][0]))
    print("AGE[0] type:", type(df["AGE"][0]))
    print()
    
    # 2. Method A: extend_arrow (PyArrow RecordBatch)
    print("--- Method A: extend_arrow ---")
    try:
        batch = pa.RecordBatch.from_pandas(df)
        table_a = fastdbf.Table(
            "test_arrow.dbf",
            field_specs="NAME C(20); AGE N(10,0); SCORE N(10,2); BIRTH D; CREATED T; ACTIVE L",
            dbf_type="vfp"
        )
        table_a.open("rw")
        table_a.extend_arrow(batch)
        table_a.close()
        
        table_a = fastdbf.Table("test_arrow.dbf")
        table_a.open("r")
        print("Row 0:", table_a.row(0))
        print("AGE type:", type(table_a.row(0)["AGE"]))
        print("BIRTH type:", type(table_a.row(0)["BIRTH"]))
        print("CREATED type:", type(table_a.row(0)["CREATED"]))
        table_a.close()
        print("OK extend_arrow works!")
    except Exception as e:
        print(f"FAIL extend_arrow: {e}")
    
    print()
    
    # 3. Method B: append row-by-row with pandas types
    print("--- Method B: append from df.itertuples() ---")
    try:
        table_b = fastdbf.Table(
            "test_append.dbf",
            field_specs="NAME C(20); AGE N(10,0); SCORE N(10,2); BIRTH D; CREATED T; ACTIVE L",
            dbf_type="vfp"
        )
        table_b.open("rw")
        for row in df.itertuples(index=False):
            table_b.append({
                "NAME": row.NAME,
                "AGE": int(row.AGE),
                "SCORE": float(row.SCORE),
                "BIRTH": row.BIRTH,
                "CREATED": row.CREATED,  # pandas.Timestamp
                "ACTIVE": bool(row.ACTIVE),
            })
        table_b.close()
        
        table_b = fastdbf.Table("test_append.dbf")
        table_b.open("r")
        print("Row 0:", table_b.row(0))
        print("CREATED type:", type(table_b.row(0)["CREATED"]))
        table_b.close()
        print("OK append works!")
    except Exception as e:
        print(f"FAIL append: {e}")
    
    print()
    
    # 4. Method C: append with raw pandas types (no manual cast)
    print("--- Method C: append with raw pandas row dict ---")
    try:
        table_c = fastdbf.Table(
            "test_raw.dbf",
            field_specs="NAME C(20); AGE N(10,0); SCORE N(10,2); BIRTH D; CREATED T; ACTIVE L",
            dbf_type="vfp"
        )
        table_c.open("rw")
        for _, row in df.iterrows():
            table_c.append(row.to_dict())   # raw pandas types, no cast!
        table_c.close()
        
        table_c = fastdbf.Table("test_raw.dbf")
        table_c.open("r")
        print("Row 0:", table_c.row(0))
        table_c.close()
        print("OK raw pandas types work!")
    except Exception as e:
        print(f"FAIL raw pandas types: {e}")
    
    # Cleanup
    for f in ["test_write.dbf", "test_arrow.dbf", "test_append.dbf", "test_raw.dbf"]:
        if os.path.exists(f): os.remove(f)

if __name__ == "__main__":
    test_write_from_df()

import os

import dbf
import fastdbf


def analyze_header(path):
    with open(path, "rb") as f:
        raw = f.read(512)

    version = raw[0]
    header_len = int.from_bytes(raw[8:10], "little")
    rec_len = int.from_bytes(raw[10:12], "little")
    terminator_index = None

    # Search for 0x0D terminator
    for i in range(32, len(raw)):
        if raw[i] == 0x0D:
            terminator_index = i
            break

    return {
        "version": f"0x{version:02X}",
        "header_len": header_len,
        "rec_len": rec_len,
        "terminator_at": terminator_index,
        "total_file_size": os.path.getsize(path),
    }


# --- Test VFP ---
specs = "name C(20); score B BINARY; amount N(10,3) null; active L null"

# Python dbf
t_dbf_vfp = dbf.Table("vfp_dbf.dbf", specs, dbf_type="vfp")
t_dbf_vfp.open()
t_dbf_vfp.close()

# FastDBF
with fastdbf.Table("vfp_fast.dbf", specs, dbf_type="vfp") as t_fast_vfp:
    pass

print("=== VFP Header Comparison ===")
print("dbf package: ", analyze_header("vfp_dbf.dbf"))
print("fastdbf:     ", analyze_header("vfp_fast.dbf"))

# --- Test DB3 ---
specs_db3 = "name C(20); score N(10,2); active L"

# Python dbf
t_dbf_db3 = dbf.Table("db3_dbf.dbf", specs_db3, dbf_type="db3")
t_dbf_db3.open()
t_dbf_db3.close()

# FastDBF
with fastdbf.Table("db3_fast.dbf", specs_db3, dbf_type="db3") as t_fast_db3:
    pass

print("\n=== DB3 Header Comparison ===")
print("dbf package: ", analyze_header("db3_dbf.dbf"))
print("fastdbf:     ", analyze_header("db3_fast.dbf"))

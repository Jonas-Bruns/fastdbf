import dbf

import fastdbf

# 1. Original mit dbf-Paket erstellen (B BINARY mit decimals, nullable N mit decimals)
t = dbf.Table(
    "roundtrip_in.dbf",
    "name C(20); score B BINARY; amount N(10,3) null; active L null",
    dbf_type="vfp",
)
t.open()
t.close()

with open("roundtrip_in.dbf", "rb") as f:
    raw = f.read(32)
hl_orig = int.from_bytes(raw[8:10], "little")
print(f"Original header_length = {hl_orig}")

# 2. Mit fastdbf einlesen
with fastdbf.Table("roundtrip_in.dbf", dbf_type="vfp").open("r") as inp:
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
    print("Reconstructed specs:", field_specs)
    print("structure():", inp.structure())

# 3. Neue Datei mit fastdbf schreiben
with fastdbf.Table("roundtrip_out.dbf", "; ".join(field_specs), dbf_type="vfp") as out:
    pass

with open("roundtrip_out.dbf", "rb") as f:
    raw = f.read(32)
hl_out = int.from_bytes(raw[8:10], "little")
print(f"Output header_length   = {hl_out}")
print(f"Headers match: {hl_orig == hl_out}")

# 4. Kann das dbf-Paket die Output-Datei oeffnen?
t2 = dbf.Table("roundtrip_out.dbf")
t2.open()
print("dbf pkg opens output:", t2.structure())
t2.close()

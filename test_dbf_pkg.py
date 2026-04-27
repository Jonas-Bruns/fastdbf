import sys
import subprocess

try:
    import dbf
except ImportError:
    subprocess.check_call([sys.executable, "-m", "pip", "install", "dbf"])
    import dbf

t = dbf.Table('test_dbf2.dbf', 'name C(10); data B', dbf_type='vfp')
print("VFP B structure:", t.structure())

try:
    t2 = dbf.Table('test_dbf3.dbf', 'name C(10); data B', dbf_type='db3')
    print("DB3 B structure:", t2.structure())
except Exception as e:
    print("DB3 error:", e)

try:
    t3 = dbf.Table('test_dbf4.dbf', 'name C(10); data C(10) BINARY; memo M BINARY', dbf_type='vfp')
    print("VFP BINARY structure:", t3.structure())
except Exception as e:
    print("VFP BINARY error:", e)

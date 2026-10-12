"""Opens a DXF written by cave-dxf with ezdxf (an independent library) and audits it.
Usage:  python3 tools/dxf/validate_ezdxf.py file.dxf
Needs `pip install ezdxf`. Prints entity counts by type and any audit findings."""
import sys, collections
import ezdxf
from ezdxf import recover

path = sys.argv[1]
doc = ezdxf.readfile(path)
auditor = doc.audit()
counts = collections.Counter(e.dxftype() for e in doc.modelspace())
print(f"{path}: DXF {doc.dxfversion}, units={doc.units}")
print("entities:", dict(counts), "total", sum(counts.values()))
print("layers:", sorted(l.dxf.name for l in doc.layers))
print("audit errors:", len(auditor.errors), "fixes:", len(auditor.fixes))
for e in auditor.errors[:10]:
    print("  ERROR", e.code, e.message)
rec_doc, rec_aud = recover.readfile(path)
print("strict recover audit errors:", len(rec_aud.errors))
objs = doc.rootdict.get("CAVECAD")
print("CAVECAD dictionary present:", objs is not None)
sys.exit(1 if auditor.errors else 0)

import glob, pathlib, re

def write(id, body, **fields):
    p = pathlib.Path(glob.glob(f"cairn/items/{id:04d}-*.md")[0])
    s = p.read_text()
    end = s.index("---", 4)
    fm = s[:end]
    dep = fields.pop("depends_on", None)
    for k, v in fields.items():
        if re.search(rf"^{k}: ", fm, re.M):
            fm = re.sub(rf"^{k}: .*$", f"{k}: {v}", fm, flags=re.M)
        else:
            fm = fm.rstrip("\n") + f"\n{k}: {v}\n"
    if dep:
        fm = fm.rstrip("\n") + "\ndepends_on:\n" + "".join(f"- {x}\n" for x in dep)
    p.write_text(fm.rstrip("\n") + "\n---\n" + body)
    print("wrote", p.name)

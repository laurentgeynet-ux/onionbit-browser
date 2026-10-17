import re

src = open(r"D:\Projet\dillo\src\pixmaps.h", encoding="utf-8").read()
blocks = re.findall(r"static const char \*const (\w+)\[\] = \{(.*?)\};", src, re.S)

out = []
out.append("//! XPM icon data — verbatim `src/pixmaps.h` (upstream bitmaps).")
out.append("//! The strings are copied as-is; decoding happens in `xpm_decode`.\n")
for name, body in blocks:
    lines = re.findall(r'"((?:[^"\\]|\\.)*)"', body)
    out.append("/// `" + name + "` (verbatim `pixmaps.h`).")
    out.append("pub const " + name.upper() + ": &[&str] = &[")
    for ln in lines:
        esc = ln.replace("\\", "\\\\").replace('"', '\\"')
        out.append('    "' + esc + '",')
    out.append("];\n")

open(r"crates/dillo-browser/src/pixmaps.rs", "w", encoding="utf-8", newline="\n").write("\n".join(out))
print(len(blocks), "icons")

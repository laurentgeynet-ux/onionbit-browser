"""Genere crates/dillo-html/src/entities_table.rs depuis
D:/Projet/dillo/src/html_charrefs.h (verbatim — meme ordre bsearch)."""
import io, re

src = io.open('D:/Projet/dillo/src/html_charrefs.h', encoding='utf-8').read()
entries = re.findall(r'\{"((?:[^"\\]|\\.)*)",\s*"((?:[^"\\]|\\.)*)"|NULL,\s*("(?:[^"\\]|\\.)*"|NULL)\},?', src)
# le regex ci-dessus est fragile — parse ligne a ligne a la place
entries = []
for m in re.finditer(r'\{"((?:[^"\\]|\\.)*)",\s*("(?:[^"\\]|\\.)*"|NULL),\s*("(?:[^"\\]|\\.)*"|NULL)\}', src):
    name, h5, h4 = m.group(1), m.group(2), m.group(3)
    entries.append((name, h5, h4))

def cstr_to_bytes(tok):
    if tok == 'NULL':
        return 'None'
    inner = tok[1:-1]
    # decode C escapes simples
    out = inner.replace('\\"', '"').replace('\\\\', '\\')
    b = out.encode('utf-8')
    # emit as byte-string literal with escapes
    lit = ''.join(chr(x) if 32 <= x < 127 and chr(x) not in '"\\' else ('\\"' if chr(x) == '"' else '\\\\' if chr(x) == '\\' else '\\x%02x' % x) for x in b)
    return 'Some(b"%s")' % lit

def name_lit(tok):
    inner = tok.replace('\\"', '"').replace('\\\\', '\\')
    b = inner.encode('utf-8')
    lit = ''.join(chr(x) if 32 <= x < 127 and chr(x) not in '"\\' else '\\x%02x' % x for x in b)
    return 'b"%s"' % lit

lines = []
lines.append('//! `html_charrefs.h` — table `Charrefs` generee verbatim')
lines.append('//! (ordre identique → `binary_search` = `bsearch` amont).')
lines.append('//!')
lines.append('//! Regeneree par extraction de `D:/Projet/dillo/src/html_charrefs.h`.')
lines.append('//! Ne pas editer a la main.')
lines.append('')
lines.append('/// `Charref_t` — verbatim.')
lines.append('pub struct Charref {')
lines.append('    /// `ref` — nom de l\'entite (clef bsearch, `strcmp`).')
lines.append('    pub name: &\'static [u8],')
lines.append('    /// `html5_str` — valeur pour HTML5 (`None` ≙ NULL amont).')
lines.append('    pub html5: Option<&\'static [u8]>,')
lines.append('    /// `html4_str` — valeur pre-HTML5 (`None` ≙ NULL amont).')
lines.append('    pub html4: Option<&\'static [u8]>,')
lines.append('}')
lines.append('')
lines.append('/// `NumRef`.')
lines.append('pub const NUM_REF: usize = %d;' % len(entries))
lines.append('')
lines.append('/// `Charrefs[NumRef]` — verbatim (triee pour bsearch).')
lines.append('pub static CHARREFS: [Charref; NUM_REF] = [')
for name, h5, h4 in entries:
    lines.append('    Charref { name: %s, html5: %s, html4: %s },' % (name_lit(name), cstr_to_bytes(h5), cstr_to_bytes(h4)))
lines.append('];')
lines.append('')

io.open('crates/dillo-html/src/entities_table.rs', 'w', encoding='utf-8').write('\n'.join(lines))
print('%d entites ecrites' % len(entries))

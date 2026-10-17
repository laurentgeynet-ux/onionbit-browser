//! Port de `dpip/dpip.c` — **syntaxe des tags dpip** (sans le socket
//! `a_Dpi_send_blocking_cmd`, phase DPI) :
//!
//! ```text
//! <[*alpha] *(<name>"="Quote<escaped_value>Quote) " "Quote">"
//! ```
//!
//! Quote = `'` ; les valeurs stuffent `'` en `''` ; EOT = ` '>' `.

/// `Quote` amont.
pub const QUOTE: u8 = b'\'';

/// `a_Dpip_build_cmd` — `format` contient des `%s` (un par argument,
/// strings only — verbatim) ; chaque arg est stuffé `'`→`''` et mis
/// entre quotes. `None` si `format` contient `'` (≙ amont retourne
/// NULL — nom d'attribut ne peut contenir Quote).
#[must_use]
pub fn build_cmd(format: &str, args: &[&str]) -> Option<String> {
    if format.contains('\'') {
        return None;
    }
    let mut cmd = String::with_capacity(64 + args.iter().map(|a| a.len() * 2).sum::<usize>());
    cmd.push('<');
    let mut it = args.iter();
    // boucle `strstr(q, "%s")` amont
    let mut q = format;
    loop {
        match q.find("%s") {
            None => {
                cmd.push_str(q);
                break;
            }
            Some(p) => {
                cmd.push_str(&q[..p]);
                q = &q[p + 2..];
                cmd.push('\'');
                for ch in it.next().map_or("", |s| *s).chars() {
                    cmd.push(ch);
                    if ch == '\'' {
                        cmd.push('\'');
                    }
                }
                cmd.push('\'');
            }
        }
    }
    cmd.push(' ');
    cmd.push('\'');
    cmd.push('>');
    Some(cmd)
}

/// `a_Dpip_get_attr_l` — machine à états verbatim
/// (`SEEK_NAME/MATCH_NAME/SKIP_VALUE/SKIP_QUOTE/FOUND`) ;
/// le `'` stuffé est déstuffé à la sortie. `None` = absent/malformé.
#[must_use]
pub fn get_attr_l(tag: &[u8], attrname: &[u8]) -> Option<Vec<u8>> {
    #[derive(PartialEq)]
    enum St {
        SeekName,
        MatchName,
        SkipValue,
        SkipQuote,
        Found,
    }
    if tag.is_empty() || attrname.is_empty() {
        return None;
    }
    let mut state = St::SeekName;
    let mut matched = 0usize;
    let mut idx = 1usize;
    while idx < tag.len() && state != St::Found {
        match state {
            St::SeekName => {
                if tag[idx] == attrname[0] && (tag[idx - 1] == b' ' || tag[idx - 1] == b'<') {
                    matched = 1;
                    state = St::MatchName;
                } else if tag[idx] == QUOTE && tag[idx - 1] == b'=' {
                    state = St::SkipValue;
                }
            }
            St::MatchName => {
                if matched < attrname.len() && tag[idx] == attrname[matched] {
                    matched += 1;
                } else if tag[idx] == b'=' && matched == attrname.len() {
                    state = St::Found;
                } else {
                    state = St::SeekName;
                }
            }
            St::SkipValue => {
                if tag[idx] == QUOTE {
                    state = if tag.get(idx + 1) == Some(&QUOTE) {
                        St::SkipQuote
                    } else {
                        St::SeekName
                    };
                }
            }
            St::SkipQuote => state = St::SkipValue,
            St::Found => {}
        }
        idx += 1;
    }
    if state != St::Found {
        return None;
    }
    // Amont : `FOUND` consomme une itération → `i` finit à `'='+2`
    // (après la quote ouvrante) ; ici la boucle sort à `'='+1 → `+1`.
    let start = idx + 1;
    // `p` cherche la quote de fin non-stuffée (`'` non suivie de `'`),
    // qui doit être suivie d'un espace (`' '` = EOT) — verbatim amont.
    let mut scan = start;
    let mut end_q = None;
    while scan < tag.len() {
        if tag[scan] == QUOTE {
            if tag.get(scan + 1) == Some(&QUOTE) {
                scan += 2;
                continue;
            }
            end_q = Some(scan);
            break;
        }
        scan += 1;
    }
    let q = end_q.filter(|&q| tag.get(q + 1) == Some(&b' '))?;
    let mut val = Vec::with_capacity(q - start);
    let mut src = start;
    while src < q {
        val.push(tag[src]);
        if tag[src] == QUOTE && tag.get(src + 1) == Some(&QUOTE) {
            src += 1;
        }
        src += 1;
    }
    Some(val)
}

/// `a_Dpip_get_attr`.
#[must_use]
pub fn get_attr(tag: &str, attrname: &str) -> Option<String> {
    get_attr_l(tag.as_bytes(), attrname.as_bytes()).map(|v| {
        // les valeurs dpi sont du texte — perte de non-UTF8 tolérée
        String::from_utf8_lossy(&v).into_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_cmd_stuffs_quotes() {
        assert_eq!(
            build_cmd("cmd=%s cookie=%s host=%s", &["set_cookie", "a='b", "x.y"]).as_deref(),
            Some("<cmd='set_cookie' cookie='a=''b' host='x.y' '>")
        );
        // Quote dans le format → NULL amont
        assert_eq!(build_cmd("cmd='%s'", &["x"]), None);
        // sans %s : corps verbatim
        assert_eq!(build_cmd("cmd=x", &[]).as_deref(), Some("<cmd=x '>"));
    }

    #[test]
    fn get_attr_verbatim() {
        assert_eq!(
            get_attr("<dpi a='b i' b='12' '>", "b"),
            Some("12".to_string())
        );
        assert_eq!(
            get_attr("<a='ain''t no doubt' '>", "a"),
            Some("ain't no doubt".to_string())
        );
        // absent / malformé
        assert_eq!(get_attr("<a='b' '>", "z"), None);
        assert_eq!(get_attr("", "a"), None);
        // skip d'une valeur quotée contenant le nom cherché
        assert_eq!(
            get_attr("<x='cookie' cookie='v' '>", "cookie"),
            Some("v".to_string())
        );
    }
}

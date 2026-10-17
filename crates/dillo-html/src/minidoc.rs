//! `minidoc` — parseur minimaliste de la tranche verticale (phase 0.5).
//!
//! Précurseur de `src/html.cc` : `<h1>`..`<h6>`, `<p>`, `<a href>`
//! uniquement. Tolérant à la Dillo : balise inconnue ignorée,
//! texte hors bloc groupé en paragraphe, `<a>` non fermé absorbé
//! jusqu'à la fin du bloc. Sera remplacé par le vrai parseur en
//! phase 2 — ce module est le pilote, pas la spec.
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)] // `int`/`float` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

/// Run inline d'un bloc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    /// Texte brut (entités de base décodées).
    Text(String),
    /// `<a href>` — texte + cible.
    Link {
        /// Texte affiché.
        text: String,
        /// Cible `href` brute (résolue au clic par `dillo-net`).
        href: String,
    },
}

/// Bloc de document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// `<h1>`..`<h6>`.
    Heading {
        /// Niveau 1..=6.
        level: u8,
        /// Contenu inline.
        runs: Vec<Inline>,
    },
    /// `<p>` ou texte hors bloc.
    Paragraph {
        /// Contenu inline.
        runs: Vec<Inline>,
    },
}

/// Entités nommées minimales (`html_charrefs` complet = phase 2).
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&nbsp;", "\u{a0}")
}

/// Extrait `name="value"` (ou `'value'`, ou non-quotée) d'une
/// chaîne de tag — tolérant comme le scanner amont.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(pos) = rest.find(name) {
        let after = &rest[pos + name.len()..];
        let after = after.trim_start();
        if let Some(after) = after.strip_prefix('=') {
            let after = after.trim_start();
            let (delim, after) = match after.as_bytes().first() {
                Some(b'"') => ('"', &after[1..]),
                Some(b'\'') => ('\'', &after[1..]),
                _ => {
                    let end = after
                        .find(|c: char| c.is_whitespace() || c == '>')
                        .unwrap_or(after.len());
                    return Some(&after[..end]);
                }
            };
            let end = after.find(delim).unwrap_or(after.len());
            return Some(&after[..end]);
        }
        rest = after;
    }
    None
}

/// Parse les runs inline d'un fragment : texte + `<a href>`.
fn parse_inlines(frag: &str) -> Vec<Inline> {
    let mut runs = Vec::new();
    let mut rest = frag;
    while let Some(pos) = rest.find('<') {
        let (text, tag_rest) = rest.split_at(pos);
        push_text(&mut runs, text);
        let Some(gt) = tag_rest.find('>') else {
            break;
        };
        let tag = &tag_rest[1..gt];
        let tag_lower = tag.to_ascii_lowercase();
        if tag_lower.starts_with("a ") || tag_lower.starts_with("a\t") {
            let href = attr(tag, "href").unwrap_or("").to_owned();
            let after_tag = &tag_rest[gt + 1..];
            let close = after_tag.to_ascii_lowercase().find("</a>");
            let (text, tail) = match close {
                Some(c) => (&after_tag[..c], &after_tag[c + 4..]),
                None => (after_tag, ""), // <a> non fermé → fin de bloc
            };
            runs.push(Inline::Link {
                text: decode_entities(text),
                href,
            });
            rest = tail;
        } else {
            // toute autre balise inline est ignorée, son contenu reste
            rest = &tag_rest[gt + 1..];
        }
    }
    push_text(&mut runs, rest);
    runs
}

fn push_text(runs: &mut Vec<Inline>, s: &str) {
    let decoded = decode_entities(s);
    if !decoded.trim().is_empty() {
        runs.push(Inline::Text(decoded));
    }
}

/// `parse_minidoc` — fragment HTML → blocs.
#[must_use]
pub fn parse_minidoc(input: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut rest = input;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            // texte libre → paragraphe (tolérance amont)
            let runs = parse_inlines(rest);
            if !runs.is_empty() {
                blocks.push(Block::Paragraph { runs });
            }
            break;
        };
        if lt > 0 {
            let runs = parse_inlines(&rest[..lt]);
            if !runs.is_empty() {
                blocks.push(Block::Paragraph { runs });
            }
        }
        let tag_rest = &rest[lt + 1..];
        let Some(gt) = tag_rest.find('>') else { break };
        let tag = tag_rest[..gt].to_ascii_lowercase();
        let body = &tag_rest[gt + 1..];
        let close_name = tag.split_whitespace().next().unwrap_or("");
        match close_name
            .strip_prefix('h')
            .and_then(|n| n.parse::<u8>().ok())
        {
            Some(level @ 1..=6) => {
                let close_tag = format!("</h{level}>");
                let (frag, tail) = split_on_close(body, &close_tag);
                blocks.push(Block::Heading {
                    level,
                    runs: parse_inlines(frag),
                });
                rest = tail;
            }
            _ if close_name == "p" => {
                let (frag, tail) = split_on_close(body, "</p>");
                blocks.push(Block::Paragraph {
                    runs: parse_inlines(frag),
                });
                rest = tail;
            }
            _ => {
                // balise de bloc inconnue ou </fermeture> → ignorée
                rest = body;
            }
        }
    }
    blocks
}

fn split_on_close<'a>(body: &'a str, close: &str) -> (&'a str, &'a str) {
    match body.to_ascii_lowercase().find(close) {
        Some(c) => (&body[..c], &body[c + close.len()..]),
        None => (body, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_paragraph_link() {
        let doc = parse_minidoc(
            "<h1>Titre</h1><p>Un <a href=\"p2.html\">lien</a> et du <b>gras</b>.</p>",
        );
        assert_eq!(doc.len(), 2);
        let Block::Heading { level, runs } = &doc[0] else {
            unreachable!()
        };
        assert_eq!(*level, 1);
        assert_eq!(runs, &[Inline::Text("Titre".into())]);
        let Block::Paragraph { runs } = &doc[1] else {
            unreachable!()
        };
        assert!(runs.contains(&Inline::Link {
            text: "lien".into(),
            href: "p2.html".into()
        }));
        assert!(runs.contains(&Inline::Text("gras".into())));
    }

    #[test]
    fn tolerant_unclosed_and_loose_text() {
        let doc = parse_minidoc("texte libre<p>para non fermé<a href='x'>li");
        assert_eq!(doc.len(), 2);
        assert!(matches!(doc[0], Block::Paragraph { .. }));
        let Block::Paragraph { runs } = &doc[1] else {
            unreachable!()
        };
        assert!(runs.contains(&Inline::Link {
            text: "li".into(),
            href: "x".into()
        }));
    }

    #[test]
    fn entities() {
        let doc = parse_minidoc("<p>a &amp; b &lt;ok&gt;</p>");
        let Block::Paragraph { runs } = &doc[0] else {
            unreachable!()
        };
        // les entités fusionnent dans un seul run texte
        assert_eq!(runs, &[Inline::Text("a & b <ok>".into())]);
    }
}

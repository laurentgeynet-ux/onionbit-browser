//! Port of `dpi/bookmarks.c` — modèle `bm.txt` + génération des
//! pages HTML du serveur `bookmarks.dpi`.
//!
//! Verbatim amont : sections numérotées (`:sN:`) + bookmarks
//! (`sN url title`), normalisation des numéros à la sauvegarde,
//! format HTML/CSS byte-exact (DOCTYPES/styles/textes repris à
//! l'identique pour l'interopérance visuelle).

// Lint verbatim : fidélité aux casts/tests C amont (voir AGENTS.md §5).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::similar_names)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::result_unit_err)]
#![allow(clippy::format_push_string)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::needless_continue)]
#![allow(clippy::used_underscore_binding)]
#![allow(clippy::manual_strip)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::overly_complex_bool_expr)]
use crate::dpiutil;

/// `DOCTYPE` verbatim.
const DOCTYPE: &str = "<!DOCTYPE html>\n";
/// `CSS_STYLE` verbatim.
const CSS_STYLE: &str = "<style type='text/css'>\n\
    body { margin: 2em; background: white; font-size: 14px }\n\
    ul { font-size: inherit; margin-left: 1em; margin-top: 0.5em; list-style-type: none; }\n\
    ul.main { list-style-type: none; }\n\
    li { min-height: 1.6em; }\n\
    div.modify-menu { background-color: #f0f0f0; border: solid 1px #ccc; padding: 1em }\n\
    nav ul { margin-top: 1em }\n\
    nav li { display: inline; }\n\
    a { color: black; }\n\
    </style>\n";

/// `BmRec`.
#[derive(Debug)]
pub struct BmRec {
    pub key: i32,
    pub section: i32,
    /// déjà `Escape_uri_str(url,"'")` (verbatim).
    pub url: String,
    /// déjà `Escape_html_str` (verbatim).
    pub title: String,
}

/// `BmSec`.
#[derive(Debug)]
pub struct BmSec {
    pub section: i32,
    pub title: String,
    /// `o_sec` — ancien numéro pour la normalisation.
    o_sec: i32,
}

/// État du serveur ≙ globales `B_bms`/`B_secs`/`bm_key`/`sec_key`/
/// `BmFile`/`BmFileTimeStamp`/`MODIFY_PAGE_NUM`.
pub struct BmStore {
    bms: Vec<BmRec>,
    secs: Vec<BmSec>,
    bm_key: i32,
    sec_key: i32,
    /// `BmFile` — chemin de `~/.dillo/bm.txt`.
    pub bm_file: std::path::PathBuf,
    bm_mtime: Option<std::time::SystemTime>,
    /// `MODIFY_PAGE_NUM` verbatim.
    pub modify_page_num: i32,
    /// url mémorisée par `send_modify_update(NULL,url)` amont.
    saved_url: String,
    /// section mémorisée par `modify_add_url(NULL,url)` amont.
    saved_section: i32,
}

/// `make_one_line_str` verbatim : espaces → `&nbsp;`.
fn one_line(s: &str) -> String {
    s.replace(' ', "&nbsp;")
}

/// `Unencode_str` verbatim : `+`→espace, `%0D%0A`→`\n`, `%XX`→byte.
#[must_use]
pub fn unencode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if b[i] == b'%' && i + 2 < b.len() {
            if s.len() >= i + 6 && s[i..i + 6].eq_ignore_ascii_case("%0D%0A") {
                out.push(b'\n');
                i += 6;
            } else {
                let h = |c: u8| -> u8 {
                    if c.is_ascii_digit() {
                        c - b'0'
                    } else {
                        c.to_ascii_uppercase() - b'A' + 10
                    }
                };
                out.push(h(b[i + 1]) * 16 + h(b[i + 2]));
                i += 3;
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl BmStore {
    /// `B_bms`/`B_secs`/`BmFile` init (verbatim main).
    #[must_use]
    pub fn new(bm_file: std::path::PathBuf) -> Self {
        Self {
            bms: Vec::new(),
            secs: Vec::new(),
            bm_key: 0,
            sec_key: 0,
            bm_file,
            bm_mtime: None,
            modify_page_num: 1,
            saved_url: String::new(),
            saved_section: 0,
        }
    }

    /// Accès lecture pour les pages.
    #[must_use]
    pub fn sections(&self) -> &[BmSec] {
        &self.secs
    }

    /// Accès lecture pour les pages.
    #[must_use]
    pub fn bookmarks(&self) -> &[BmRec] {
        &self.bms
    }

    /// `Bms_get`.
    fn get(&mut self, key: i32) -> Option<&mut BmRec> {
        self.bms.iter_mut().find(|b| b.key == key)
    }

    /// `Bms_get_sec`.
    fn get_sec(&mut self, key: i32) -> Option<&mut BmSec> {
        self.secs.iter_mut().find(|s| s.section == key)
    }

    /// `Bms_add` verbatim : `key=++bm_key`, url escapée, title escapé.
    fn add(&mut self, section: i32, url: &str, title: &str) {
        self.bm_key += 1;
        self.bms.push(BmRec {
            key: self.bm_key,
            section,
            url: String::from_utf8_lossy(&dpiutil::escape_uri_str(url.as_bytes(), Some(b"'")))
                .into_owned(),
            title: String::from_utf8_lossy(&dpiutil::escape_html_str(title.as_bytes()))
                .into_owned(),
        });
    }

    /// `Bms_sec_add` verbatim : `section=sec_key`++.
    fn sec_add(&mut self, title: &str) {
        self.secs.push(BmSec {
            section: self.sec_key,
            title: String::from_utf8_lossy(&dpiutil::escape_html_str(title.as_bytes()))
                .into_owned(),
            o_sec: self.sec_key,
        });
        self.sec_key += 1;
    }

    /// `Bms_del` verbatim : retire + reset `bm_key` si vide.
    fn del(&mut self, key: i32) {
        if let Some(pos) = self.bms.iter().position(|b| b.key == key) {
            self.bms.remove(pos);
        }
        if self.bms.is_empty() {
            self.bm_key = 0;
        }
    }

    /// `Bms_sec_del` verbatim : section + tous ses bookmarks.
    fn sec_del(&mut self, section: i32) {
        if self.secs.iter().any(|s| s.section == section) {
            self.secs.retain(|s| s.section != section);
            let keys: Vec<i32> = self
                .bms
                .iter()
                .filter(|b| b.section == section)
                .map(|b| b.key)
                .collect();
            for k in keys {
                self.del(k);
            }
        }
        if self.secs.is_empty() {
            self.sec_key = 0;
        }
    }

    /// `Bms_move`.
    fn mv(&mut self, key: i32, target: i32) {
        if let Some(b) = self.get(key) {
            b.section = target;
        }
    }

    /// `Bms_update_title`.
    fn update_title(&mut self, key: i32, title: &str) {
        if let Some(b) = self.get(key) {
            b.title =
                String::from_utf8_lossy(&dpiutil::escape_html_str(title.as_bytes())).into_owned();
        }
    }

    /// `Bms_update_sec_title`.
    fn update_sec_title(&mut self, key: i32, title: &str) {
        if let Some(s) = self.get_sec(key) {
            s.title =
                String::from_utf8_lossy(&dpiutil::escape_html_str(title.as_bytes())).into_owned();
        }
    }

    /// `Bms_normalize` verbatim : ≥1 section (« Unclassified »),
    /// renumérotation corrélative + remap des bookmarks.
    fn normalize(&mut self) {
        if self.secs.is_empty() {
            self.sec_add("Unclassified");
        }
        for (i, s) in self.secs.iter_mut().enumerate() {
            s.o_sec = s.section;
            s.section = i as i32;
        }
        for i in 0..self.secs.len() {
            let (new, old) = (self.secs[i].section, self.secs[i].o_sec);
            if new != old {
                for b in &mut self.bms {
                    if b.section == old {
                        b.section = new;
                    }
                }
            }
        }
    }

    /// `Bms_load` verbatim : `sN url title` et `:sN: title`.
    /// Retourne 0 ok / 1 échec (verbatim).
    pub fn load(&mut self, contents: &str) -> i32 {
        self.bms.clear();
        self.secs.clear();
        self.bm_key = 0;
        self.sec_key = 0;
        for line in contents.lines() {
            if line.starts_with('s') {
                // « sN url title » — `strtol(buf+1)` verbatim.
                let section: i32 = line[1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '-')
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0);
                let Some(p1) = line.find(' ') else {
                    eprintln!("[bookmarks dpi]: syntax error: {line}");
                    continue;
                };
                let rest2 = &line[p1 + 1..];
                let Some(p2) = rest2.find(' ') else {
                    eprintln!("[bookmarks dpi]: syntax error: {line}");
                    continue;
                };
                let (url, title) = (&rest2[..p2], &rest2[p2 + 1..]);
                let title = String::from_utf8_lossy(&dpiutil::unescape_html_str(title.as_bytes()))
                    .into_owned();
                self.add(section, url, &title);
            } else if line.starts_with(":s") {
                // « :sN: title »
                let Some(p) = line[2..].find(' ') else {
                    eprintln!("[bookmarks dpi]: syntax error: {line}");
                    continue;
                };
                let title = &line[2 + p + 1..];
                self.sec_add(title);
            } else if !line.is_empty() {
                eprintln!("[bookmarks dpi]: syntax error: {line}");
            }
        }
        self.bm_mtime = std::fs::metadata(&self.bm_file)
            .and_then(|m| m.modified())
            .ok();
        0
    }

    /// `Bms_cond_load` verbatim : recharge si timestamp changé ou
    /// listes vides. `Bms_check_import` (bookmarks.html → bm.txt via
    /// grep/sed amont) est reporté côté bin (spawn shell POSIX).
    pub fn cond_load(&mut self) -> bool {
        let mtime = std::fs::metadata(&self.bm_file)
            .and_then(|m| m.modified())
            .ok();
        if self.bm_mtime.is_none()
            || self.bms.is_empty()
            || self.secs.is_empty()
            || self.bm_mtime < mtime
        {
            let _ = self.load_file();
            return true;
        }
        false
    }

    /// Lit le fichier et charge. ≙ `Bms_load` + fopen.
    pub fn load_file(&mut self) -> i32 {
        match std::fs::read(&self.bm_file) {
            Ok(bytes) => self.load(&String::from_utf8_lossy(&bytes)),
            Err(_) => 1,
        }
    }

    /// `Bms_save` verbatim : backup `.bak` si >256 o, normalisation,
    /// `:sN: title` puis `sN url title` groupés par section.
    pub fn save(&mut self) -> i32 {
        if let Ok(md) = std::fs::metadata(&self.bm_file) {
            if md.len() > 256 {
                let bak = self.bm_file.with_extension("txt.bak");
                let _ = std::fs::rename(&self.bm_file, &bak);
            }
        }
        self.normalize();
        let mut out = String::new();
        for s in &self.secs {
            let t = dpiutil::unescape_html_str(s.title.as_bytes());
            out.push_str(&format!(
                ":s{}: {}\n",
                s.section,
                String::from_utf8_lossy(&t)
            ));
        }
        for s in &self.secs {
            for b in &self.bms {
                if b.section == s.section {
                    let t = dpiutil::unescape_html_str(b.title.as_bytes());
                    out.push_str(&format!(
                        "s{} {} {}\n",
                        b.section,
                        b.url,
                        String::from_utf8_lossy(&t)
                    ));
                }
            }
        }
        if std::fs::write(&self.bm_file, out.as_bytes()).is_err() {
            return 1;
        }
        self.bm_mtime = std::fs::metadata(&self.bm_file)
            .and_then(|m| m.modified())
            .ok();
        0
    }

    /// `Bmsrv_add_bm` verbatim : section 0 + save.
    pub fn add_bm(&mut self, url: &str, title: &str) {
        let t = String::from_utf8_lossy(&dpiutil::unescape_html_str(title.as_bytes())).into_owned();
        self.add(0, url, &t);
        let _ = self.save();
    }

    /// `Bmsrv_count_urls_and_sections` verbatim : compte `&urlN=` et
    /// `&sN=` dans la query.
    fn count_marked(url: &str) -> (usize, usize) {
        let Some(p) = url.find('?') else {
            return (0, 0);
        };
        let q = &url[p..];
        let b = q.as_bytes();
        let mut n_url = 0;
        let mut n_sec = 0;
        // `strstr(q,"&url")` puis digits puis '=' — verbatim.
        let mut i = 0;
        while let Some(rel) = q[i..].find("&url") {
            let st = i + rel + 4;
            let mut j = st;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j < b.len() && b[j] == b'=' {
                n_url += 1;
            }
            i = i + rel + 1;
        }
        i = 0;
        while let Some(rel) = q[i..].find("&s") {
            let st = i + rel + 2;
            let mut j = st;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j < b.len() && b[j] == b'=' {
                n_sec += 1;
            }
            i = i + rel + 1;
        }
        (n_sec, n_url)
    }

    /// Itère les `&sN=` marqués (verbatim strstr loop).
    fn marked_secs(url: &str) -> Vec<i32> {
        let Some(p) = url.find('?') else {
            return Vec::new();
        };
        let q = &url[p..];
        let b = q.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(rel) = q[i..].find("&s") {
            let st = i + rel + 2;
            let mut j = st;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j < b.len() && b[j] == b'=' && j > st {
                out.push(q[st..j].parse().unwrap_or(0));
            }
            i = i + rel + 1;
        }
        out
    }

    /// Itère les `&urlN=` marqués (verbatim strstr loop).
    fn marked_urls(url: &str) -> Vec<i32> {
        let Some(p) = url.find('?') else {
            return Vec::new();
        };
        let q = &url[p..];
        let b = q.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(rel) = q[i..].find("&url") {
            let st = i + rel + 4;
            let mut j = st;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j < b.len() && b[j] == b'=' && j > st {
                out.push(q[st..j].parse().unwrap_or(0));
            }
            i = i + rel + 1;
        }
        out
    }

    /// Itère `&sN…` **sans** exiger `=` (verbatim `dIsdigit(p[2])`
    /// dans `modify_delete`/`modify_move`).
    fn loose_secs(url: &str) -> Vec<i32> {
        let Some(p) = url.find('?') else {
            return Vec::new();
        };
        let q = &url[p..];
        let b = q.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(rel) = q[i..].find("&s") {
            let st = i + rel + 2;
            if st < b.len() && b[st].is_ascii_digit() {
                let mut j = st;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                out.push(q[st..j].parse().unwrap_or(0));
            }
            i = i + rel + 1;
        }
        out
    }

    /// Itère `&urlN…` sans exiger `=` (verbatim delete/move).
    fn loose_urls(url: &str) -> Vec<i32> {
        let Some(p) = url.find('?') else {
            return Vec::new();
        };
        let q = &url[p..];
        let b = q.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(rel) = q[i..].find("&url") {
            let st = i + rel + 4;
            if st < b.len() && b[st].is_ascii_digit() {
                let mut j = st;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                out.push(q[st..j].parse().unwrap_or(0));
            }
            i = i + rel + 1;
        }
        out
    }

    /// `Bmsrv_modify_delete` verbatim.
    pub fn modify_delete(&mut self, url: &str) {
        let mut any = false;
        for k in Self::loose_secs(url) {
            self.sec_del(k);
            any = true;
        }
        for k in Self::loose_urls(url) {
            self.del(k);
            any = true;
        }
        if any {
            let _ = self.save();
        }
    }

    /// `Bmsrv_modify_move` verbatim : 1re section marquée = cible,
    /// urls déplacées.
    pub fn modify_move(&mut self, url: &str) -> i32 {
        let Some(section) = Self::loose_secs(url).into_iter().next() else {
            return 1;
        };
        let mut n = 0;
        for k in Self::loose_urls(url) {
            self.mv(k, section);
            n += 1;
        }
        if n > 0 {
            let _ = self.save();
        }
        0
    }

    /// `Bmsrv_modify_update` verbatim : `&sK=title` et `&titleK=val`.
    pub fn modify_update(&mut self, url: &str) {
        // verbatim : cherche « s » précédé de & ou ?, digits, '='.
        let b = url.as_bytes();
        let mut i = 0;
        while let Some(rel) = url[i..].find('s') {
            let p = i + rel;
            let prev_ok = p > 0 && (b[p - 1] == b'&' || b[p - 1] == b'?');
            if prev_ok {
                let mut j = p + 1;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                if j > p + 1 && j < b.len() && b[j] == b'=' {
                    let key: i32 = url[p + 1..j].parse().unwrap_or(0);
                    let vstart = j + 1;
                    let vend = url[vstart..].find('&').map_or(url.len(), |e| vstart + e);
                    let title = unencode(&url[vstart..vend]);
                    self.update_sec_title(key, &title);
                }
            }
            i = p + 1;
        }
        let mut i = 0;
        while let Some(rel) = url[i..].find("title") {
            let p = i + rel;
            let prev_ok = p > 0 && (b[p - 1] == b'&' || b[p - 1] == b'?');
            if prev_ok {
                let mut j = p + 5;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                if j > p + 5 && j < b.len() && b[j] == b'=' {
                    let key: i32 = url[p + 5..j].parse().unwrap_or(0);
                    let vstart = j + 1;
                    let vend = url[vstart..].find('&').map_or(url.len(), |e| vstart + e);
                    let title = unencode(&url[vstart..vend]);
                    self.update_title(key, &title);
                }
            }
            i = p + 1;
        }
        let _ = self.save();
    }

    /// `Bmsrv_modify_add_section` verbatim : `&title=…`.
    pub fn modify_add_section(&mut self, url: &str) -> i32 {
        let Some(p) = url.find("&title=") else {
            return 1;
        };
        let tstart = p + 7;
        let tend = url[tstart..].find('&').map_or(url.len(), |e| tstart + e);
        let title = unencode(&url[tstart..tend]);
        self.sec_add(&title);
        let _ = self.save();
        0
    }

    /// `Bmsrv_modify_add_url(NULL, url)` verbatim — mémorise la
    /// section marquée (phase 1 de `operation=add_url`).
    pub fn save_target_section(&mut self, url: &str) {
        for k in Self::marked_secs(url) {
            self.saved_section = k;
        }
    }

    /// `Bmsrv_modify_add_url(sh, url)` verbatim — `&title=`+`&url=`
    /// → ajout dans la section mémorisée.
    pub fn modify_add_url(&mut self, url: &str) -> i32 {
        let u = url;
        let Some(tp) = u.find("&title=") else {
            return 1;
        };
        let Some(up) = u.find("&url=") else { return 1 };
        let tstart = tp + 7;
        let tend = u[tstart..].find('&').map_or(u.len(), |e| tstart + e);
        let ustart = up + 5;
        let uend = u[ustart..].find('&').map_or(u.len(), |e| ustart + e);
        let title = unencode(&u[tstart..tend]);
        let surl = unencode(&u[ustart..uend]);
        if !title.is_empty() && !surl.is_empty() {
            let t =
                String::from_utf8_lossy(&dpiutil::unescape_html_str(title.as_bytes())).into_owned();
            self.add(self.saved_section, &surl, &t);
        }
        self.saved_section = 0;
        let _ = self.save();
        0
    }

    /// `Bmsrv_check_modify_request` verbatim — retourne
    /// `Ok(())` ≙ 0 (proceed), `Err(msg)` ≙ envoie status + retourne 2.
    pub fn check_modify_request(&self, url: &str) -> Result<(), &'static str> {
        let (n_sec, n_url) = Self::count_marked(url);
        let has = |op: &str| url.contains(op);
        if has("operation=delete&") {
            if n_url > 0 || n_sec > 0 {
                return Ok(());
            }
            Err("Delete: you must mark what to delete!")
        } else if has("operation=move&") {
            if n_url > 0 && n_sec > 0 {
                Ok(())
            } else if n_url > 0 {
                Err("Move: you must mark a target section!")
            } else if n_sec > 0 {
                Err("Move: can not move a section (yet).")
            } else {
                Err("Move: you must mark some urls, and a target section!")
            }
        } else if has("operation=modify&") {
            if n_url > 0 || n_sec > 0 {
                return Ok(());
            }
            Err("Modify: you must mark what to update!")
        } else if has("operation=modify2&")
            || has("operation=add_sec&")
            || has("operation=add_section&")
        {
            Ok(())
        } else if has("operation=add_url&") {
            if n_sec <= 1 {
                return Ok(());
            }
            Err("Add url: only one target section is allowed!")
        } else if has("operation=add_url2&") {
            Ok(())
        } else if has("operation=none&") {
            Err("No operation, just do nothing!")
        } else {
            Err("Sorry, not implemented yet.")
        }
    }

    /// `Bmsrv_send_modify_update` verbatim — `phase1` ≙ `sh==NULL`
    /// (mémorise l'URL pour la page suivante).
    pub fn send_modify_update(&mut self, phase1_url: Option<&str>) -> Option<String> {
        if let Some(u) = phase1_url {
            self.saved_url = u.to_string();
            return None;
        }
        let url1 = self.saved_url.clone();
        let mut full = String::from(DOCTYPE);
        full.push_str("<html>\n<head>\n<title>Bookmarks</title>\n");
        full.push_str(CSS_STYLE);
        full.push_str("</head>\n<body id='dillo_bm'>\n");
        full.push_str(
            "<h1>Bookmarks: Update</h1>\n<form action='modify'>\n\
             <input type='hidden' name='operation' value='modify2'>\n",
        );
        let (n_sec, n_url) = Self::count_marked(&url1);
        if n_sec > 0 {
            full.push_str("<h2>Update sections</h2>\n<ul>\n");
            for key in Self::marked_secs(&url1) {
                if let Some(s) = self.secs.iter().find(|s| s.section == key) {
                    full.push_str(&format!(
                        "<li style='padding: 0.5em'>\n  \
                         <input type='text' name='s{}' size='64' value='{}'>\n</li>\n",
                        s.section, s.title
                    ));
                }
            }
            full.push_str("</ul>\n");
        }
        if n_url > 0 {
            full.push_str("<h2>Update titles</h2>\n<ul>\n");
            for key in Self::marked_urls(&url1) {
                if let Some(b) = self.bms.iter().find(|b| b.key == key) {
                    full.push_str(&format!(
                        "<li style='padding: 0.5em'>\n  \
                         <input type='text' name='title{}' size='64'\n value='{}'>\n  \
                         <br>URL: <code>{}</code>\n</li>\n",
                        b.key, b.title, b.url
                    ));
                }
            }
            full.push_str("</ul>\n");
        }
        full.push_str(
            "<input type='submit' name='submit' value='Submit'>\n</form>\n</body>\n</html>\n",
        );
        Some(full)
    }
}

// ---------------------------------------------------------------
// pages — verbatim `send_bm_page`/`Bmsrv_send_modify_*` (le corps
// est produit en String ; le bin écrit header HTTP + corps).
// ---------------------------------------------------------------

/// `send_bm_page` verbatim.
#[must_use]
pub fn bm_page(st: &BmStore) -> String {
    let mut out = String::from(DOCTYPE);
    out.push_str("<html>\n<head>\n<title>Bookmarks</title>\n");
    out.push_str(CSS_STYLE);
    out.push_str("</head>\n<body id='dillo_bm'>\n\n  <h1>Bookmarks</h1>\n");
    out.push_str(
        "  <div style='float: right; padding: 1em'>\n    \
         [<a href='dpi:/bm/modify'>Modify</a>]\n  </div>\n",
    );
    out.push_str("<nav>\n  <ul>\n");
    for (i, s) in st.secs.iter().enumerate() {
        if i > 0 {
            out.push_str(" | \n");
        }
        out.push_str(&format!(
            "    <li><a href='#s{}'>{}</a></li>\n",
            s.section, s.title
        ));
    }
    out.push_str("  </ul>\n</nav>\n\n");
    for s in &st.secs {
        out.push_str(&format!(
            "<h2 id='s{}'>{}</h2>\n<ul class='main'>\n",
            s.section,
            one_line(&s.title)
        ));
        for b in &st.bms {
            if b.section == s.section {
                out.push_str(&format!(
                    "  <li>\n    <a href='{}'>{}</a>\n  </li>\n",
                    b.url, b.title
                ));
            }
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</body>\n</html>\n");
    out
}

/// `Bmsrv_send_modify_page` verbatim (formulaire + checkboxes).
#[must_use]
pub fn modify_page(st: &BmStore) -> String {
    let mut out = String::from(DOCTYPE);
    out.push_str("<html>\n<head>\n<title>Bookmarks</title>\n");
    out.push_str(CSS_STYLE);
    out.push_str("</head>\n<body id='dillo_bm'>\n  <h1>Bookmarks</h1>\n\n");
    out.push_str(
        "<form action='modify'>\n<div class='modify-menu'>\n  \
         <div style='float: right'>\n    [<a href='dpi:/bm/'>Dismiss</a>]\n  </div>\n\
         \x20 <p>Select an operation\n  <select name='operation'>\n    \
         <option value='none' selected>--\n    <option value='delete'>Delete\n    \
         <option value='move'>Move\n    <option value='modify'>Modify\n    \
         <option value='add_sec'>Add Section\n    <option value='add_url'>Add URL\n  \
         </select>\n  , mark its operands and\n  \
         <input type='submit' name='submit' value='Submit'>\n  \
         <p>You can also modify the bookmarks by editing the\n  \
         <a href='file:~/.dillo/bm.txt'><code>~/.dillo/bm.txt</code></a> file.\n\
         </div>\n",
    );
    out.push_str("<nav>\n  <ul>\n");
    for (i, s) in st.secs.iter().enumerate() {
        if i > 0 {
            out.push_str(" | \n");
        }
        out.push_str(&format!(
            "    <li><input type='checkbox' name='s{}'><a href='#s{}'>{}</a></li>\n",
            s.section, s.section, s.title
        ));
    }
    out.push_str("  </ul>\n</nav>\n\n");
    for s in &st.secs {
        out.push_str(&format!(
            "<h2 id='s{}'>{}</h2>\n<ul class='main'>\n",
            s.section,
            one_line(&s.title)
        ));
        for b in &st.bms {
            if b.section == s.section {
                out.push_str(&format!(
                    "  <li>\n    <input type='checkbox' name='url{}'>\n    \
                     <a href='{}'>{}</a>\n  </li>\n",
                    b.key, b.url, b.title
                ));
            }
        }
        out.push_str("</ul>\n");
    }
    out.push_str("</form>\n</body>\n</html>\n");
    out
}

/// `modifypage_add_section_page` verbatim (« Boorkmarks » — typo
/// verbatim amont).
#[must_use]
pub fn add_section_page() -> String {
    let mut out = String::from(DOCTYPE);
    out.push_str("<html>\n<head>\n<title>Bookmarks</title>\n");
    out.push_str(CSS_STYLE);
    out.push_str(
        "</head>\n<body id='dillo_bm'>\n<h1>Boorkmarks: New section</h1>\n\
         <form action='modify'>\n  \
         <input type='hidden' name='operation' value='add_section'>\n  <p>\n   \
         New section title:\n   <input type='text' name='title' size='32'>\n   \
         <input type='submit' name='submit' value='Submit'>\n   \
         or <a href='dpi:/bm/'>Cancel</a>\n  </p>\n</form>\n</body>\n</html>\n\n",
    );
    out
}

/// `modifypage_add_url` verbatim.
#[must_use]
pub fn add_url_page() -> String {
    let mut out = String::from(DOCTYPE);
    out.push_str("<html>\n<head>\n<title>Bookmarks</title>\n");
    out.push_str(CSS_STYLE);
    out.push_str(
        "</head>\n<body id='dillo_bm'>\n<h1>Bookmarks: Add URL</h1>\n\
         <form action='modify'>\n\
         <input type='hidden' name='operation' value='add_url2'>\n\
         <p>Title:\n  <input type='text' name='title' size='64'>\n</p>\n\
         <p>URL:\n  <input type='text' name='url' size='64'>\n</p>\n<p>\n  \
         <input type='submit' name='submit' value='Submit'>\n  \
         or <a href='dpi:/bm/'>Cancel</a>\n</p>\n</form>\n</body>\n</html>\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &std::path::Path) -> BmStore {
        BmStore::new(dir.join("bm.txt"))
    }

    #[test]
    fn load_save_roundtrip() {
        let dir = std::env::temp_dir().join("bmtest_rt");
        let _ = std::fs::create_dir_all(&dir);
        let mut st = store(&dir);
        // verbatim format bm.txt.
        st.load(":s0: Unclassified\n:s1: Docs\ns0 http://a/ A\ns1 http://b/ B title\n");
        assert_eq!(st.sections().len(), 2);
        assert_eq!(st.bookmarks().len(), 2);
        st.save();
        let txt = std::fs::read_to_string(dir.join("bm.txt")).unwrap_or_default();
        assert!(txt.contains(":s0: Unclassified"), "{txt}");
        assert!(txt.contains("s1 http://b/ B title"), "{txt}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalize_renumbers() {
        let dir = std::env::temp_dir().join("bmtest_n");
        let _ = std::fs::create_dir_all(&dir);
        let mut st = store(&dir);
        st.load(":s0: A\n:s0: B\ns7 http://a/ A\n"); // s7 → section inexistante
        st.save();
        // verbatim : bookmarks dans section absente restent tels quels
        // (normalize ne touche que les o_sec présents).
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unencode_verbatim() {
        assert_eq!(unencode("a+b%20c"), "a b c");
        assert_eq!(unencode("x%0D%0Ay"), "x\ny");
        assert_eq!(unencode("a%41"), "aA");
    }

    #[test]
    fn marked_counts() {
        let (ns, nu) = BmStore::count_marked("dpi:/bm/modify?operation=delete&s1=&url3=&s2=");
        assert_eq!((ns, nu), (2, 1));
        assert_eq!(BmStore::marked_secs("x?y&s5=&s7="), vec![5, 7]);
        assert_eq!(BmStore::marked_urls("x?y&url4=&url9="), vec![4, 9]);
    }

    #[test]
    fn modify_ops() {
        let dir = std::env::temp_dir().join("bmtest_ops");
        let _ = std::fs::create_dir_all(&dir);
        let mut st = store(&dir);
        st.load(":s0: A\n:s1: B\ns0 http://a/ tA\ns1 http://b/ tB\n");
        // move : section cible = s1… urls marquées url1/url2 (keys).
        let k = st.bookmarks()[0].key;
        assert_eq!(st.modify_move(&format!("x?operation=move&s1=&url{k}=&")), 0);
        assert_eq!(st.bookmarks()[0].section, 1);
        // delete section 1 → les DEUX bookmarks dedans (verbatim :
        // le bm déplacé + celui d'origine).
        st.modify_delete("x?operation=delete&s1=");
        assert_eq!(st.sections().len(), 1);
        assert_eq!(st.bookmarks().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

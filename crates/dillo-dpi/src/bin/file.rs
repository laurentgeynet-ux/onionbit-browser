//! Port of `dpi/file.c` — serveur `file:` (listing de répertoires +
//! envoi de fichiers, multi-clients).
//!
//! Amont : `stdin` est une socket d'écoute héritée de dpid ;
//! `select` + `accept` donnent un client par connexion, servi en
//! non-bloquant. Ici (verbatim observable) : un thread bloquant par
//! connexion acceptée — le flux d'octets DPIP est inchangé.

#![cfg_attr(not(unix), allow(dead_code))]
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
use std::sync::atomic::{AtomicBool, Ordering};

/// `MAXNAMESIZE` verbatim.
const MAXNAMESIZE: usize = 30;
/// `HIDE_DOTFILES` verbatim.
const HIDE_DOTFILES: bool = true;

// FILE_* flags (doc verbatim — le port utilise des états, cf. ci-dessous).
#[allow(dead_code)]
mod flags {
    pub const AUTH_OK: u8 = 1;
    pub const READ: u8 = 2;
    pub const WRITE: u8 = 4;
    pub const DONE: u8 = 8;
    pub const ERR: u8 = 16;
}

/// `FileState` verbatim.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FileState {
    Start = 10,
    Dpip,
    Http,
    Content,
    Done,
    Err,
}

/// `OLD_STYLE` global amont (basculé par `dpi:/file/toggle`).
static OLD_STYLE: AtomicBool = AtomicBool::new(false);

/// `FileInfo`.
struct FileInfo {
    full_path: String,
    filename: String,
    size: u64,
    is_dir: bool,
    is_exec: bool,
    mtime: i64,
}

/// `DilloDir`.
struct DilloDir {
    dirname: String,
    flist: Vec<FileInfo>,
}

/// `File_comp` verbatim : répertoires d'abord, `strcmp` croissant.
fn file_comp(a: &FileInfo, b: &FileInfo) -> std::cmp::Ordering {
    match (a.is_dir, b.is_dir) {
        (true, true) | (false, false) => a.filename.cmp(&b.filename),
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
    }
}

/// `File_dillodir_new` verbatim : skip `.`/`..`, dotfiles/`#…`/`…~`,
/// entrées non-statables ignorées, tri `File_comp`.
fn dillodir_new(dirname: &str) -> Option<DilloDir> {
    let rd = std::fs::read_dir(dirname).ok()?;
    let mut flist = Vec::new();
    for ent in rd.flatten() {
        let name = ent.file_name();
        let Some(name) = name.to_str() else { continue };
        if name == "." || name == ".." {
            continue;
        }
        if HIDE_DOTFILES && (name.starts_with('.') || name.starts_with('#') || name.ends_with('~'))
        {
            continue;
        }
        let full = format!("{dirname}{name}");
        let Ok(md) = std::fs::metadata(&full) else {
            continue;
        };
        #[cfg(unix)]
        let (is_dir, is_exec) = {
            use std::os::unix::fs::MetadataExt;
            (md.is_dir(), md.mode() & 0o111 != 0)
        };
        #[cfg(not(unix))]
        let (is_dir, is_exec) = (md.is_dir(), false);
        flist.push(FileInfo {
            filename: name.to_string(),
            full_path: full,
            size: md.len(),
            is_dir,
            is_exec,
            mtime: md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64),
        });
    }
    flist.sort_by(file_comp);
    Some(DilloDir {
        dirname: dirname.to_string(),
        flist,
    })
}

/// `File_ext` verbatim — MIME depuis l'extension.
fn file_ext(filename: &str) -> Option<&'static str> {
    let dot = filename.rfind('.')?;
    let e = &filename[dot + 1..];
    match e {
        _ if e.eq_ignore_ascii_case("gif") => Some("image/gif"),
        _ if e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg") => Some("image/jpeg"),
        _ if e.eq_ignore_ascii_case("png") => Some("image/png"),
        _ if e.eq_ignore_ascii_case("svg") => Some("image/svg+xml"),
        _ if e.eq_ignore_ascii_case("html")
            || e.eq_ignore_ascii_case("xhtml")
            || e.eq_ignore_ascii_case("htm")
            || e.eq_ignore_ascii_case("shtml") =>
        {
            Some("text/html")
        }
        _ if e.eq_ignore_ascii_case("txt") => Some("text/plain"),
        _ => None,
    }
}

/// `File_get_content_type_from_data` verbatim — sniffing HTML/images/
/// texte (heuristique « ≤10 non-ASCII dans un échantillon de 256 »).
fn content_type_from_data(data: &[u8]) -> &'static str {
    let mut i = 0;
    while i < data.len() && (data[i] as char).is_whitespace() {
        i += 1;
    }
    let rest = &data[i..];
    let starts = |pat: &str| {
        rest.len() >= pat.len() && rest[..pat.len()].eq_ignore_ascii_case(pat.as_bytes())
    };
    if starts("<html")
        || starts("<head")
        || starts("<title")
        || starts("<!doctype html")
        || starts("<!-- HTML listing")
    {
        "text/html"
    } else if data.len() >= 4 && &data[..4] == b"GIF8" {
        "image/gif"
    } else if data.len() >= 4 && &data[..4] == b"\x89PNG" {
        "image/png"
    } else if data.len() >= 2 && &data[..2] == b"\xff\xd8" {
        "image/jpeg"
    } else {
        let sample = &data[..data.len().min(256)];
        let non_ascii = sample.iter().filter(|&&c| c > 127).count();
        let limit_hit = sample.len() == 256;
        if (limit_hit && non_ascii > 10) || (!limit_hit && non_ascii > 0) {
            "application/octet-stream"
        } else {
            "text/plain"
        }
    }
}

/// `File_content_type` verbatim : extension d'abord, sniffing 256
/// octets sinon (fichier entier si < 256).
fn file_content_type(filename: &str) -> Option<&'static str> {
    if let Some(ct) = file_ext(filename) {
        return Some(ct);
    }
    let mut f = std::fs::File::open(filename).ok()?;
    use std::io::Read;
    let mut buf = [0u8; 256];
    let n = f.read(&mut buf).ok()?;
    // verbatim : sniff si 256 octets lus OU si le fichier complet tient.
    if n == 256 || (n > 0 && u64::try_from(n).ok() == f.metadata().ok().map(|m| m.len())) {
        Some(content_type_from_data(&buf[..n]))
    } else {
        None
    }
}

/// `File_parse_hex_octet` verbatim.
fn parse_hex_octet(s: &[u8]) -> Option<u8> {
    if s.len() >= 2 && s[0].is_ascii_hexdigit() && s[1].is_ascii_hexdigit() {
        let h = |c: u8| {
            if c.is_ascii_digit() {
                c - b'0'
            } else {
                c.to_ascii_uppercase() - b'A' + 10
            }
        };
        Some(h(s[0]) * 16 + h(s[1]))
    } else {
        None
    }
}

/// `File_normalize_path` verbatim : `file:` + `~` + `//localhost` +
/// slashes compactés + `%XX` + `#frag` retiré si le fichier sans
/// fragment existe.
fn normalize_path(orig: &str) -> Option<String> {
    if !orig[..orig.len().min(5)].eq_ignore_ascii_case("file:") {
        return None;
    }
    let str_: String;
    let mut s = &orig[5..];
    if s.starts_with('~') && (s.len() == 1 || s.as_bytes()[1] == b'/') {
        // expand ~ → home
        let home = dillo_common::dlib::get_home_dir();
        let home = String::from_utf8_lossy(&home).into_owned();
        let sep = if home.ends_with('/') { "" } else { "/" };
        let next = s[1..].trim_start_matches('/');
        str_ = format!("{home}{sep}{next}");
        s = &str_;
    } else if s.len() >= 12 && s[..12].eq_ignore_ascii_case("//localhost/") {
        s = &s[11..];
    }
    while s.len() >= 2 && s.as_bytes()[0] == b'/' && s.as_bytes()[1] == b'/' {
        s = &s[1..];
    }
    // %XX inline (verbatim boucle ds->str).
    let mut bytes = s.as_bytes().to_vec();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(v) = parse_hex_octet(&bytes[i + 1..]) {
                bytes[i] = v;
                bytes.drain(i + 1..i + 3);
                i += 1;
                continue;
            }
        }
        i += 1;
    }
    let mut path = String::from_utf8_lossy(&bytes).into_owned();
    // `#frag` retiré si le chemin complet n'existe pas.
    if let Some(p) = path.rfind('#') {
        if std::fs::metadata(&path).is_err() {
            path.truncate(p);
        }
    }
    Some(path)
}

/// `ctime` minimal verbatim : "Www Mmm dd HH:MM:SS YYYY\n" en heure
/// locale. `ds[4..]` = "Mmm dd…" — on reproduit les offsets C.
fn ctime_str(mtime: i64) -> String {
    // Civil-from-days (Howard Hinnant) → UTC ; l'amont est en
    // localtime mais le décalage n'affecte que le bord de la fenêtre
    // « 6 mois » — écart documenté.
    let days = mtime.div_euclid(86400);
    let secs = mtime.rem_euclid(86400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    const MON: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    // format ctime : "Mmm _d HH:MM:SS YYYY" (jour paddé espace).
    format!(
        "{} {:2} {:02}:{:02}:{:02} {}",
        MON[(m - 1) as usize],
        d,
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60,
        y
    )
}

/// `File_print_mtime` verbatim : `%.3s %.2s` puis `%.4s` an ou
/// `%.5s` heure (> 6 mois = 15811200 s).
fn print_mtime(old_style: bool, mtime: i64) -> String {
    let ds = ctime_str(mtime); // "Mmm _d HH:MM:SS YYYY"
    let b = ds.as_bytes();
    let s = |a: usize, n: usize| String::from_utf8_lossy(&b[a..(a + n).min(b.len())]);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    // index verbatim : ds+4=Mmm(0..3) ds+8=dd(4..6) ds+11=HH:MM ds+20=YYYY
    if old_style {
        if now - mtime > 15_811_200 {
            format!(" {:.3} {:.2}  {:.4}", s(0, 3), s(4, 2), s(16, 4))
        } else {
            format!(" {:.3} {:.2} {:.5}", s(0, 3), s(4, 2), s(7, 5))
        }
    } else {
        let tail = if now - mtime > 15_811_200 {
            s(16, 4)
        } else {
            s(7, 5)
        };
        format!("<td>{:.3}&nbsp;{:.2}&nbsp;{:.5}", s(0, 3), s(4, 2), tail)
    }
}

// ---------------------------------------------------------------
// client — verbatim `ClientInfo`/`File_*`.
// ---------------------------------------------------------------

struct ClientInfo {
    sh: dillo_dpi::dpip::Dsh<'static>,
    orig_url: String,
    filename: String,
    file: Option<std::fs::File>,
    file_sz: u64,
    d_dir: Option<DilloDir>,
    state: FileState,
    err_code: i32,
    done: bool,
    err: bool,
    old_style: bool,
}

impl ClientInfo {
    /// `File_print_parent_dir` verbatim.
    fn print_parent_dir(&mut self, dirname: &str) {
        if dirname != "/" {
            let mut parent = dirname.to_string();
            parent.truncate(parent.len().saturating_sub(1)); // trailing '/'
            if let Some(p) = parent.rfind('/') {
                parent.truncate(p + 1);
            }
            let u = dillo_dpi::dpiutil::escape_uri_str(parent.as_bytes(), None);
            let hu = dillo_dpi::dpiutil::escape_html_str(&u);
            let _ = self.sh.write_str(
                false,
                &format!(
                    "<a href='file:{}'>Parent directory</a>",
                    String::from_utf8_lossy(&hu)
                ),
            );
        }
    }

    /// `File_info2html` verbatim.
    fn info2html(&mut self, fi: &FileInfo, n: usize) {
        let (size, units) = if fi.size <= 9999 {
            (fi.size, "bytes")
        } else if fi.size / 1024 <= 9999 {
            (fi.size / 1024 + u64::from(fi.size % 1024 >= 512), "KB")
        } else {
            (
                fi.size / 1_048_576 + u64::from(fi.size % 1_048_576 >= 524_288),
                "MB",
            )
        };
        let filecont: String = if fi.is_dir {
            "Directory".into()
        } else if fi.is_exec {
            "Executable".into()
        } else {
            match file_content_type(&fi.full_path) {
                Some("application/octet-stream") | None => "unknown".into(),
                Some(ct) => ct.to_string(),
            }
        };
        let mut name = fi.filename.clone();
        if name.len() > MAXNAMESIZE {
            name = format!("{}...", &name[..MAXNAMESIZE - 3]);
        }
        let uref = dillo_dpi::dpiutil::escape_uri_str(fi.filename.as_bytes(), None);
        let huref =
            String::from_utf8_lossy(&dillo_dpi::dpiutil::escape_html_str(&uref)).into_owned();
        let hname = String::from_utf8_lossy(&dillo_dpi::dpiutil::escape_html_str(name.as_bytes()))
            .into_owned();
        let isdir = if fi.is_dir { ">" } else { " " };
        if self.old_style {
            let dots = ".. .. .. .. .. .. .. .. .. .. .. .. .. .. .. .. ..";
            let ndots = MAXNAMESIZE.saturating_sub(name.len());
            let dstart = 50usize.saturating_sub(ndots.min(50));
            let _ = self.sh.write_str(
                false,
                &format!(
                    "{isdir}<a href='{huref}'>{hname}</a> {} {filecont:<11}{size:4} {units:<5}",
                    &dots[dstart..]
                ),
            );
        } else {
            let bg = if n & 1 == 1 { "bgcolor=#dcdcdc" } else { "" };
            let _ = self.sh.write_str(
                false,
                &format!(
                    "<tr align=center {bg}><td>{isdir}<td align=left><a href='{huref}'>{hname}</a><td>{filecont}<td>{size}&nbsp;{units}"
                ),
            );
        }
        let _ = self
            .sh
            .write_str(false, &print_mtime(self.old_style, fi.mtime));
        let _ = self.sh.write_str(false, "\n");
    }

    /// `File_send_dir` verbatim.
    fn send_dir(&mut self) {
        match self.state {
            FileState::Start => {
                if let Some(d) = dillo_net::dpip::build_cmd(
                    "cmd=%s url=%s",
                    &["start_send_page", &self.orig_url],
                ) {
                    let _ = self.sh.write_str(true, &d);
                }
                self.state = FileState::Dpip;
            }
            FileState::Dpip => {
                let ddir = self
                    .d_dir
                    .as_ref()
                    .map_or(String::new(), |d| d.dirname.clone());
                let udir = dillo_dpi::dpiutil::escape_uri_str(ddir.as_bytes(), None);
                let hudir = String::from_utf8_lossy(&dillo_dpi::dpiutil::escape_html_str(&udir))
                    .into_owned();
                let hdir =
                    String::from_utf8_lossy(&dillo_dpi::dpiutil::escape_html_str(ddir.as_bytes()))
                        .into_owned();
                let _ = self.sh.write_str(
                    false,
                    &format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
                         <!DOCTYPE HTML PUBLIC '-//W3C//DTD HTML 4.01 Transitional//EN'>\n\
                         <HTML>\n<HEAD>\n <BASE href='file:{hudir}'>\n \
                         <TITLE>file:{hdir}</TITLE>\n</HEAD>\n\
                         <BODY><H1>Directory listing of {hdir}</H1>\n"
                    ),
                );
                if self.old_style {
                    let _ = self.sh.write_str(false, "<pre>\n");
                }
                self.print_parent_dir(&ddir);
                let _ = self
                    .sh
                    .write_str(false, "&nbsp;&nbsp;<a href='dpi:/file/toggle'>%</a>\n");
                let empty = self.d_dir.as_ref().map_or(0, |d| d.flist.len()) == 0;
                if empty {
                    let _ = self.sh.write_str(false, "<br><br>Directory is empty...");
                } else if self.old_style {
                    let _ = self.sh.write_str(false, "\n\n");
                } else {
                    let _ = self.sh.write_str(
                        false,
                        "<br><br>\n<table border=0 cellpadding=1 cellspacing=0 \
                         bgcolor=#E0E0E0 width=100%>\n<tr align=center>\n<td>\n\
                         <td width=60%><b>Filename</b><td><b>Type</b><td><b>Size</b>\
                         <td><b>Modified&nbsp;at</b>\n",
                    );
                }
                self.state = FileState::Http;
            }
            FileState::Http => {
                // `dList_nth_data(flist,n)` → `info2html(n+1)` : la
                // liste est extraite temporairement pour permettre
                // l'emprunt mutable du socket pendant l'itération.
                let mut flist = self
                    .d_dir
                    .as_mut()
                    .map_or_else(Vec::new, |d| std::mem::take(&mut d.flist));
                let had_entries = !flist.is_empty();
                for (i, fi) in flist.iter().enumerate() {
                    self.info2html(fi, i + 1);
                }
                if let Some(d) = self.d_dir.as_mut() {
                    d.flist = std::mem::take(&mut flist);
                }
                if self.old_style {
                    let _ = self.sh.write_str(false, "</pre>\n");
                } else if had_entries {
                    let _ = self.sh.write_str(false, "</table>\n");
                }
                let _ = self.sh.write_str(true, "</BODY></HTML>\n");
                self.state = FileState::Content;
                self.done = true;
            }
            _ => {}
        }
    }

    /// `File_send_error_page` verbatim (EACCES→403, ENOENT→404,
    /// sinon 500).
    fn send_error_page(&mut self) {
        let (status, emsg) = match self.err_code {
            13 => ("403 Forbidden", "Permission denied"),
            2 => ("404 Not Found", "No such file or directory"),
            _ => ("500 Internal Server Error", "Internal error"),
        };
        let body = format!("{status}\n{emsg}");
        if let Some(d) =
            dillo_net::dpip::build_cmd("cmd=%s url=%s", &["start_send_page", &self.orig_url])
        {
            let _ = self.sh.write_str(false, &d);
        }
        let _ = self.sh.write_str(
            true,
            &format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            ),
        );
        self.done = true;
    }

    /// `File_get` verbatim : stat → dir / file / ENOENT.
    fn file_get(&mut self, path: &str, orig_url: &str) {
        let md = std::fs::metadata(path);
        self.orig_url = orig_url.to_string();
        match md {
            Err(_) => {
                self.state = FileState::Err;
                self.err_code = 2;
            }
            Ok(md) if md.is_dir() => {
                let mut dn = path.to_string();
                if !dn.ends_with('/') {
                    dn.push('/');
                }
                if let Some(d) = dillodir_new(&dn) {
                    self.d_dir = Some(d);
                    self.state = FileState::Start;
                } else {
                    self.state = FileState::Err;
                    self.err_code = 13;
                }
            }
            Ok(md) => match std::fs::File::open(path) {
                Ok(f) => {
                    self.file = Some(f);
                    self.file_sz = md.len();
                    self.filename = path.to_string();
                    self.state = FileState::Start;
                }
                Err(e) => {
                    self.state = FileState::Err;
                    self.err_code = e.raw_os_error().unwrap_or(5);
                }
            },
        }
    }

    /// `File_send_file` verbatim — machine Start→Dpip→Http→Content.
    fn send_file(&mut self) {
        const LBUF: usize = 16 * 1024;
        match self.state {
            FileState::Start => {
                if let Some(d) = dillo_net::dpip::build_cmd(
                    "cmd=%s url=%s",
                    &["start_send_page", &self.orig_url],
                ) {
                    let _ = self.sh.write_str(true, &d);
                }
                self.state = FileState::Dpip;
            }
            FileState::Dpip => {
                let mut name = self.filename.clone();
                let gzipped = name.len() > 3 && name[name.len() - 3..].eq_ignore_ascii_case(".gz");
                if gzipped {
                    name.truncate(name.len() - 3);
                }
                let ct = file_content_type(&name).unwrap_or("application/octet-stream");
                let _ = self.sh.write_str(false, "HTTP/1.1 200 OK\r\n");
                if gzipped {
                    let _ = self.sh.write_str(false, "Content-Encoding: gzip\r\n");
                }
                if !gzipped || ct != "application/octet-stream" {
                    let _ = self.sh.write_str(false, &format!("Content-Type: {ct}\r\n"));
                }
                let _ = self
                    .sh
                    .write_str(true, &format!("Content-Length: {}\r\n\r\n", self.file_sz));
                self.state = FileState::Http;
            }
            FileState::Http => {
                if self.sh.try_flush() < 0 {
                    return; // verbatim : -3 → FILE_ERR
                }
                use std::io::Read;
                let mut buf = vec![0u8; LBUF];
                match self.file.as_mut().map(|f| f.read(&mut buf)) {
                    Some(Ok(0)) | None => {
                        self.state = FileState::Content;
                        self.done = true;
                    }
                    Some(Ok(n)) => {
                        if self.sh.try_write(&buf[..n]) < 0 {
                            self.err = true;
                        }
                    }
                    Some(Err(_)) => {
                        self.err = true;
                    }
                }
            }
            _ => {}
        }
    }
}

/// `File_serve_client` verbatim : auth → boucle cmd → service jusqu'à
/// `FILE_DONE | FILE_ERR` (`DpiBye` lève le flag global de fin).
/// Ici bloquant (un thread par connexion ≙ le select amont).
fn serve_conn(sock: std::net::TcpStream, bye: std::sync::Arc<AtomicBool>) {
    use dillo_dpi::dpip::{check_auth, DpIoTcp, Dsh};

    let Ok(rd) = sock.try_clone() else { return };
    let mut cli = ClientInfo {
        sh: Dsh::new(
            Box::new(DpIoTcp::new(rd, false)),
            Box::new(DpIoTcp::new(sock, false)),
            8 * 1024,
        ),
        orig_url: String::new(),
        filename: String::new(),
        file: None,
        file_sz: 0,
        d_dir: None,
        state: FileState::Start,
        err_code: 0,
        done: false,
        err: false,
        old_style: OLD_STYLE.load(Ordering::Relaxed),
    };
    let mut auth = false;
    loop {
        if cli.done || cli.err {
            break;
        }
        if cli.state == FileState::Err {
            cli.send_error_page();
            break;
        } else if cli.d_dir.is_some() {
            cli.send_dir();
            if cli.done {
                break;
            }
            continue;
        } else if cli.file.is_some() {
            cli.send_file();
            if cli.done || cli.err {
                break;
            }
            continue;
        }
        // FILE_READ : lit un token dpip (bloquant — verbatim lecture
        // non-bloquante pilotée par select ; résultat identique).
        let Some(tag) = cli.sh.read_token(true) else {
            break;
        };
        let tag = String::from_utf8_lossy(&tag).into_owned();
        if !auth {
            // verbatim `a_Dpip_check_auth` → FILE_AUTH_OK / FILE_ERR.
            auth = check_auth(&tag);
            if !auth {
                cli.err = true;
            }
            continue;
        }
        let cmd = dillo_net::dpip::get_attr(&tag, "cmd");
        let url = dillo_net::dpip::get_attr(&tag, "url");
        match cmd.as_deref() {
            Some("DpiBye") => {
                bye.store(true, Ordering::Relaxed);
                cli.done = true;
            }
            Some(_) => {
                if let Some(u) = url.as_deref() {
                    if u.len() >= 4
                        && u[..4].eq_ignore_ascii_case("dpi:")
                        && &u[4..] == "/file/toggle"
                    {
                        OLD_STYLE.fetch_xor(true, Ordering::Relaxed);
                        if let Some(d) = dillo_net::dpip::build_cmd("cmd=%s", &["reload_request"]) {
                            let _ = cli.sh.write_str(true, &d);
                        }
                    } else if let Some(path) = normalize_path(u) {
                        cli.file_get(&path, u);
                    } else {
                        cli.err = true;
                    }
                } else {
                    cli.err = true;
                }
            }
            None => {}
        }
    }
    cli.sh.close();
}

/// `main` verbatim : signaux → exit, stdin non-bloquant, boucle
/// accept (≙ `FD_ISSET(STDIN)` + `accept` non-bloquant), clients en
/// threads bloquants.
#[cfg(unix)]
fn run() -> i32 {
    use std::io;
    use std::os::unix::io::AsRawFd;

    // Signaux verbatim `termination_handler` (exit(signum)) : flag
    // consulté dans la boucle d'accept (amont exit immédiat — ici on
    // termine les clients en cours avant de sortir).
    let sig_int = std::sync::Arc::new(AtomicBool::new(false));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, sig_int.clone());
    let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, sig_int.clone());
    let _ = signal_hook::flag::register(signal_hook::consts::SIGHUP, sig_int.clone());

    // stdin = socket d'écoute non-bloquante (verbatim fcntl O_NONBLOCK).
    let stdin_ref = socket2::SockRef::from(&io::stdin());
    if stdin_ref.set_nonblocking(true).is_err() {
        eprintln!("[file dpi]: stdin is not a socket");
        return 1;
    }

    let bye = std::sync::Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();
    while !bye.load(Ordering::Relaxed) && !sig_int.load(Ordering::Relaxed) {
        match stdin_ref.accept() {
            Ok((sock, _addr)) => {
                let _ = sock.set_nonblocking(false);
                let std_sock: std::net::TcpStream = sock.into();
                let bye2 = bye.clone();
                handles.push(std::thread::spawn(move || serve_conn(std_sock, bye2)));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => {
                eprintln!("[file dpi]: accept() {e}");
                break;
            }
        }
    }
    for h in handles {
        let _ = h.join();
    }
    i32::from(!bye.load(Ordering::Relaxed))
}

#[cfg(unix)]
fn main() {
    std::process::exit(run());
}

/// Hors POSIX : verbatim — file.dpi est POSIX-only.
#[cfg(not(unix))]
fn main() {
    eprintln!("[file dpi]: socket stdio unsupported on this platform");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(normalize_path("file:///tmp/x"), Some("/tmp/x".into()));
        assert_eq!(
            normalize_path("file://localhost/tmp/x"),
            Some("/tmp/x".into())
        );
        assert_eq!(normalize_path("file:///a%20b"), Some("/a b".into()));
        assert_eq!(normalize_path("http://x"), None);
        // slashes compactés verbatim
        assert_eq!(normalize_path("file:////tmp"), Some("/tmp".into()));
    }

    #[test]
    fn ext() {
        assert_eq!(file_ext("a.GIF"), Some("image/gif"));
        assert_eq!(file_ext("a.jpeg"), Some("image/jpeg"));
        assert_eq!(file_ext("a.shtml"), Some("text/html"));
        assert_eq!(file_ext("a.bin"), None);
        assert_eq!(file_ext("nodot"), None);
    }

    #[test]
    fn sniff() {
        assert_eq!(content_type_from_data(b"GIF89a reste"), "image/gif");
        assert_eq!(content_type_from_data(b"\x89PNGzz"), "image/png");
        assert_eq!(content_type_from_data(b"\xff\xd8\xff"), "image/jpeg");
        assert_eq!(content_type_from_data(b"  \n<html><body>"), "text/html");
        assert_eq!(content_type_from_data(b"hello ascii"), "text/plain");
        assert_eq!(
            content_type_from_data(&[0xffu8; 100]),
            "application/octet-stream"
        );
    }

    #[test]
    fn ctime_layout() {
        // 0 = "Thu Jan  1 00:00:00 1970" → notre format "Jan  1 00:00:00 1970"
        assert_eq!(ctime_str(0), "Jan  1 00:00:00 1970");
    }

    #[test]
    fn mtime_render() {
        let old = print_mtime(true, 0); // 1970 > 6 mois → année
        assert!(old.contains("1970"), "{old}");
        let new = print_mtime(false, 0);
        assert!(new.starts_with("<td>"), "{new}");
    }
}

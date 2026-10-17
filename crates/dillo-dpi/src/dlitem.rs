//! Port of `dpi/downloads.cc` — logique `DLItem` (sans les widgets
//! FLTK, qui vivent dans le binaire).
//!
//! Verbatim : `wget -U <ua> -c --load-cookies ~/.dillo/cookies.txt
//! -O <dest> <url-escapée>` avec stderr → pipe de log ; FSM qui
//! retire le « dot-progress » de wget (`^ `, `^[0-9]+K`) ; scan de
//! `Length: N` ; rates (~, courant 2 s) et ETA identiques à amont.

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

/// `LogPipe` FSM verbatim.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LogState {
    Newline,
    Number,
    Discard,
    Copy,
}

/// `DLItem` — un téléchargement (modèle verbatim ; les labels FLTK
/// sont des chaînes du modèle ici).
pub struct DlItem {
    /// `fullname` — destination complète.
    pub fullname: String,
    /// `shortname` — basename après le dernier `/` (« ?? » sinon).
    pub shortname: String,
    /// `target_dir` — jusqu'au dernier `/` inclus (« ?? » sinon).
    pub target_dir: String,
    /// `esc_url` ≙ `Escape_uri_str(url,"'")` + `Filter_smtp_hack`.
    pub esc_url: String,
    /// `cookies_path` ≙ `~/.dillo/cookies.txt`.
    pub cookies_path: String,
    /// `dl_argv`.
    pub argv: Vec<String>,
    /// `log_text` (tooltip ≙ `prTitle->tooltip`).
    pub log_text: String,
    log_state: LogState,
    /// processus wget (≙ `mPid`).
    pub child: Option<std::process::Child>,
    /// Récepteur des chunks stderr (≙ `LogPipe[0]` — lu par un
    /// thread puisque `std` n'expose pas de pipe non-bloquant).
    log_rx: Option<std::sync::mpsc::Receiver<String>>,
    /// `init_time`/`twosec_time`/`onesec_time` (verbatim `time_t`).
    init_time: i64,
    twosec_time: i64,
    onesec_time: i64,
    twosec_bytesize: i64,
    onesec_bytesize: i64,
    init_bytesize: i64,
    /// `curr_bytesize`.
    pub curr_bytesize: i64,
    /// `total_bytesize` (-1 ≙ inconnu).
    pub total_bytesize: i64,
    /// `ForkDone` — wget terminé.
    pub fork_done: bool,
    /// `LogDone`.
    pub log_done: bool,
    /// `UpdatesDone`.
    pub updates_done: bool,
    /// `WidgetDone` — l'item est à retirer.
    pub widget_done: bool,
    /// `WgetStatus` — code de sortie (-1 ≙ inconnu).
    pub wget_status: i32,
    // — champs « label » verbatim —
    /// `prTitle` (court nom, ou « Connecting… »).
    pub title: String,
    /// `prGot` (« 0KB », « 1.5MB »).
    pub got: String,
    /// `prSize` (« ?? » puis taille).
    pub size: String,
    /// `prRate` (débit instantané).
    pub rate: String,
    /// `pr_Rate` (débit moyen).
    pub avg_rate: String,
    /// `prETA`.
    pub eta: String,
    /// `prETAt` (devient « Time » à la fin).
    pub eta_title: String,
    /// `prBar` : (position 0..100, message optionnel « Done »/« ABORTED »).
    pub bar: (f64, Option<String>),
    /// `prButton` : « Stop »/« Done »/« Close » + actif.
    pub button: (String, bool),
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// `escape_tooltip` verbatim : double les `@` (FLTK @-symbols).
fn escape_tooltip(s: &str) -> String {
    s.replace('@', "@@")
}

/// `secs2timestr` verbatim.
fn secs2timestr(et: i64) -> String {
    let (eh, em, es) = (et / 3600, (et % 3600) / 60, et % 60);
    if eh == 0 {
        if em == 0 {
            format!("{es}s")
        } else {
            format!("{em}m{es}s")
        }
    } else {
        format!("{eh}h{em}m")
    }
}

/// `update_prSize` verbatim (`%.1fMB` / `%.0fKB`).
fn size_str(n: i64) -> String {
    if n > 1024 * 1024 {
        format!("{:.1}MB", n as f64 / 1_048_576.0)
    } else {
        format!("{:.0}KB", n as f64 / 1024.0)
    }
}

impl DlItem {
    /// `DLItem` ctor verbatim : argv wget, log init « Target File: ».
    /// `home` ≙ `dGethomedir()` (préfixe de `cookies_path`).
    #[must_use]
    pub fn new(full_filename: &str, url: &str, user_agent: &str, home: &str) -> Self {
        // verbatim : `strrchr('/') ? p+1 : "??"`.
        let shortname = full_filename
            .rsplit('/')
            .next()
            .map_or("??".to_string(), str::to_string);
        let target_dir = full_filename
            .rfind('/')
            .map_or("??".to_string(), |p| full_filename[..=p].to_string());
        // verbatim : `Escape_uri_str(url, "'")` puis smtp hack ftp:
        let mut esc = String::from_utf8_lossy(&dpiutil::escape_uri_str(url.as_bytes(), Some(b"'")))
            .into_owned();
        if esc[..esc.len().min(5)].eq_ignore_ascii_case("ftp:/") {
            esc = String::from_utf8_lossy(&dpiutil::filter_smtp_hack(esc.as_bytes())).into_owned();
        }
        let cookies_path = format!("{home}/.dillo/cookies.txt");
        // verbatim argv : wget [-c si fichier existant] -U ua -c
        // --load-cookies cookies -O dest url.
        let mut argv = vec!["wget".to_string()];
        let init_bytesize = std::fs::metadata(full_filename).map_or(0, |m| m.len() as i64);
        argv.push("-U".into());
        argv.push(user_agent.to_string());
        argv.push("-c".into());
        argv.push("--load-cookies".into());
        argv.push(cookies_path.clone());
        argv.push("-O".into());
        argv.push(full_filename.to_string());
        argv.push(esc.clone());
        let t = now();
        let mut it = Self {
            fullname: full_filename.to_string(),
            shortname: shortname.clone(),
            target_dir,
            esc_url: esc,
            cookies_path,
            argv,
            log_text: String::new(),
            log_state: LogState::Newline,
            child: None,
            log_rx: None,
            init_time: t,
            twosec_time: t,
            onesec_time: t,
            twosec_bytesize: 0,
            onesec_bytesize: 0,
            init_bytesize,
            curr_bytesize: 0,
            total_bytesize: -1,
            fork_done: false,
            log_done: false,
            updates_done: false,
            widget_done: false,
            wget_status: -1,
            title: shortname,
            got: "0KB".into(),
            size: "??".into(),
            rate: "??".into(),
            avg_rate: "??".into(),
            eta: "??".into(),
            eta_title: "ETA".into(),
            bar: (0.0, None),
            button: ("Stop".into(), true),
        };
        // verbatim : tooltip « Target File: <fullname>\n\n ».
        it.log_text_add("Target File: ");
        it.log_text_add(&it.fullname.clone());
        it.log_text_add("\n\n");
        it
    }

    /// `child_init` + spawn verbatim : `wget` avec stderr pipé
    /// (≙ `dup2(LogPipe[1],2)` amont — le log wget arrive sur stderr).
    /// `LC_ALL=C` verbatim pour parser le log.
    pub fn spawn(&mut self) -> std::io::Result<()> {
        use std::process::{Command, Stdio};
        let mut cmd = Command::new(&self.argv[0]);
        cmd.args(&self.argv[1..])
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut ch = cmd.spawn()?;
        // ≙ `Fl::add_fd(LogPipe[0])` : thread lecteur bloquant qui
        // forward les chunks ; disconnect ≙ EOF → `log_done`.
        if let Some(mut err) = ch.stderr.take() {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = [0u8; 4096];
                while let Ok(n) = err.read(&mut buf) {
                    if n == 0
                        || tx
                            .send(String::from_utf8_lossy(&buf[..n]).into_owned())
                            .is_err()
                    {
                        break;
                    }
                }
            });
            self.log_rx = Some(rx);
        }
        self.child = Some(ch);
        Ok(())
    }

    /// `log_text_add` verbatim : escape `@`, FSM dot-progress,
    /// scan « \nLength: ».
    pub fn log_text_add(&mut self, buf: &str) {
        let esc = escape_tooltip(buf);
        for &c in esc.as_bytes() {
            match self.log_state {
                LogState::Newline => {
                    if c == b' ' {
                        self.log_state = LogState::Discard;
                    } else if c.is_ascii_digit() {
                        self.log_text.push(c as char);
                        self.log_state = LogState::Number;
                    } else if c == b'\n' {
                        self.log_text.push('\n');
                    } else {
                        self.log_text.push(c as char);
                        self.log_state = LogState::Copy;
                    }
                }
                LogState::Number => {
                    if c.is_ascii_digit() {
                        self.log_text.push(c as char);
                    } else if c == b'K' {
                        while self
                            .log_text
                            .bytes()
                            .last()
                            .is_some_and(|b| b.is_ascii_digit())
                        {
                            self.log_text.pop();
                        }
                        self.log_state = LogState::Discard;
                    } else {
                        self.log_text.push(c as char);
                        self.log_state = LogState::Copy;
                    }
                }
                LogState::Discard => {
                    if c == b'\n' {
                        self.log_state = LogState::Newline;
                    }
                }
                LogState::Copy => {
                    self.log_text.push(c as char);
                    if c == b'\n' {
                        self.log_state = LogState::Newline;
                    }
                }
            }
        }
        // verbatim : scan « \nLength: <digits> » (une fois).
        if self.total_bytesize == -1 {
            if let Some(p) = self.log_text.find("\nLength: ") {
                let s = &self.log_text[p + 9..];
                if s.as_bytes().first().is_some_and(u8::is_ascii_digit) && s.contains(' ') {
                    let num: String = s
                        .bytes()
                        .take_while(|&b| b != b' ')
                        .filter(u8::is_ascii_digit)
                        .map(|b| b as char)
                        .collect();
                    if let Ok(n) = num.parse() {
                        self.total_bytesize = n;
                        self.size = size_str(n);
                    }
                }
            }
        }
        // verbatim : « Connecting… » tant que 0 octet.
        if self.curr_bytesize == 0 {
            self.title = "Connecting...".into();
        }
    }

    /// `read_log_cb` verbatim : draine les chunks du thread lecteur
    /// (≙ `read` jusqu'à `EAGAIN` ; `0`/déconnexion → `log_done`).
    pub fn drain_log(&mut self) {
        let mut disconnected = false;
        let mut chunks = Vec::new();
        if let Some(rx) = &self.log_rx {
            loop {
                match rx.try_recv() {
                    Ok(s) => chunks.push(s),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        for s in chunks {
            self.log_text_add(&s);
        }
        if disconnected {
            self.log_done = true;
        }
    }

    /// `abort_dl` verbatim : tue wget + marque done.
    pub fn abort(&mut self) {
        if !self.log_done {
            self.log_done = true;
            if !self.fork_done {
                if let Some(ch) = self.child.as_mut() {
                    let _ = ch.kill();
                }
            }
        }
        self.widget_done = true;
        self.button.1 = false;
    }

    /// `child_finished` verbatim.
    pub fn child_finished(&mut self, status: i32) {
        self.wget_status = status;
        if status == 0 {
            self.button = ("Done".into(), true);
        } else {
            self.button = ("Close".into(), true);
            self.bar.1 = Some("ABORTED".into());
            if self.curr_bytesize == 0 {
                self.title = self.shortname.clone();
            }
        }
    }

    /// `try_wait` ≙ `waitpid(WNOHANG)` dans `cleanup_cb`.
    pub fn poll_child(&mut self) {
        if self.fork_done {
            return;
        }
        if let Some(ch) = self.child.as_mut() {
            match ch.try_wait() {
                Ok(Some(st)) => {
                    self.fork_done = true;
                    self.child_finished(st.code().unwrap_or(-1));
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }
    }

    /// `update_size` verbatim : premier octet → `init_time` + titre ;
    /// barre : `move(1)` si taille inconnue sinon position %.
    fn update_size(&mut self, new_sz: i64) {
        if self.curr_bytesize == 0 && new_sz != 0 {
            self.init_time = now();
            self.title = self.shortname.clone();
        }
        self.curr_bytesize = new_sz;
        self.got = size_str(new_sz);
        if self.total_bytesize == -1 {
            // verbatim `prBar->move(1)` : `mPresent += step`, wrap à
            // `mMin` si `> mMax` ; showtext(false) ≙ pas de %.
            self.bar.0 += 1.0;
            if self.bar.0 > 100.0 {
                self.bar.0 = 0.0;
            }
        } else {
            let pos = if self.total_bytesize > 0 {
                100.0 * self.curr_bytesize as f64 / self.total_bytesize as f64
            } else {
                100.0
            };
            self.bar.0 = pos;
        }
    }

    /// `update` verbatim — stat du fichier, rates, ETA. Appelé 1/s.
    pub fn update(&mut self) {
        if self.updates_done {
            return;
        }
        let Ok(md) = std::fs::metadata(&self.fullname) else {
            return;
        };
        self.update_size(md.len() as i64);
        let curr_time = now();
        let csec = (curr_time - self.init_time) as f64;
        // Rate (2 s de recul).
        if csec >= 2.0 {
            let tsec = (curr_time - self.twosec_time) as f64;
            let rate = (self.curr_bytesize - self.twosec_bytesize) as f64 / 1024.0 / tsec;
            self.rate = if rate < 100.0 {
                format!("{rate:.1}K/s")
            } else {
                format!("{rate:.0}K/s")
            };
        }
        // ~Rate.
        let mut _rate = 0.0;
        if csec >= 1.0 {
            _rate = (self.curr_bytesize - self.init_bytesize) as f64 / 1024.0 / csec;
            self.avg_rate = if _rate < 100.0 {
                format!("{_rate:.1}K/s")
            } else {
                format!("{_rate:.0}K/s")
            };
        }
        // ETA.
        if self.fork_done {
            self.updates_done = true;
            self.eta_title = "Time".into();
            self.eta = secs2timestr(csec as i64);
            if self.total_bytesize == -1 {
                self.size = size_str(self.curr_bytesize);
                if self.wget_status == 0 {
                    self.bar.1 = Some("Done".into());
                }
            }
        } else if _rate > 0.0 && self.total_bytesize > 0 && self.curr_bytesize > 0 {
            let et = (self.total_bytesize - self.curr_bytesize) as f64 / (_rate * 1024.0);
            self.eta = secs2timestr(et as i64);
        }
        self.twosec_time = self.onesec_time;
        self.onesec_time = curr_time;
        self.twosec_bytesize = self.onesec_bytesize;
        self.onesec_bytesize = self.curr_bytesize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_verbatim() {
        let it = DlItem::new("/tmp/dl/f.iso", "http://h/f.iso", "Dillo/3.1", "/home/u");
        assert_eq!(
            it.argv,
            vec![
                "wget",
                "-U",
                "Dillo/3.1",
                "-c",
                "--load-cookies",
                "/home/u/.dillo/cookies.txt",
                "-O",
                "/tmp/dl/f.iso",
                "http://h/f.iso"
            ]
        );
        assert_eq!(it.shortname, "f.iso");
        assert_eq!(it.target_dir, "/tmp/dl/");
    }

    #[test]
    fn tooltip_escape() {
        assert_eq!(escape_tooltip("a@b@@c"), "a@@b@@@@c");
    }

    #[test]
    fn log_fsm() {
        let mut it = DlItem::new("/tmp/f", "http://h/f", "ua", "/h");
        it.log_text.clear();
        // verbatim : ligne «   5K ... » → digits+K jetés ; «  » jetés.
        it.log_text_add("Saving to: '/tmp/f'\n     5K ....... 10%\nresolving\n");
        assert!(!it.log_text.contains("5K"), "{}", it.log_text);
        assert!(it.log_text.contains("resolving"));
        // « Length: 1234 » détecté.
        it.log_text_add("\nLength: 1234 (1.2K) [text/html]\n");
        assert_eq!(it.total_bytesize, 1234);
        assert_eq!(it.size, "1KB");
    }

    #[test]
    fn timestr() {
        assert_eq!(secs2timestr(5), "5s");
        assert_eq!(secs2timestr(75), "1m15s");
        assert_eq!(secs2timestr(3700), "1h1m");
    }

    #[test]
    fn size_fmt() {
        assert_eq!(size_str(0), "0KB");
        assert_eq!(size_str(1536), "2KB");
        assert_eq!(size_str(5 * 1024 * 1024), "5.0MB");
    }
}

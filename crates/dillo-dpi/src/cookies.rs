//! Port of `dpi/cookies.c` — moteur du serveur cookies.dpi.
//!
//! Contient le store complet (verbatim amont) : domaines triés,
//! `all_cookies` partagé, LRU, caps `MAX_*`, parsing
//! `Set-Cookie` (attr `Path`/`Domain`/`Max-Age`/`Expires`/`Secure`),
//! règles `cookiesrc` (`DEFAULT DENY|ACCEPT|ACCEPT_SESSION` triées
//! domaine le plus long d'abord), lecture/écriture
//! `~/.dillo/cookies.txt` **byte-compatibles** et la boucle
//! `accept` séquentielle du démon (côté `bin/cookies.rs`).
//!
//! `mktime` amont = localtime : reproduit via `time` (`assume_local`
//! ≙ `mktime`, `OffsetDateTime` UTC ≙ `gmtime`).

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
use std::cell::RefCell;
use std::rc::Rc;

/// `MAX_DOMAIN_COOKIES`.
const MAX_DOMAIN_COOKIES: usize = 20;
/// `MAX_TOTAL_COOKIES`.
const MAX_TOTAL_COOKIES: usize = 1200;

/// `CookieControlAction`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControlAction {
    Accept,
    AcceptSession,
    Deny,
}

/// `CookieData_t` — un cookie.
#[derive(Debug)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    /// `expires_at` — unix time ; `-1` = « expiré/sans date ».
    pub expires_at: i64,
    pub host_only: bool,
    pub secure: bool,
    pub session_only: bool,
    pub last_used: i64,
}

impl Cookie {
    fn new() -> Self {
        Self {
            name: String::new(),
            value: String::new(),
            domain: String::new(),
            path: String::new(),
            expires_at: -1,
            host_only: false,
            secure: false,
            session_only: false,
            last_used: 0,
        }
    }
}

/// `DomainNode` — domaine trié + sa liste de cookies (références
/// partagées vers les mêmes objets que `all_cookies`, ≙ pointeurs
/// amont partagés entre les deux Dlist).
struct DomainNode {
    domain: String,
    cookies: Vec<Rc<RefCell<Cookie>>>,
}

/// État global verbatim (`all_cookies`, `domains`, `ccontrol`,
/// `default_action`, `cookies_use_counter`, `disabled`,
/// `cookies_epoch_time`, `cookies_future_time`).
pub struct CookieJar {
    all_cookies: Vec<Rc<RefCell<Cookie>>>,
    /// Trié par domaine (`dList_insert_sorted`, `dStrAsciiCasecmp`).
    domains: Vec<DomainNode>,
    /// `ccontrol` — règles, les plus longues d'abord.
    ccontrol: Vec<(String, ControlAction)>,
    default_action: ControlAction,
    cookies_use_counter: i64,
    pub disabled: bool,
    /// `cookies_epoch_time` ≙ `mktime(1970-01-01 local)`.
    epoch_time: i64,
    /// `cookies_future_time` ≙ `mktime(2038-01-19 03:14:07 local)`.
    future_time: i64,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// `mktime` ≙ `struct tm` local → unix. `None` ≙ `(time_t)-1`.
fn mktime(year: i32, mon: u32, day: u8, hour: u8, min: u8, sec: u8) -> Option<i64> {
    let d = time::Date::from_calendar_date(
        year,
        time::Month::try_from(u8::try_from(mon).ok()?).ok()?,
        day,
    )
    .ok()?;
    let t = time::Time::from_hms(hour, min, sec).ok()?;
    let pdt = time::PrimitiveDateTime::new(d, t);
    // ≙ mktime : interprète en heure locale.
    let off = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    Some(pdt.assume_offset(off).unix_timestamp())
}

/// `gmtime` ≙ unix → champs UTC (annee, mon 0-11, mday, h, m, s).
fn gmtime(t: i64) -> (i32, u32, u8, u8, u8, u8) {
    let odt =
        time::OffsetDateTime::from_unix_timestamp(t).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    (
        odt.year(),
        u32::from(u8::from(odt.month())),
        odt.day(),
        odt.hour(),
        odt.minute(),
        odt.second(),
    )
}

impl CookieJar {
    /// `Cookies_init` partiel : initialise les constantes epoch/future
    /// ; `disabled = TRUE` jusqu'à `init()`.
    #[must_use]
    pub fn new() -> Self {
        // epoch = 1er jan 1970 00:00 local ≙ cookies_epoch_tm + mktime.
        let epoch = mktime(1970, 1, 1, 0, 0, 0).unwrap_or(0);
        // future_tm = {7,14,3, 19,0,138} → 2038-01-19 03:14:07 local.
        let future = mktime(2038, 1, 19, 3, 14, 7).unwrap_or(i64::MAX - 1);
        Self {
            all_cookies: Vec::new(),
            domains: Vec::new(),
            ccontrol: Vec::new(),
            default_action: ControlAction::Deny,
            cookies_use_counter: 0,
            disabled: true,
            epoch_time: epoch,
            future_time: future,
        }
    }

    /// `Cookies_control_init` verbatim — parse `cookiesrc` (contenu
    /// passé par l'appelant : lecture fichier côté glue) ; retourne
    /// `(ok, enabled)` ≙ code amont (0 ok+enabled, 1 ok+disabled,
    /// 2 pas de fichier).
    pub fn control_init(&mut self, cookiesrc: &str) -> i32 {
        let mut enabled = false;
        for raw in cookiesrc.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut it = line.split_whitespace();
            let domain = it.next().unwrap_or("");
            let rule = it.next().unwrap_or("");
            let action = if rule.eq_ignore_ascii_case("ACCEPT") {
                ControlAction::Accept
            } else if rule.eq_ignore_ascii_case("ACCEPT_SESSION") {
                ControlAction::AcceptSession
            } else if rule.eq_ignore_ascii_case("DENY") {
                ControlAction::Deny
            } else {
                continue; // règle inconnue → MSG + continue verbatim
            };
            if domain.eq_ignore_ascii_case("DEFAULT") {
                self.default_action = action;
            } else {
                // insertion triée : les domaines les plus longs d'abord.
                let pos = self
                    .ccontrol
                    .iter()
                    .position(|(d, _)| domain.len() > d.len())
                    .unwrap_or(self.ccontrol.len());
                self.ccontrol.insert(pos, (domain.to_string(), action));
            }
            if action != ControlAction::Deny {
                enabled = true;
            }
        }
        i32::from(!enabled)
    }

    /// `Cookies_control_check_domain` verbatim : `.dom` suffixe ≙
    /// `domain+diff == rule` (diff>=0), sinon égalité stricte.
    #[must_use]
    pub fn control_check_domain(&self, domain: &str) -> ControlAction {
        for (rule, action) in &self.ccontrol {
            if let Some(suffix) = rule.strip_prefix('.') {
                let _ = suffix;
                let diff = domain.len() as isize - rule.len() as isize;
                if diff < 0 {
                    continue;
                }
                if !domain[diff as usize..].eq_ignore_ascii_case(rule) {
                    continue;
                }
            } else if !domain.eq_ignore_ascii_case(rule) {
                continue;
            }
            return *action;
        }
        self.default_action
    }

    /// `Domain_node_by_domain_cmp` — indice du nœud `domain` (trié).
    fn find_node(&self, domain: &str) -> Option<usize> {
        self.domains
            .binary_search_by(|n| {
                n.domain
                    .as_str()
                    .to_ascii_lowercase()
                    .cmp(&domain.to_ascii_lowercase())
            })
            .ok()
    }

    fn insert_node(&mut self, domain: &str) -> usize {
        let pos = self
            .domains
            .binary_search_by(|n| {
                n.domain
                    .as_str()
                    .to_ascii_lowercase()
                    .cmp(&domain.to_ascii_lowercase())
            })
            .unwrap_or_else(|p| p);
        self.domains.insert(
            pos,
            DomainNode {
                domain: domain.to_string(),
                cookies: Vec::new(),
            },
        );
        pos
    }

    /// `Cookies_delete_node`.
    fn delete_node(&mut self, idx: usize) {
        self.domains.remove(idx);
    }

    /// `Cookies_rm_expired_cookies(node)` — `node=None` ≙ all.
    fn rm_expired(&mut self, node: Option<usize>) -> usize {
        let now = now_unix();
        let mut removed = 0;
        let mut i = 0;
        // verbatim : itération sur la liste du nœud (ou all).
        loop {
            let cookie = match node {
                Some(ni) => self.domains.get(ni).and_then(|n| n.cookies.get(i)).cloned(),
                None => self.all_cookies.get(i).cloned(),
            };
            let Some(cookie) = cookie else { break };
            if cookie.borrow().expires_at - now < 0 {
                let ni = match node {
                    Some(ni) => Some(ni),
                    None => self.find_node(&cookie.borrow().domain),
                };
                if let Some(ni) = ni {
                    self.domains[ni].cookies.retain(|c| !Rc::ptr_eq(c, &cookie));
                    if self.domains[ni].cookies.is_empty() {
                        self.delete_node(ni);
                    }
                }
                self.all_cookies.retain(|c| !Rc::ptr_eq(c, &cookie));
                removed += 1;
            } else {
                i += 1;
            }
        }
        removed
    }

    /// `Cookies_get_LRU` + `Cookies_too_many`.
    fn too_many(&mut self, node: Option<usize>) {
        let lru = {
            let list: &[Rc<RefCell<Cookie>>] = match node {
                Some(ni) => &self.domains[ni].cookies,
                None => &self.all_cookies,
            };
            list.iter().min_by_key(|c| c.borrow().last_used).cloned()
        };
        let Some(lru) = lru else { return };
        let ni = match node {
            Some(ni) => Some(ni),
            None => self.find_node(&lru.borrow().domain),
        };
        if let Some(ni) = ni {
            self.domains[ni].cookies.retain(|c| !Rc::ptr_eq(c, &lru));
            if self.domains[ni].cookies.is_empty() {
                self.delete_node(ni);
            }
        }
        self.all_cookies.retain(|c| !Rc::ptr_eq(c, &lru));
    }

    /// `Cookies_cmp` verbatim : `name+path+host_only` identiques.
    fn same_cookie(a: &Cookie, b: &Cookie) -> bool {
        a.host_only == b.host_only && a.name == b.name && a.path == b.path
    }

    /// `Cookies_add_cookie` verbatim : remplacement par (name,path,
    /// `host_only`), rejet expiré, caps domaine/total avec LRU.
    pub fn add_cookie(&mut self, cookie: Cookie) {
        let node_idx = self.find_node(&cookie.domain);
        if let Some(ni) = node_idx {
            // retire les cookies (name,path,host_only) identiques.
            let dups: Vec<Rc<RefCell<Cookie>>> = self.domains[ni]
                .cookies
                .iter()
                .filter(|c| Self::same_cookie(&c.borrow(), &cookie))
                .cloned()
                .collect();
            for d in dups {
                self.domains[ni].cookies.retain(|c| !Rc::ptr_eq(c, &d));
                self.all_cookies.retain(|c| !Rc::ptr_eq(c, &d));
            }
        }
        if cookie.expires_at == -1 || cookie.expires_at - now_unix() <= 0 {
            return; // verbatim : cookie déjà expiré → ignoré.
        }
        let mut node_idx = node_idx;
        if let Some(ni) = node_idx {
            if self.domains[ni].cookies.len() >= MAX_DOMAIN_COOKIES {
                let removed = self.rm_expired(Some(ni));
                if removed == 0 {
                    self.too_many(Some(ni));
                } else if removed >= MAX_DOMAIN_COOKIES {
                    node_idx = self.find_node(&cookie.domain);
                }
            }
        }
        if self.all_cookies.len() >= MAX_TOTAL_COOKIES {
            if self.rm_expired(None) == 0 {
                self.too_many(None);
            }
            node_idx = self.find_node(&cookie.domain);
        }
        let rc = Rc::new(RefCell::new(cookie));
        rc.borrow_mut().last_used = self.cookies_use_counter;
        self.cookies_use_counter += 1;
        self.all_cookies.push(rc.clone());
        let dom = rc.borrow().domain.clone();
        if let Some(ni) = node_idx {
            self.domains[ni].cookies.push(rc);
        } else {
            let ni = self.insert_node(&dom);
            self.domains[ni].cookies.push(rc);
        }
        // verbatim `dList_length(domain_cookies) == 0` → delete
        // (après LRU/expirés le nœud peut être vide).
        if let Some(ni) = self.find_node(&dom) {
            if self.domains[ni].cookies.is_empty() {
                self.delete_node(ni);
            }
        }
    }

    /// `Cookies_set` verbatim — retourne 0 ok, -2 denied, -3 parse fail.
    pub fn set(
        &mut self,
        cookie_str: &str,
        url_host: &str,
        url_path: &str,
        server_date: Option<&str>,
    ) -> i32 {
        if self.disabled {
            return -1;
        }
        let action = self.control_check_domain(url_host);
        if action == ControlAction::Deny {
            return -2;
        }
        let Some(mut cookie) = parse(cookie_str, server_date, self.future_time) else {
            return -3;
        };
        if !validate_domain(&mut cookie, url_host) {
            return -3;
        }
        validate_path(&mut cookie, url_path);
        if action == ControlAction::AcceptSession {
            cookie.session_only = true;
        }
        self.add_cookie(cookie);
        0
    }

    /// `Cookies_match` verbatim.
    fn match_cookie(c: &Cookie, url_path: &str, host_only: bool, is_tls: bool) -> bool {
        if c.host_only != host_only {
            return false;
        }
        if c.secure && !is_tls {
            return false;
        }
        path_matches(url_path, &c.path)
    }

    /// `Cookies_add_matching_cookies` verbatim : purge expirés au
    /// passage, insert trié par path le plus long d'abord.
    fn add_matching(
        &mut self,
        domain: &str,
        url_path: &str,
        host_only: bool,
        out: &mut Vec<Rc<RefCell<Cookie>>>,
        is_tls: bool,
    ) {
        let Some(ni) = self.find_node(domain) else {
            return;
        };
        let now = now_unix();
        let mut i = 0;
        while i < self.domains[ni].cookies.len() {
            let c = self.domains[ni].cookies[i].clone();
            if c.borrow().expires_at - now < 0 {
                self.domains[ni].cookies.remove(i);
                self.all_cookies.retain(|x| !Rc::ptr_eq(x, &c));
                continue;
            }
            if Self::match_cookie(&c.borrow(), url_path, host_only, is_tls) {
                c.borrow_mut().last_used = self.cookies_use_counter;
                let plen = c.borrow().path.len();
                let pos = out
                    .iter()
                    .position(|x| x.borrow().path.len() < plen)
                    .unwrap_or(out.len());
                out.insert(pos, c);
            }
            i += 1;
        }
        if self.domains[ni].cookies.is_empty() {
            self.delete_node(ni);
        }
    }

    /// `Cookies_get` verbatim — chaîne « Cookie: a=b; c=d\r\n ».
    pub fn get(&mut self, url_host: &str, url_path: &str, url_scheme: &str) -> String {
        if self.disabled {
            return String::new();
        }
        let is_tls = url_scheme.eq_ignore_ascii_case("https");
        let is_ip = domain_is_ip(url_host);
        let mut matching: Vec<Rc<RefCell<Cookie>>> = Vec::new();
        let mut host_only = false;
        if !is_ip {
            // « .sub.example.com » — domaine avec point préfixé.
            let dstr = format!(".{url_host}");
            self.add_matching(&dstr, url_path, host_only, &mut matching, is_tls);
        }
        host_only = true;
        self.add_matching(url_host, url_path, host_only, &mut matching, is_tls);
        host_only = false;
        self.add_matching(url_host, url_path, host_only, &mut matching, is_tls);
        if !is_ip {
            // suffixes « .example.com » puis « example.com ».
            let host = url_host;
            let mut start = host[1..].find('.').map(|p| p + 1);
            while let Some(p) = start {
                if p >= host.len() {
                    break;
                }
                self.add_matching(&host[p..], url_path, host_only, &mut matching, is_tls);
                if p + 1 < host.len() {
                    self.add_matching(&host[p + 1..], url_path, host_only, &mut matching, is_tls);
                }
                start = host[p + 1..].find('.').map(|np| np + p + 1);
            }
        }
        if matching.is_empty() {
            return String::new();
        }
        let mut out = String::from("Cookie: ");
        for (i, c) in matching.iter().enumerate() {
            let c = c.borrow();
            out.push_str(&c.name);
            out.push('=');
            out.push_str(&c.value);
            out.push_str(if i + 1 < matching.len() { "; " } else { "\r\n" });
        }
        self.cookies_use_counter += 1;
        out
    }

    /// `Cookies_load_cookies` verbatim — parse du fichier
    /// `cookies.txt` (tab-separated, `F`=`host_only`, `T`=secure,
    /// expiry = secondes depuis `epoch_time` local).
    pub fn load(&mut self, contents: &str) {
        for raw in contents.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut pieces = line.splitn(7, '\t');
            let domain = pieces.next().unwrap_or("").to_string();
            let host_only = pieces.next().map(|p| p.starts_with('F'));
            let path = pieces.next().unwrap_or("").to_string();
            let secure = pieces.next().map(|p| p.starts_with('T'));
            let expires_at = pieces
                .next()
                .and_then(|p| p.parse::<i64>().ok())
                .map_or(-1, |secs| self.epoch_time + secs);
            let name = pieces.next().unwrap_or("").to_string();
            let value = pieces.next().unwrap_or("").to_string();
            // verbatim : domain non vide, path commence par '/',
            // name et value présents.
            if domain.is_empty()
                || host_only.is_none()
                || !path.starts_with('/')
                || secure.is_none()
                || name.is_empty()
            {
                continue; // « Malformed line » verbatim.
            }
            let action = self.control_check_domain(&domain);
            if action == ControlAction::Deny {
                continue;
            }
            let cookie = Cookie {
                name,
                value,
                domain,
                path,
                expires_at,
                host_only: host_only.unwrap_or(false),
                secure: secure.unwrap_or(false),
                session_only: action == ControlAction::AcceptSession,
                last_used: 0,
            };
            self.add_cookie(cookie);
        }
    }

    /// `Cookies_save_and_free` verbatim — écrit `cookies.txt`
    /// (en-tête + lignes tab) et vide le store.
    pub fn save(&mut self) -> String {
        let now = now_unix();
        let mut out = String::from(
            "# HTTP Cookie File\n# This is a generated file!  Do not edit.\n\
             # [domain  subdomains  path  secure  expiry_time  name  value]\n\n",
        );
        // verbatim : nœuds itérés dans l'ordre de la liste triée.
        for node in &self.domains {
            for c in &node.cookies {
                let c = c.borrow();
                if !c.session_only && c.expires_at - now > 0 {
                    let line = format!(
                        "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                        c.domain,
                        if c.host_only { "FALSE" } else { "TRUE" },
                        c.path,
                        if c.secure { "TRUE" } else { "FALSE" },
                        c.expires_at - self.epoch_time,
                        c.name,
                        c.value
                    );
                    if line.len() < 4096 {
                        out.push_str(&line);
                    }
                }
            }
        }
        self.domains.clear();
        self.all_cookies.clear();
        out
    }
}

impl Default for CookieJar {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------
// parsing Set-Cookie — verbatim `Cookies_parse*`/`Cookies_parse_date`.
// ---------------------------------------------------------------

/// `Cookies_parse_attr` verbatim : trim espaces, `strcspn(str,"=;")`,
/// rstrip.
fn parse_attr(s: &str) -> (&str, &str) {
    let s = s.trim_start_matches([' ', '\t']);
    let len = s.find(['=', ';']).unwrap_or(s.len());
    let (a, rest) = s.split_at(len);
    (a.trim_end_matches([' ', '\t']), rest)
}

/// `Cookies_parse_value` verbatim : `=` optionnel, jusqu'à `;`.
fn parse_value(s: &str) -> (&str, &str) {
    if let Some(rest) = s.strip_prefix('=') {
        let rest = rest.trim_start_matches([' ', '\t']);
        let len = rest.find(';').unwrap_or(rest.len());
        let (v, rest) = rest.split_at(len);
        (v.trim_end_matches([' ', '\t']), rest)
    } else {
        ("", s)
    }
}

/// `Cookies_eat_value` verbatim.
fn eat_value(s: &str) -> &str {
    if s.starts_with('=') {
        &s[s.find(';').unwrap_or(s.len())..]
    } else {
        s
    }
}

/// `Cookies_get_timefield` verbatim : 1-2 digits, -1 si 3e digit.
fn get_timefield(s: &[u8]) -> Option<(i32, usize)> {
    if s.is_empty() || !s[0].is_ascii_digit() {
        return None;
    }
    let mut n = i32::from(s[0] - b'0');
    let mut i = 1;
    if s.get(1).is_some_and(u8::is_ascii_digit) {
        n = n * 10 + i32::from(s[1] - b'0');
        i = 2;
        if s.get(2).is_some_and(u8::is_ascii_digit) {
            return None;
        }
    }
    Some((n, i))
}

/// `Cookies_get_month` verbatim.
fn get_month(s: &[u8]) -> Option<(u32, usize)> {
    const MONTHS: [&[u8]; 12] = [
        b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov",
        b"Dec",
    ];
    for (i, m) in MONTHS.iter().enumerate() {
        if s.len() >= 3 && s[..3].eq_ignore_ascii_case(m) {
            return Some((i as u32 + 1, 3));
        }
    }
    None
}

/// `Cookies_get_year` verbatim : 2 digits obligatoires, 3e et 4e
/// optionnels, 5e rejeté (« année 10000 »). Pivot 70 sur 2 digits.
fn get_year(s: &[u8]) -> Option<(i32, usize)> {
    if s.is_empty() || !s[0].is_ascii_digit() {
        return None;
    }
    let mut n = i32::from(s[0] - b'0');
    let mut i = 1;
    // 2e digit obligatoire.
    match s.get(i) {
        Some(c) if c.is_ascii_digit() => {
            n = n * 10 + i32::from(c - b'0');
            i += 1;
        }
        _ => return None,
    }
    // 3e et 4e digits optionnels.
    for _ in 0..2 {
        match s.get(i) {
            Some(c) if c.is_ascii_digit() => {
                n = n * 10 + i32::from(c - b'0');
                i += 1;
            }
            _ => break,
        }
    }
    // 5e digit → rejet verbatim.
    if s.get(i).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    if i == 2 {
        if (70..=99).contains(&n) {
            n += 1900;
        } else if n <= 69 {
            n += 2000;
        }
    }
    Some((n, i))
}

/// `Cookies_date_delim` verbatim.
fn date_delim(c: u8) -> bool {
    c == b'\t'
        || (0x20..=0x2f).contains(&c)
        || (0x3b..=0x40).contains(&c)
        || (0x5b..=0x60).contains(&c)
        || (0x7b..=0x7e).contains(&c)
}

/// `Cookies_parse_date` verbatim — champs time/day/month/year dans
/// n'importe quel ordre, séparés par `date_delim`.
/// Retourne `mktime`-ready `(y,mon,mday,h,min,s)`.
fn parse_date(date: &str) -> Option<(i32, u32, u8, u8, u8, u8)> {
    let s = date.as_bytes();
    let mut i = 0;
    let mut time_f = (0u8, 0u8, 0u8);
    let mut day = 0u8;
    let mut month = 0u32;
    let mut year = 0i32;
    let mut f_time = false;
    let mut f_day = false;
    let mut f_month = false;
    let mut f_year = false;
    while i < s.len() {
        let mut matched = false;
        if !f_time {
            // `Cookies_get_time` : h ':' m ':' s
            if let Some(((h, m, sec), len)) = get_time(&s[i..]) {
                time_f = (h as u8, m as u8, sec as u8);
                i += len;
                f_time = true;
                matched = true;
            }
        }
        if !matched && !f_day {
            if let Some((d, len)) = get_timefield(&s[i..]) {
                day = d as u8;
                i += len;
                f_day = true;
                matched = true;
            }
        }
        if !matched && !f_month {
            if let Some((m, len)) = get_month(&s[i..]) {
                month = m;
                i += len;
                f_month = true;
                matched = true;
            }
        }
        if !matched && !f_year {
            if let Some((y, len)) = get_year(&s[i..]) {
                year = y;
                i += len;
                f_year = true;
                matched = true;
            }
        }
        let _ = matched;
        while i < s.len() && !date_delim(s[i]) {
            i += 1;
        }
        while i < s.len() && date_delim(s[i]) {
            i += 1;
        }
    }
    if !(f_time && f_day && f_month && f_year) {
        return None;
    }
    // verbatim range-check.
    if !(day > 0
        && day < 32
        && (1..=12).contains(&month)
        && year >= 1900
        && time_f.0 < 24
        && time_f.1 < 60
        && time_f.2 < 60)
    {
        return None;
    }
    Some((year, month, day, time_f.0, time_f.1, time_f.2))
}

/// `Cookies_get_time` verbatim : « h:m:s ».
fn get_time(s: &[u8]) -> Option<((i32, i32, i32), usize)> {
    let (h, mut i) = get_timefield(s)?;
    if s.get(i) != Some(&b':') {
        return None;
    }
    i += 1;
    let (m, n2) = get_timefield(&s[i..])?;
    i += n2;
    if s.get(i) != Some(&b':') {
        return None;
    }
    i += 1;
    let (sec, n3) = get_timefield(&s[i..])?;
    i += n3;
    Some(((h, m, sec), i))
}

/// `Cookies_server_timediff` verbatim : `time(NULL) - mktime(server)`.
fn server_timediff(server_date: Option<&str>) -> i64 {
    server_date
        .and_then(parse_date)
        .and_then(|(y, mo, d, h, mi, s)| mktime(y, mo, d, h, mi, s))
        .map_or(0, |t| now_unix() - t)
}

/// `Cookies_unquote_string` verbatim.
fn unquote(s: &str) -> &str {
    if s.len() > 1 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// `Cookies_parse` verbatim — premier pair = name=value, puis attrs.
fn parse(cookie_str: &str, server_date: Option<&str>, future_time: i64) -> Option<Cookie> {
    let mut str_ = cookie_str;
    let mut cookie: Option<Cookie> = None;
    let mut first = true;
    let mut max_age = false;
    let mut expires = false;
    while !str_.is_empty() {
        let (attr, rest) = parse_attr(str_);
        str_ = rest;
        if first {
            if !str_.starts_with('=') || attr.is_empty() {
                return None; // nameless → disregard verbatim.
            }
            let mut c = Cookie::new();
            c.name = attr.to_string();
            let (v, r) = parse_value(str_);
            c.value = v.to_string();
            str_ = r;
            // verbatim : default = dans un an (gmtime + tm_year++ → mktime).
            let (y, mo, d, h, mi, s) = gmtime(now_unix());
            c.expires_at = mktime(y + 1, mo, d, h, mi, s).unwrap_or(future_time);
            cookie = Some(c);
            first = false;
        } else if attr.eq_ignore_ascii_case("Path") {
            let (v, r) = parse_value(str_);
            if let Some(c) = cookie.as_mut() {
                c.path = v.to_string();
            }
            str_ = r;
        } else if attr.eq_ignore_ascii_case("Domain") {
            let (v, r) = parse_value(str_);
            if let Some(c) = cookie.as_mut() {
                c.domain = v.to_string();
            }
            str_ = r;
        } else if attr.eq_ignore_ascii_case("Max-Age") {
            let (v, r) = parse_value(str_);
            str_ = r;
            if v.starts_with(|c: char| c.is_ascii_digit()) || v.starts_with('-') {
                let mut age = if v.starts_with('-') {
                    0i64
                } else {
                    v.parse::<i64>().unwrap_or(i64::MAX)
                };
                const ONE_YEAR: i64 = 31_536_000;
                if age > ONE_YEAR {
                    age = ONE_YEAR;
                }
                if age < 0 {
                    age = 0;
                }
                if let Some(c) = cookie.as_mut() {
                    c.expires_at = now_unix() + age;
                }
                expires = true;
                max_age = true;
            }
        } else if attr.eq_ignore_ascii_case("Expires") {
            if max_age {
                str_ = eat_value(str_);
            } else {
                let (v, r) = parse_value(str_);
                str_ = r;
                let v = unquote(v);
                if let Some((y, mo, d, h, mi, s)) = parse_date(v) {
                    let shift = server_timediff(server_date);
                    // verbatim `tm->tm_sec += timediff; mktime(tm)`.
                    if let Some(c) = cookie.as_mut() {
                        c.expires_at = mktime(y, mo, d, h, mi, s).map_or(-1, |t| t + shift);
                        if c.expires_at == -1 && y >= 2038 {
                            c.expires_at = future_time;
                        }
                    }
                } else if let Some(c) = cookie.as_mut() {
                    c.expires_at = -1;
                }
                expires = true;
            }
        } else if attr.eq_ignore_ascii_case("Secure") {
            if let Some(c) = cookie.as_mut() {
                c.secure = true;
            }
            str_ = eat_value(str_);
        } else if attr.eq_ignore_ascii_case("HttpOnly") {
            str_ = eat_value(str_);
        } else {
            str_ = eat_value(str_);
        }
        if let Some(r) = str_.strip_prefix(';') {
            str_ = r;
        }
    }
    if let Some(c) = cookie.as_mut() {
        c.session_only = !expires;
    }
    cookie
}

/// `Cookies_domain_is_ip` verbatim.
fn domain_is_ip(domain: &str) -> bool {
    if domain.is_empty() {
        return false;
    }
    if domain.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return true;
    }
    domain.contains(':')
        && domain
            .bytes()
            .all(|c| c.is_ascii_hexdigit() || c == b':' || c == b'.')
}

/// `Cookies_path_matches` verbatim.
fn path_matches(url_path: &str, cookie_path: &str) -> bool {
    let c_len = cookie_path.len();
    url_path.starts_with(cookie_path)
        && (c_len == url_path.len()
            || cookie_path.ends_with('/')
            || url_path.as_bytes().get(c_len) == Some(&b'/'))
}

/// `Cookies_validate_path` verbatim : path absent/non-`/` →
/// répertoire de l'URL (jusqu'au dernier `/` inclus).
fn validate_path(cookie: &mut Cookie, url_path: &str) {
    if !cookie.path.starts_with('/') {
        // verbatim `while (len && url_path[len] != '/') len--` puis
        // `dStrndup(url_path, len ? len : 1)` — index du dernier '/'
        // (copie 1 octet si aucun '/' — quirk amont verbatim).
        let len = url_path.rfind('/').unwrap_or(0);
        cookie.path = url_path[..if len > 0 { len } else { 1.min(url_path.len()) }].to_string();
        if cookie.path.is_empty() {
            cookie.path = "/".to_string();
        }
    }
}

/// `Cookies_internal_dots_required` verbatim — liste de TLD à 2
/// niveaux (`*.tld`).
fn internal_dots_required(host: &str) -> usize {
    let mut ret = 1;
    let after = host.len() - usize::from(host.ends_with('.'));
    let start = host[..after].rfind('.').map_or(0, |p| p + 1);
    let tld = &host[start..after];
    const TLDS: [&str; 23] = [
        "bd", "bn", "ck", "cy", "er", "fj", "fk", "gu", "il", "jm", "ke", "kh", "kw", "mm", "mz",
        "ni", "np", "pg", "ye", "za", "zm", "zw", "",
    ];
    if TLDS.iter().take(22).any(|t| t.eq_ignore_ascii_case(tld)) {
        ret += 1;
    }
    ret
}

/// `Cookies_domain_matches` verbatim : égalité, ou `B` suffixe de
/// `A` précédé d'un '.' (sauf IP).
fn domain_matches(a: &str, b_: &str) -> bool {
    if a.is_empty() || b_.is_empty() {
        return false;
    }
    let b = b_.strip_prefix('.').unwrap_or(b_);
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    if domain_is_ip(b) {
        return false;
    }
    if a.len() > b.len() {
        let diff = a.len() - b.len();
        return a[diff..].eq_ignore_ascii_case(b) && a.as_bytes()[diff - 1] == b'.';
    }
    false
}

/// `Cookies_validate_domain` verbatim : pas de Domain → `host_only` ;
/// sinon match + dots internes ≥ requis.
fn validate_domain(cookie: &mut Cookie, host: &str) -> bool {
    if cookie.domain.is_empty() {
        cookie.domain = host.to_string();
        cookie.host_only = true;
        return true;
    }
    if !domain_matches(host, &cookie.domain) {
        return false;
    }
    let internal = cookie.domain[1..cookie.domain.len().saturating_sub(1)]
        .bytes()
        .filter(|&c| c == b'.')
        .count();
    if internal < internal_dots_required(host) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jar() -> CookieJar {
        let mut j = CookieJar::new();
        j.control_init("DEFAULT ACCEPT\n.example.com DENY\n");
        j.disabled = false;
        j
    }

    #[test]
    fn control_rules() {
        let mut j = CookieJar::new();
        assert_eq!(j.control_init("DEFAULT DENY\n"), 1); // rien d'activé
        assert_eq!(
            j.control_init("DEFAULT ACCEPT\n.example.com DENY\nwww.example.com ACCEPT\n"),
            0
        );
        // longest-first : « www.example.com » avant « .example.com ».
        assert_eq!(
            j.control_check_domain("www.example.com"),
            ControlAction::Accept
        );
        assert_eq!(
            j.control_check_domain("other.example.com"),
            ControlAction::Deny
        );
        assert_eq!(
            j.control_check_domain("elsewhere.org"),
            ControlAction::Accept
        );
    }

    #[test]
    fn set_get() {
        let mut j = jar();
        // cookie posé depuis sub.example.com → règle .example.com=DENY
        assert_eq!(j.set("a=1", "x.example.com", "/", None), -2);
        // hôte sans règle → DEFAULT ACCEPT.
        assert_eq!(j.set("a=1; Path=/", "www.site.org", "/", None), 0);
        let hdr = j.get("www.site.org", "/", "http");
        assert_eq!(hdr, "Cookie: a=1\r\n");
        // secure cookie non-TLS → absent.
        j.set("s=2; Path=/; Secure", "www.site.org", "/", None);
        assert!(!j.get("www.site.org", "/", "http").contains("s=2"));
        assert!(j.get("www.site.org", "/", "https").contains("s=2"));
    }

    #[test]
    fn host_only_vs_domain() {
        let mut j = jar();
        j.set("h=1", "sub.site.org", "/", None); // host_only
        j.set("d=2; Domain=site.org", "sub.site.org", "/", None); // suffixe
                                                                  // requête depuis l'apex : seul le cookie Domain matche.
        let hdr = j.get("site.org", "/", "http");
        assert!(hdr.contains("d=2"), "{hdr}");
        assert!(!hdr.contains("h=1"), "{hdr}");
    }

    #[test]
    fn parse_dates() {
        // RFC 850 / asctime / RFC 1123 verbatim.
        assert!(parse_date("Wed, 21 Oct 2015 07:28:00 GMT").is_some());
        assert!(parse_date("Wednesday, 21-Oct-15 07:28:00 GMT").is_some());
        assert!(parse_date("Wed Oct 21 07:28:00 2015").is_some());
        assert!(parse_date("garbage").is_none());
    }

    #[test]
    fn max_age_wins_over_expires() {
        let mut j = jar();
        j.set(
            "a=1; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=3600",
            "www.site.org",
            "/",
            None,
        );
        assert!(j.get("www.site.org", "/", "http").contains("a=1"));
        // Expires passé sans Max-Age → rejeté.
        j.set(
            "b=2; Expires=Thu, 01 Jan 1970 00:00:00 GMT",
            "www.site.org",
            "/",
            None,
        );
        assert!(!j.get("www.site.org", "/", "http").contains("b=2"));
    }

    #[test]
    fn file_roundtrip() {
        let mut j = jar();
        // verbatim : les cookies sans Expires/Max-Age sont
        // session-only → non sauvés. Max-Age force la persistance.
        j.set("a=1; Path=/; Max-Age=3600", "www.site.org", "/", None);
        j.set(
            "b=x y; Path=/p; Domain=site.org; Max-Age=3600",
            "sub.site.org",
            "/",
            None,
        );
        let saved = j.save();
        assert!(saved.contains("www.site.org\tFALSE\t/\tFALSE\t"));
        assert!(saved.contains("site.org\tTRUE\t/p\tFALSE\t"));
        let mut j2 = jar();
        j2.load(&saved);
        assert_eq!(j2.get("www.site.org", "/", "http"), "Cookie: a=1\r\n");
        assert!(j2.get("site.org", "/p", "http").contains("b=x y"));
    }

    #[test]
    fn path_rules() {
        // verbatim RFC6265 path-match.
        assert!(path_matches("/p", "/p"));
        assert!(path_matches("/p/x", "/p"));
        assert!(!path_matches("/px", "/p"));
        assert!(path_matches("/anything", "/"));
    }

    #[test]
    fn domain_dots() {
        assert_eq!(internal_dots_required("site.org"), 1);
        assert_eq!(internal_dots_required("x.ck"), 2);
        // TLD « *.ck » : un cookie Domain=.ck depuis x.ck → rejeté
        // (0 dot interne < 2 requis).
        let mut c = Cookie {
            domain: ".ck".into(),
            ..Cookie::new()
        };
        assert!(!validate_domain(&mut c, "x.ck"));
        let mut c2 = Cookie {
            domain: ".x.ck".into(),
            ..Cookie::new()
        };
        // « .x.ck » a 1 dot interne < 2 → rejeté aussi (verbatim).
        assert!(!validate_domain(&mut c2, "x.ck"));
        let mut ok = Cookie {
            domain: ".a.b.site.org".into(),
            ..Cookie::new()
        };
        assert!(validate_domain(&mut ok, "a.b.site.org"));
    }
}

//! `dillo` — binaire principal du navigateur.
//!
//! Port de `src/dillo.cc` : parsing CLI (table `Options` verbatim),
//! init des prefs/paths/modules dans l'ordre amont, création de la
//! première `BrowserWindow`, ouverture des URL de la ligne de
//! commande, boucle d'événements ([`DwApp::run`] ≙ `while
//! (Fl::wait() > 0)`), puis nettoyage (`*_freeall`).
//!
//! Déviations documentées (cf. `docs/PORTING.md`) :
//! - `xembed`/`-x XID` est X11-spécifique → option rejetée
//!   (`won't port`, ADR-0002) ;
//! - les signaux `SIGUSR1/2`/`SIGCHLD` n'ont pas d'équivalent sûr en
//!   std-Rust : le rechargement passe par le socket de contrôle
//!   (`dilloc`) et `dpid` est réapé par le runtime (phase dpi) ;
//! - `Fl::scheme(theme)`, `fl_message_title_default` et les label
//!   types FLTK sont absorbés par le chrome `dillo-browser` (le
//!   rendu n'est pas celui d'un toolkit natif).

#![allow(clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use dillo_browser::glue::Glue;
use dillo_browser::paths;
use dillo_browser::prefs::Prefs;
use dillo_common::misc;
use dillo_net::cache;
use dillo_net::capi;
#[cfg(unix)]
use dillo_net::control::unix;
use dillo_net::cookies;
use dillo_net::dns::Dns;
use dillo_net::domain;
use dillo_net::dpi;
use dillo_net::history;
use dillo_net::hsts;
use dillo_net::http::{HttpConfig, HttpEngine};
use dillo_net::io::IoEngine;
use dillo_net::tls::TlsEngine;
use dillo_net::url::{self, DilloUrl};
use dillo_net::{auth, timeout};
use dillo_platform::app::DwApp;

/// `OptID` (verbatim `dillo.cc`).
const DILLO_CLI_XID: u32 = 1 << 0;
const DILLO_CLI_FULLWINDOW: u32 = 1 << 1;
const DILLO_CLI_HELP: u32 = 1 << 2;
const DILLO_CLI_VERSION: u32 = 1 << 3;
const DILLO_CLI_LOCAL: u32 = 1 << 4;
const DILLO_CLI_GEOMETRY: u32 = 1 << 5;

/// `CLI_options` (verbatim).
struct CliOption {
    shortopt: &'static str,
    longopt: &'static str,
    /// Positif : argument obligatoire ; négatif : optionnel.
    opt_argc: i32,
    id: u32,
    help: &'static str,
}

/// Table `Options[]` verbatim.
const OPTIONS: &[CliOption] = &[
    CliOption {
        shortopt: "-f",
        longopt: "--fullwindow",
        opt_argc: 0,
        id: DILLO_CLI_FULLWINDOW,
        help: "  -f, --fullwindow       Start in full window mode: hide address bar,\n  \
               \x20                        navigation buttons, menu, and status bar.",
    },
    CliOption {
        shortopt: "-g",
        longopt: "-geometry",
        opt_argc: 1,
        id: DILLO_CLI_GEOMETRY,
        help: "  -g, -geometry GEO      Set initial window position where GEO is\n  \
               \x20                        WxH[{+-}X{+-}Y]",
    },
    CliOption {
        shortopt: "-h",
        longopt: "--help",
        opt_argc: 0,
        id: DILLO_CLI_HELP,
        help: "  -h, --help             Display this help text and exit.",
    },
    CliOption {
        shortopt: "-l",
        longopt: "--local",
        opt_argc: 0,
        id: DILLO_CLI_LOCAL,
        help: "  -l, --local            Don't load images or stylesheets, or follow\n  \
               \x20                        redirections, for these FILEs or URLs.",
    },
    CliOption {
        shortopt: "-v",
        longopt: "--version",
        opt_argc: 0,
        id: DILLO_CLI_VERSION,
        help: "  -v, --version          Display version info and exit.",
    },
    CliOption {
        shortopt: "-x",
        longopt: "--xid",
        opt_argc: 1,
        id: DILLO_CLI_XID,
        help: "  -x, --xid XID          Open first Dillo window in an existing\n  \
               \x20                        window whose window ID is XID.",
    },
];

/// `printHelp` verbatim.
fn print_help(cmdname: &str) {
    eprintln!("Usage: {cmdname} [OPTION]... [URL|FILE]...\nOptions:");
    for o in OPTIONS {
        eprintln!("{}", o.help);
    }
}

/// `getCmdOption` verbatim — retourne l'`OptID` et avance `idx`.
/// `opt_args` reçoit la valeur d'option (verbatim `opt_argv`).
fn get_cmd_option<'a>(args: &'a [String], idx: &mut usize, opt_args: &mut Vec<&'a str>) -> u32 {
    if *idx >= args.len() {
        return 0;
    }
    let arg = &args[*idx];
    for o in OPTIONS {
        if arg == o.shortopt || arg == o.longopt {
            *idx += 1;
            let need = o.opt_argc.unsigned_abs() as usize;
            for _ in 0..need {
                if *idx >= args.len() {
                    eprintln!("Not enough parameters for option \"{arg}\".");
                    return 1 << 15; // DILLO_CLI_ERROR
                }
                opt_args.push(&args[*idx]);
                *idx += 1;
            }
            return o.id;
        }
    }
    0 // pas une option — fin du scan (verbatim : non-reconnu ⇒ arrêt)
}

/// `a_Version_print_info` ≙ version.cc (`dillo --version`).
fn print_version() {
    println!("Dillo {}", env!("CARGO_PKG_VERSION"));
    println!("  (port Rust — stack winit/softbuffer/tiny-skia/cosmic-text, rustls)");
}

/// `makeStartUrl` verbatim — fichier local (`F_OK`) ou URL.
fn make_start_url(arg: &str, local: bool) -> Option<DilloUrl> {
    // Chemin relatif → préfixe du cwd de lancement
    // (`Paths::getOldWorkingDir`).
    let p: Vec<u8> = if arg.starts_with('/') {
        arg.as_bytes().to_vec()
    } else {
        let mut v = paths::old_working_dir().unwrap_or_default();
        if !v.ends_with(b"/") {
            v.push(b'/');
        }
        v.extend_from_slice(arg.as_bytes());
        v
    };

    let mut start_url = if std::fs::metadata(String::from_utf8_lossy(&p).as_ref()).is_ok() {
        // F_OK ≙ access() — le chemin absolu peut contenir des
        // caractères non-URL.
        let url_str = misc::escape_chars(&p, b"% #");
        url::url_new(&url_str[1..], Some(b"file:/"))
    } else {
        // Pas un fichier → filtrage URL.
        let url_str = url::string_strip_delimiters(arg.as_bytes());
        url::url_new(&url_str, None)
    }?;

    if local {
        let f = start_url.flags | url::flags::SPAM_SAFE;
        start_url.set_flags(f);
    }
    Some(start_url)
}

/// Lit un fichier de prefs (retourne `None` si absent ≙ `NULL` amont).
fn prefs_content(rc_file: &str) -> Option<String> {
    let mut r = paths::prefs_fp(rc_file)?;
    let mut s = String::new();
    std::io::Read::read_to_string(&mut r, &mut s).ok()?;
    Some(s)
}

fn main() -> std::process::ExitCode {
    tracing_subscriber_init();
    let args: Vec<String> = std::env::args().collect();

    // --- CLI (verbatim) -------------------------------------------------
    let mut options_got: u32 = 0;
    let mut idx = 1usize;
    let mut xpos = 0;
    let mut ypos = 0;
    let mut width = 0;
    let mut height = 0;

    loop {
        let mut opt_args: Vec<&str> = Vec::new();
        let opt_id = get_cmd_option(&args, &mut idx, &mut opt_args);
        if opt_id == 0 {
            break;
        }
        options_got |= opt_id;
        match opt_id {
            DILLO_CLI_FULLWINDOW | DILLO_CLI_LOCAL => {}
            DILLO_CLI_XID => {
                // xembed non porté (X11) — l'option est reconnue puis
                // refusée proprement plutôt qu'ignorée en silence.
                eprintln!("dillo: -x/--xid (xembed) is not supported by this port");
                return std::process::ExitCode::from(2);
            }
            DILLO_CLI_GEOMETRY => {
                let Some((x, y, w, h)) = misc::parse_geometry(opt_args[0].as_bytes()) else {
                    eprintln!(
                        "geometry argument \"{}\" not valid. Must be of the form WxH[{{+-}}X{{+-}}Y].",
                        opt_args[0]
                    );
                    return std::process::ExitCode::from(2);
                };
                width = w;
                height = h;
                xpos = x.unwrap_or(-1);
                ypos = y.unwrap_or(-1);
            }
            DILLO_CLI_VERSION => {
                print_version();
                return std::process::ExitCode::from(0);
            }
            DILLO_CLI_HELP => {
                print_help(&args[0]);
                return std::process::ExitCode::from(0);
            }
            _ => {
                print_help(&args[0]);
                return std::process::ExitCode::from(2);
            }
        }
    }

    // --- a_Prefs_init + fichiers de conf (ordre verbatim) ---------------
    paths::init();

    let mut prefs = Prefs::default();
    if let Some(mut fp) = paths::prefs_fp(paths::RC_PREFS) {
        let _ = prefs.parse_dillorc(&mut fp);
    }

    // `a_Control_init` côté transport — créé avant Glue (le listener
    // est injecté dessous ; hors Unix ≙ `ENABLE_CONTROL_SOCKET` off).
    #[cfg(unix)]
    let ctl_listener = unix::CtlListener::init(&paths::dillo_dir()).ok();

    // Moteurs réseau : leurs `a_*_init` amont sont globaux ; ici ils
    // partagent un runtime tokio worker (ADR-0003) et s'installent
    // dans la `Chain` de chaque connexion via la factory capi.
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("dillo: cannot start io runtime: {e}");
            return std::process::ExitCode::from(1);
        }
    };
    let handle = rt.handle().clone();

    // Partage `prefs` Chrome↔glue (≙ global `prefs` amont).
    let prefs_rc = Rc::new(RefCell::new(prefs));
    let mut app = DwApp::new_empty();
    let mut glue = Glue::new(app.platform(), Rc::clone(&prefs_rc));

    // keysrc / domainrc (verbatim — après dillorc).
    if let Some(content) = prefs_content(paths::RC_KEYS) {
        glue.chrome.keys.parse(&content);
    }
    if let Some(content) = prefs_content(paths::RC_DOMAIN) {
        domain::domain_parse(&content);
    }
    // `dLib_show_messages(prefs.show_msg)` ≙ filtre tracing.
    if !prefs_rc.borrow().show_msg {
        tracing::info!("dillo: show_msg off");
    }

    // --- initialisation des modules (ordre verbatim de `dillo.cc`) ------
    // a_Dpi_init (vide amont) + client dpi ≙ `a_Dpi_ccc`.
    dpi::init();
    capi::capi_set_dpi_ccc(dpi::dpi_ccb());
    // Canal cookies → serveur dpi "cookies" (`a_Dpi_send_blocking_cmd`).
    cache::cache_set_dpi_send(Box::new(|cmd| dpi::send_blocking_cmd("cookies", cmd)));
    // a_Dns_init + a_IO_init + a_Http_init + a_Tls_init → factory.
    capi::capi_init();
    // a_Mime_init + a_Dicache_init des viewers/factory : Glue::new.
    dillo_images::dicache::dicache_init(prefs_rc.borrow().ignore_image_formats.as_deref());
    // a_Bw_init → registre dans Glue::new.
    // a_Cookies_init — contrôle `~/.dillo/cookiesrc` (verbatim : pas
    // de fallback sysconfdir). `Cookies_fopen` : si absent, création
    // `O_WRONLY|O_CREAT|O_TRUNC` 0600 + contenu "DEFAULT DENY\n".
    let cookiesrc_path = paths::dillo_dir().join("cookiesrc");
    let cookiesrc = std::fs::read_to_string(&cookiesrc_path).ok().or_else(|| {
        use std::io::Write;
        let mut opt = std::fs::OpenOptions::new();
        // `O_WRONLY | O_CREAT | O_TRUNC` verbatim.
        opt.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opt.mode(0o600);
        }
        opt.open(&cookiesrc_path)
            .and_then(|mut f| f.write_all(b"DEFAULT DENY\n"))
            .ok()?;
        Some("DEFAULT DENY\n".to_string())
    });
    let _ = cookies::cookies_init(cookiesrc.as_deref());
    // a_Actions_init → Chrome::new (`actions_init`).
    let hsts_preload = prefs_content(paths::HSTS_PRELOAD);
    hsts::hsts_init(
        prefs_rc.borrow().http_strict_transport_security,
        hsts_preload.as_deref(),
    );
    auth::auth_init();
    // a_UIcmd_init → UiCmd::new (Glue::new).
    // a_Control_init → Control::new (Glue::new) + listener injecté.
    #[cfg(unix)]
    if let Some(l) = ctl_listener {
        glue.set_ctl_listener(l);
    }

    // StyleEngine::init — feuille UA statique.
    {
        let mut ti = dillo_html::html::HtmlTagIndex;
        dillo_css::styleengine::StyleEngine::init(&mut ti);
    }

    // Keys::genAboutKeys — page `about:keys` injectée dans le cache.
    glue.chrome.keys.gen_about_keys();

    // Réglages dw (verbatim — statiques globaux amont).
    {
        let p = prefs_rc.borrow();
        dillo_dw::widget::set_adjust_min_width(p.adjust_min_width);
        dillo_dw::table::set_adjust_table_min_width(p.adjust_table_min_width);
        dillo_dw::textblock::set_penalty_hyphen(p.penalty_hyphen);
        dillo_dw::textblock::set_penalty_hyphen2(p.penalty_hyphen_2);
        dillo_dw::textblock::set_penalty_em_dash_left(p.penalty_em_dash_left);
        dillo_dw::textblock::set_penalty_em_dash_right(p.penalty_em_dash_right);
        dillo_dw::textblock::set_penalty_em_dash_right2(p.penalty_em_dash_right_2);
        dillo_dw::textblock::set_stretchability_factor(p.stretchability_factor);
    }

    // La ligne de commande prime sur dillorc (verbatim).
    {
        let mut p = prefs_rc.borrow_mut();
        if options_got & DILLO_CLI_FULLWINDOW != 0 {
            p.fullwindow_start = true;
        }
        if options_got & DILLO_CLI_GEOMETRY != 0 {
            p.width = width;
            p.height = height;
            p.xpos = xpos;
            p.ypos = ypos;
        }
    }

    // --- moteurs I/O/HTTP/DNS/TLS partagés (≙ a_Http_init…) -------------
    // Le mot de passe proxy est saisi plus bas (dialog) puis injecté
    // dans chaque `HttpEngine` via cette cellule partagée.
    let proxy_passwd: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    {
        let p = prefs_rc.borrow();
        let cfg = HttpConfig {
            http_language: p
                .http_language
                .as_deref()
                .map(|s| String::from_utf8_lossy(s).into_owned()),
            http_proxy: p.http_proxy.clone(),
            http_proxyuser: p
                .http_proxyuser
                .as_deref()
                .map(|s| String::from_utf8_lossy(s).into_owned()),
            no_proxy: p
                .no_proxy
                .as_deref()
                .map(|s| String::from_utf8_lossy(s).into_owned()),
            http_max_conns: p.http_max_conns,
            http_persistent_conns: p.http_persistent_conns,
            http_user_agent: p
                .http_user_agent
                .as_deref()
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .unwrap_or_default(),
            http_referer: p.http_referer.as_deref().map_or_else(
                || "host".to_string(),
                |s| String::from_utf8_lossy(s).into_owned(),
            ),
            trace_http: p.trace_http,
            brotli: true, // ≙ ENABLE_BROTLI amont
        };
        let rt2 = handle.clone();
        let passwd = Rc::clone(&proxy_passwd);
        let dialogs = Rc::clone(&glue.dialogs);
        capi::capi_set_engine_factory(Rc::new(move |chain| {
            chain.set_module(IoEngine::new(rt2.clone()));
            chain.set_module(Dns::new(rt2.clone()));
            let mut http = HttpEngine::new(rt2.clone(), cfg.clone());
            if let Some(pw) = passwd.borrow().as_ref() {
                http.set_proxy_passwd(pw);
            }
            // `a_Cookies_get_query` — Cookie header via le dpi "cookies".
            http.set_cookies_fn(Rc::new(|url, requester, is_root| {
                let mut send = |cmd: &str| dpi::send_blocking_cmd("cookies", cmd);
                cookies::cookies_get_query(url, requester, is_root, &mut send)
            }));
            chain.set_module(http);
            // `Tls_fd_dialog` ≙ `a_Dialog_choice(title,msg,"Continue",
            // "Cancel")` — via le runner partagé (≙ `a_Dialog_*` amont).
            let dl = Rc::clone(&dialogs);
            let dialog: dillo_net::tls::CertDialog = Rc::new(move |title, msg| {
                let spec = dillo_browser::dialog::choice_spec(title, msg, &["Continue", "Cancel"]);
                match spec {
                    Some(spec) => match dl.try_borrow_mut() {
                        Ok(mut d) => match d.run(&spec) {
                            dillo_browser::dialog::DialogAnswer::Choice(i) => i,
                            _ => 2, // échec fermé ≙ Cancel amont
                        },
                        Err(_) => 2,
                    },
                    None => 2,
                }
            });
            if let Ok(tls) = TlsEngine::with_dialog(rt2.clone(), dialog) {
                chain.set_module(tls);
            }
        }));
    }

    // --- première BrowserWindow (verbatim) -------------------------------
    let p = prefs_rc.borrow();
    let bw = glue
        .uicmd
        .browser_window_new(&glue.reg, &mut glue.chrome, &p, 0, 0, None);
    // ≙ `((Fl_Widget*)bw->ui)->window()->make_current()` — la fenêtre
    // native est créée au premier tick (`drain_pending`).

    // --- auth proxy (verbatim, après la fenêtre) -------------------------
    if let Some(user) = p.http_proxyuser.as_deref() {
        if proxy_passwd.borrow().is_none() {
            // ≙ `!a_Http_proxy_auth()` — jamais renseigné à ce stade.
            let user_s = String::from_utf8_lossy(user).into_owned();
            if let Some(pw) = glue.uicmd.get_passwd(&mut glue.chrome, &user_s) {
                *proxy_passwd.borrow_mut() = Some(pw);
            } else {
                tracing::warn!("Not using proxy authentication.");
            }
        }
    }

    // --- URL/fichiers de la ligne de commande (verbatim) -----------------
    let local = options_got & DILLO_CLI_LOCAL != 0;
    if idx >= args.len() {
        // Pas d'URL → page de démarrage (`about:blank` → focus location).
        let start = p.start_page.clone();
        let blank = start.as_ref().is_some_and(|u| {
            u.scheme.as_deref() == Some(b"about") && u.path.as_deref() == Some(b"blank")
        });
        if blank || start.is_none() {
            glue.uicmd
                .open_url(&glue.reg, &mut glue.chrome, &p, bw, None);
        } else if let Some(mut u) = start {
            glue.uicmd
                .open_url(&glue.reg, &mut glue.chrome, &p, bw, Some(&u));
            let txt = u.str_().to_vec();
            glue.uicmd
                .set_location_text(&glue.reg, &mut glue.chrome, bw, &txt);
        }
    } else {
        for (i, a) in args.iter().enumerate().skip(idx) {
            let Some(start_url) = make_start_url(a, local) else {
                continue;
            };
            if i > idx {
                if p.middle_click_opens_new_tab {
                    glue.uicmd.open_url_nt(
                        &glue.reg,
                        &mut glue.chrome,
                        &p,
                        bw,
                        Some(&start_url),
                        1,
                    );
                } else {
                    glue.uicmd
                        .open_url_nw(&glue.reg, &mut glue.chrome, &p, bw, Some(&start_url));
                }
            } else {
                glue.uicmd
                    .open_url(&glue.reg, &mut glue.chrome, &p, bw, Some(&start_url));
                let mut u = start_url;
                let txt = u.str_().to_vec();
                glue.uicmd
                    .set_location_text(&glue.reg, &mut glue.chrome, bw, &txt);
            }
        }
    }
    drop(p);

    // --- boucle `while (Fl::wait() > 0)` --------------------------------
    app.set_shell(Rc::new(RefCell::new(glue)));
    let ret = app.run();

    // --- nettoyage (verbatim ordre de `dillo.cc`) ------------------------
    // a_Control_free → `Drop` du listener (unlink du socket).
    drop(rt); // arrête les worker tokio (dns/io/tls/http moteurs)
    domain::domain_freeall();
    cookies::cookies_freeall();
    hsts::hsts_freeall();
    cache::cache_freeall();
    dillo_images::dicache::dicache_freeall();
    // a_Http/a_Tls/a_Dns_freeall → drop des moteurs par-connexion.
    history::history_freeall();
    timeout::timeout_freeall();
    // a_Prefs_freeall / Keys::free → drop de `prefs`/`chrome`.
    paths::free();
    // dw::core::freeall / dw::fltk::freeall → drop des layouts/vues.
    if let Err(e) = ret {
        eprintln!("dillo: event loop error: {e}");
        return std::process::ExitCode::from(1);
    }
    println!("Dillo: normal exit!");
    std::process::ExitCode::from(0)
}

/// `dLib_show_messages`/MSG → filtre `tracing` (AGENTS §4.10).
fn tracing_subscriber_init() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr) // MSG/_MSG/DEBUG_MSG amont → stderr
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .try_init();
}

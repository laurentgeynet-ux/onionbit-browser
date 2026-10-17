//! Port of `dpid/main.c` — démon superviseur des plugins dpi.
//!
//! Boucle verbatim :
//! - `register_all` (plugins `.dpi` de `~/.dillo/dpi` + `dpi_dir`
//!   système) puis `fill_services_list` (liens `service=path`).
//! - `init_ids_srs_socket` : socket de supervision `127.0.0.1`,
//!   port publié avec la clé dans `~/.dillo/dpid_comm_keys`.
//! - `init_all_dpi_sockets` : une socket d'écoute par plugin.
//! - boucle `select` ≙ tâches tokio `accept` par socket + signaux ;
//!   un accept sur `srs` = requête de service (`auth/check_server`/
//!   `DpiBye/register_all`), sur un socket plugin = `fork`/`execl`.
//! - idle `dpid_idle_timeout` (3600 s) sans activité → `DpiBye` aux
//!   serveurs actifs puis exit (verbatim — sauf si `downloads` tourne).

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
use std::net::TcpStream;
use std::process::exit;
use std::time::{Duration, Instant};

use dillo_dpi::dpid::{self, DpidCmd};
use dillo_dpi::dpid_common;
use dillo_dpi::dpip::Dsh;
use tracing::warn;

/// `dpid_idle_timeout` — verbatim 60*60 s (TODO amont : lire dpidrc).
const IDLE_TIMEOUT: Duration = Duration::from_secs(60 * 60);

fn main() {
    // verbatim : umask(0077) ≙ les fichiers écrits (comm_keys, sockdir)
    // sont en 0600/0700 explicitement côté port.
    run();
}

fn run() {
    let mut dpis = dpid::register_all();
    if dpis.is_empty() {
        eprintln!("dpid: can't find the directory for dpis.");
        // verbatim : register_all exit(1) quand aucun dir trouvé ;
        // ici on continue quand même si l'un des dirs existe mais vide
        // — comportement amont : il a déjà exit. On suit amont.
        exit(1);
    }
    let mut services = dpid::fill_services_list(&dpis);

    // `init_ids_srs_socket` + `SharedKey = a_Misc_mksecret(8)`.
    let Some((srs, srs_port)) = dpid::bind_socket_fd(dpid::DPID_BASE_PORT) else {
        eprintln!("dpid refuses to start, possibly because:");
        eprintln!("\t1) An instance of dpid is already running.");
        eprintln!("\t2) A previous dpid didn't clean up on exit.");
        exit(1);
    };
    let shared_key = dpid_common::mksecret(8);
    if dpid::save_comm_keys(srs_port, &shared_key).is_err() {
        eprintln!("dpid: can't save comm keys");
        exit(1);
    }

    // `init_all_dpi_sockets` — une socket par plugin.
    for d in &mut dpis {
        if let Some((l, port)) = dpid::bind_socket_fd(dpid::DPID_BASE_PORT) {
            d.port = port;
            d.sock = Some(l);
        } else {
            eprintln!("dpid: can't bind socket for {}", d.id);
            exit(1);
        }
    }
    let mut watched: Vec<bool> = dpis.iter().map(|_| true).collect();

    // Signaux ≙ `est_dpi_terminator` — un flag atomique par signal ;
    // la boucle vérifie à chaque itération (≙ handler `terminator`
    // amont qui cleanup+_exit — différé de ≤50 ms ici).
    #[cfg(unix)]
    let term = {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let t = Arc::new(AtomicBool::new(false));
        for sig in [
            signal_hook::consts::SIGHUP,
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGQUIT,
            signal_hook::consts::SIGTERM,
        ] {
            let _ = signal_hook::flag::register(sig, Arc::clone(&t));
        }
        Some(t)
    };

    println!("dpid started (found {} dpis)", dpis.len());

    let mut last_activity = Instant::now();
    loop {
        // ≙ `handle_sigchld` quand caught_sigchld : polling try_wait
        // (le port n'a pas de vrai handler de signal — même effet
        // observable, cadence ~50 ms au lieu d'immédiat).
        let mut activity = false;
        activity |= dpid::handle_sigchld(&mut dpis, &mut watched) > 0;

        // termination demandée ?
        #[cfg(unix)]
        if let Some(t) = &term {
            if t.load(std::sync::atomic::Ordering::Relaxed) {
                dpid::stop_active_dpis(&dpis, &shared_key);
                dpid::cleanup();
                exit(0);
            }
        }

        // srs : requête de service
        match srs.accept() {
            Ok((mut sock, _)) => {
                activity = true;
                service_request(
                    &mut sock,
                    &mut dpis,
                    &mut services,
                    &mut watched,
                    &shared_key,
                );
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => warn!("accept on srs socket failed: {}", e),
        }

        // sockets plugin : une connexion = lancer le plugin
        for (i, d) in dpis.iter_mut().enumerate() {
            if !watched.get(i).copied().unwrap_or(false) {
                continue;
            }
            let Some(l) = d.sock.as_ref() else {
                continue;
            };
            match l.accept() {
                Ok((stream, _)) => {
                    activity = true;
                    if d.filter {
                        // `start_filter_plugin` : socket acceptée → stdio
                        let _ = dpid::spawn_plugin(&d.path, dpid::PluginStdio::Stream(stream));
                        // verbatim : la socket reste surveillée.
                    } else {
                        // `start_server_plugin` : la socket d'écoute
                        // part dans le fils ; plus surveillée.
                        watched[i] = false;
                        if let Some(l) = d.sock.take() {
                            d.child = dpid::spawn_plugin(&d.path, dpid::PluginStdio::Listener(l));
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => warn!("accept on dpi socket {}: {}", d.id, e),
            }
        }

        if activity {
            last_activity = Instant::now();
        } else {
            // verbatim : select timeout → bye si pas de downloads actif
            if last_activity.elapsed() >= IDLE_TIMEOUT {
                if dpid::server_is_running(&dpis, "downloads") {
                    last_activity = Instant::now();
                    std::thread::sleep(Duration::from_millis(50));
                    continue;
                }
                dpid::stop_active_dpis(&dpis, &shared_key);
                dpid::cleanup();
                exit(0);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Traite une connexion `srs` — `get_request`/`get_command` + switch
/// verbatim (`read_next` pour `auth`).
fn service_request(
    sock: &mut TcpStream,
    dpis: &mut Vec<dpid::DpAttr>,
    services: &mut Vec<dpid::Service>,
    watched: &mut Vec<bool>,
    shared_key: &str,
) {
    let Some(cloned) = sock.try_clone().ok() else {
        return;
    };
    let Some(mut sh) = Dsh::tcp(cloned) else {
        return;
    };

    loop {
        let req = sh.read_token(true);
        let req_s = req
            .as_deref()
            .map(|b| String::from_utf8_lossy(b).into_owned());
        match dpid::get_command(req_s.as_deref(), &mut sh) {
            Ok(DpidCmd::Auth) => {
                if dillo_dpi::dpip::check_auth(req_s.as_deref().unwrap_or("")) {
                    continue; // ≙ `goto read_next`
                }
            }
            Ok(DpidCmd::Bye) => {
                dpid::stop_active_dpis(dpis, shared_key);
                dpid::cleanup();
                exit(0);
            }
            Ok(DpidCmd::CheckServer) => {
                if let Some(tag) = req_s.as_deref() {
                    dpid::send_sockport(sock, tag, dpis, services);
                }
            }
            Ok(DpidCmd::RegisterAll) => {
                // `register_all_cmd` verbatim.
                dpid::stop_active_dpis(dpis, shared_key);
                let mut new_dpis = dpid::register_all();
                *services = dpid::fill_services_list(&new_dpis);
                for d in &mut new_dpis {
                    if let Some((l, port)) = dpid::bind_socket_fd(dpid::DPID_BASE_PORT) {
                        d.port = port;
                        d.sock = Some(l);
                    }
                }
                *watched = new_dpis.iter().map(|_| true).collect();
                *dpis = new_dpis;
            }
            Ok(DpidCmd::Unknown) => {
                if let Some(d) =
                    dillo_net::dpip::build_cmd("cmd=%s msg=%s", &["DpiError", "Unknown command"])
                {
                    let _ = dpid_common::ckd_write(sock, d.as_bytes());
                }
                warn!("Unknown command for request: {:?}", req_s);
            }
            Ok(DpidCmd::RegisterService) | Err(()) => {
                // verbatim : pas de case → rien, la socket se ferme.
            }
        }
        break;
    }
    sh.close();
}

//! Port of `dpid/dpidc.c` — programme de contrôle du démon dpid.
//!
//! Se connecte au port de supervision (`127.0.0.1:port` lu dans
//! `~/.dillo/dpid_comm_keys`), envoie `auth` puis la commande :
//! `stop` (`DpiBye`), `register` (`register_all`) ou `chat`
//! (message libre depuis stdin — verbatim, sans framing supplémentaire).

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
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::exit;

use dillo_dpi::dpid_common::{ckd_write, read_comm_keys_pub};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!(
            "Control program for the Dillo plugin daemon\nUsage: {} {{stop|register|chat}}\n",
            args.first().map_or("dpidc", String::as_str)
        );
        exit(1);
    }

    // `Dpi_read_comm_keys` verbatim.
    let Some((port, key)) = read_comm_keys_pub() else {
        eprintln!("** ERROR **: main: Can't read dpid's port number");
        exit(1);
    };

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], u16::try_from(port).unwrap_or(0)));
    let mut sock = match TcpStream::connect(addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("** ERROR **: ERROR connecting: {e}");
            exit(1);
        }
    };

    // verbatim : auth envoyé en premier, en clair (pas via build_cmd).
    let auth = format!("<cmd='auth' msg='{}' '>", String::from_utf8_lossy(&key));
    if !ckd_write(&mut sock, auth.as_bytes()) {
        eprintln!("** ERROR **: ERROR writing to socket");
        exit(1);
    }

    let buffer: Vec<u8> = match args[1].as_str() {
        "stop" => b"<cmd='DpiBye' '>".to_vec(),
        "register" => b"<cmd='register_all' '>".to_vec(),
        "chat" => {
            print!("Please enter the message: ");
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            line.into_bytes()
        }
        other => {
            eprintln!("** ERROR **: main: Unknown operation '{other}'");
            eprintln!(
                "Control program for the Dillo plugin daemon\nUsage: {} {{stop|register|chat}}\n",
                args.first().map_or("dpidc", String::as_str)
            );
            exit(1);
        }
    };
    if !ckd_write(&mut sock, &buffer) {
        eprintln!("** ERROR **: ERROR writing to socket");
        exit(1);
    }
    // verbatim : la réponse n'est pas lue (code commenté amont).
    let mut buf = [0u8; 1];
    let _ = sock.read(&mut buf); // laisse le socket se fermer proprement
}

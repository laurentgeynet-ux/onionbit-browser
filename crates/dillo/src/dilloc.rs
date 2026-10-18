//! `dilloc` — client du socket de contrôle Dillo
//! (`src/dilloc.c` verbatim).
//!
//! Protocole : connecte `~/.dillo/ctl/<pid>` (`AF_UNIX`), envoie la
//! commande + `\n`, forward stdin pour `load`/`rawload`, puis lit la
//! réponse dont le 1er octet est le code de retour suivi de `\n`.
//!
//! POSIX-only comme amont (`AF_UNIX`) — hors Unix : message d'erreur
//! + exit 2 (≙ build sans `ENABLE_CONTROL_SOCKET`).

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(u8::try_from(dilloc::run()).unwrap_or(1))
}

#[cfg(not(unix))]
fn main() -> std::process::ExitCode {
    eprintln!("dilloc: control socket not supported on this platform");
    std::process::ExitCode::from(2)
}

#[cfg(unix)]
mod dilloc {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    /// `is_number` verbatim (tous chiffres).
    fn is_number(s: &str) -> bool {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
    }

    fn home() -> String {
        String::from_utf8_lossy(&dillo_common::dlib::get_home_dir()).into_owned()
    }

    fn ctl_dir() -> PathBuf {
        PathBuf::from(home()).join(".dillo").join("ctl")
    }

    /// `connect_given_pid` verbatim — retry toutes les 50 ms,
    /// warn à 1 s, timeout 10 s.
    fn connect_given_pid(pid: &str) -> Option<UnixStream> {
        let path = ctl_dir().join(pid);
        let t0 = Instant::now();
        let max = Duration::from_secs(10);
        let warn = Duration::from_secs(1);
        let mut warned = false;
        loop {
            match UnixStream::connect(&path) {
                Ok(s) => return Some(s),
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    eprintln!("connect to {} failed: {}", path.display(), e);
                    return None;
                }
                Err(_) => {}
            }
            if !warned && t0.elapsed() >= warn {
                eprintln!("connect to {} taking long", path.display());
                warned = true;
            }
            if t0.elapsed() >= max {
                eprintln!("timeout connecting to {}", path.display());
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// `find_working_socket` verbatim — scanne `~/.dillo/ctl/*`,
    /// garde le pid unique connectable, retire les `ECONNREFUSED`.
    fn find_working_socket() -> Option<UnixStream> {
        let dir = ctl_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("error: cannot open {} directory: {}", dir.display(), e);
                eprintln!("hint: is dillo running?");
                return None;
            }
        };
        let mut found = 0;
        let mut sock = None;
        for ep in entries.flatten() {
            let name = ep.file_name();
            let Some(num) = name.to_str() else { continue };
            if !is_number(num) {
                continue;
            }
            let path = dir.join(num);
            match UnixStream::connect(&path) {
                Ok(s) => {
                    found += 1;
                    if found == 1 {
                        sock = Some(s); // verbatim : gardé ouvert
                    }
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::ConnectionRefused {
                        // verbatim : pid mort → nettoie le socket.
                        let _ = std::fs::remove_file(&path);
                    } else {
                        eprintln!("cannot connect to {}, skipping: {}", path.display(), e);
                    }
                }
            }
        }
        match found {
            1 => sock,
            0 => {
                eprintln!("error: cannot find ctl socket, is dillo running?");
                None
            }
            _ => {
                eprintln!("multiple ctl sockets found, set DILLO_PID");
                None
            }
        }
    }

    /// `connect_to_dillo` verbatim — `DILLO_PID` sinon scan.
    fn connect() -> Option<UnixStream> {
        match std::env::var("DILLO_PID") {
            Ok(pid) if !pid.is_empty() => connect_given_pid(&pid),
            _ => find_working_socket(),
        }
    }

    /// `main` verbatim — code de retour = 1er octet de la réponse.
    pub fn run() -> i32 {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if args.is_empty() {
            eprintln!("missing command, try 'dilloc help'");
            return 2;
        }
        let Some(mut fd) = connect() else { return 2 };

        let is_load = args[0] == "load" || args[0] == "rawload";
        // verbatim : argv joints par ' ' + '\n'.
        let mut cmd = args.join(" ");
        cmd.push('\n');
        if fd.write_all(cmd.as_bytes()).is_err() {
            eprintln!("write command to socket failed");
            return 2;
        }

        if is_load {
            // verbatim : forward stdin → socket.
            let mut buf = [0u8; 8192];
            loop {
                match std::io::stdin().read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if fd.write_all(&buf[..n]).is_err() {
                            eprintln!("write stdin to socket failed");
                            return 2;
                        }
                    }
                    Err(e) => {
                        eprintln!("read stdin failed: {e}");
                        return 2;
                    }
                }
            }
            return 0; // verbatim : dClose(fd) implicite
        }

        // verbatim : 2 premiers octets = rc + '\n', puis forward
        // socket → stdout.
        let mut saw_rc = false;
        let mut rc = 2;
        let mut buf = [0u8; 8192];
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        loop {
            match fd.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let mut p: &[u8] = &buf[..n];
                    if !saw_rc {
                        if n < 2 {
                            eprintln!("read reply failed");
                            return 2;
                        }
                        let ch = buf[0];
                        if !ch.is_ascii_digit() {
                            eprintln!(
                                "malformed reply: expected digit at first byte (got {})",
                                ch as char
                            );
                            return 2;
                        }
                        if buf[1] != b'\n' {
                            eprintln!("malformed reply: expected newline at second byte");
                            return 2;
                        }
                        rc = i32::from(ch - b'0');
                        p = &buf[2..n];
                        saw_rc = true;
                    }
                    if out.write_all(p).is_err() {
                        eprintln!("cannot write reply to stdout");
                        return 2;
                    }
                }
                Err(e) => {
                    eprintln!("read reply failed: {e}");
                    return 2;
                }
            }
        }
        rc
    }
}

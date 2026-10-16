//! Port of `src/IO/tls.c` + la couche « dispatch » de `tls_mbedtls.c`/
//! `tls_openssl.c` — backends remplacés par **rustls** (ADR-0003).
//!
//! Mapping :
//! - `a_Tls_init` → [`TlsEngine::new`] / [`TlsEngine::with_dialog`]
//!   (store CA = natif via `rustls-native-certs` ≙
//!   `Tls_load_certificates` ; `~/.dillo/certs`/`tls_ca_cert_file`
//!   ≙ [`TlsEngine::add_ca_file`]).
//! - `a_Tls_connection(fd)` → le variant [`crate::io::IoStream::Tls`]
//!   (le lookup fd→conn devient ownership/`Rc` partagé).
//! - `a_Tls_connect_ready` → [`TlsEngine::connect_ready`], machine à
//!   états `CERT_STATUS_*` verbatim.
//! - `a_Tls_connect` → [`TlsEngine::connect`] : wrap `IoStream` →
//!   `Tls` (`Rc<TlsConn>` ≙ le fd/conn partagé lecteur+écrivain),
//!   handshake conduit en tâche tokio (≙ `Tls_handshake_cb` armé
//!   par iowatch), événement [`TlsEvent`] vers le thread principal.
//! - `MBEDTLS_SSL_VERIFY_OPTIONAL` → `DilloVerifier` : webpki
//!   vérifie mais *enregistre* les warnings au lieu d'échouer ;
//!   décision post-handshake dans [`TlsEngine::after_handshake`]
//!   = `Tls_check_certificate` (`a_Dialog_choice` injecté).
//! - `a_Tls_close_by_fd` → `TlsConn::shutdown_both` (`close_notify`).
//! - `tls_mbedtls.c` : sémantique cert portée ici (ci-dessus) ;
//!   `tls_openssl.c` → won't port (backend alternatif amont).

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme};
use tokio::io::Interest;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::debug;

use crate::io::IoStream;
use crate::url::DilloUrl;

/// `TLS_CONNECT_*` (tls.h).
pub const TLS_CONNECT_NEVER: i32 = -1;
pub const TLS_CONNECT_NOT_YET: i32 = 0;
pub const TLS_CONNECT_READY: i32 = 1;

// `CERT_STATUS_*` (tls_mbedtls.c) — pub : `cert_status`/`connect_ready`
// les exposent (http.c teste `RECEIVING`/`USER_ACCEPTED`).
/// `CERT_STATUS_NONE` — aucun certificat reçu pour ce serveur.
pub const CERT_STATUS_NONE: i32 = 0;
/// `CERT_STATUS_RECEIVING` — handshake en cours / premier cert reçu.
pub const CERT_STATUS_RECEIVING: i32 = 1;
/// `CERT_STATUS_CLEAN` — certificat vérifié sans warning.
pub const CERT_STATUS_CLEAN: i32 = 2;
/// `CERT_STATUS_BAD` — certificat refusé (warnings non acceptés).
pub const CERT_STATUS_BAD: i32 = 3;
/// `CERT_STATUS_USER_ACCEPTED` — warnings acceptés par l'utilisateur.
pub const CERT_STATUS_USER_ACCEPTED: i32 = 4;

/// `Server_t` amont (clé = hostname insensible casse + port).
#[derive(Debug, Default)]
struct TlsServer {
    cert_status: i32,
}

/// Warning de certificat ≙ bits de `mbedtls_ssl_get_verify_result`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertWarning {
    /// Certificat expiré.
    Expired,
    /// Nom de sujet ≠ host (`VERIFY_BAD_CN`).
    NameMismatch,
    /// Chaîne non ancrée dans les CA de confiance.
    Untrusted,
    /// Certificat pas encore valide.
    NotYetValid,
    /// Autre échec de vérification.
    Other,
}

/// `a_Dialog_choice` — injecté par la couche UI ; retour `1` =
/// « Continue », autre = Cancel/fermeture. Défaut : Cancel (échec
/// fermé ≙ `CERT_STATUS_BAD`).
pub type CertDialog = Rc<dyn Fn(&str, &str) -> i32>;

/// Vérificateur « optional » : délègue à webpki, consigne le warning
/// par host (consommé au `after_handshake` par le serveur (host,port)
/// concerné) et **accepte toujours** — `MBEDTLS_SSL_VERIFY_OPTIONAL`.
#[derive(Debug)]
struct DilloVerifier {
    inner: Arc<dyn ServerCertVerifier>,
    warnings: Mutex<HashMap<String, Vec<CertWarning>>>,
}

impl DilloVerifier {
    fn new(inner: Arc<dyn ServerCertVerifier>) -> Self {
        Self {
            inner,
            warnings: Mutex::new(HashMap::new()),
        }
    }

    /// Warnings enregistrés pour `host` (vidés à la lecture).
    fn take_warnings(&self, host: &str) -> Vec<CertWarning> {
        self.warnings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(host)
            .unwrap_or_default()
    }

    fn classify(err: &rustls::Error) -> CertWarning {
        use rustls::{CertificateError, Error};
        match err {
            Error::InvalidCertificate(e) => match e {
                CertificateError::Expired => CertWarning::Expired,
                CertificateError::NotValidForName
                | CertificateError::NotValidForNameContext { .. } => CertWarning::NameMismatch,
                CertificateError::NotValidYet => CertWarning::NotYetValid,
                CertificateError::UnknownIssuer => CertWarning::Untrusted,
                _ => CertWarning::Other,
            },
            _ => CertWarning::Other,
        }
    }
}

impl ServerCertVerifier for DilloVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let host = match server_name {
            ServerName::DnsName(d) => d.as_ref().to_string(),
            _ => String::new(),
        };
        match self.inner.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            Ok(v) => Ok(v),
            Err(e) => {
                // VERIFY_OPTIONAL : on enregistre et on laisse passer —
                // l'utilisateur tranchera au dialog post-handshake.
                self.warnings
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .entry(host)
                    .or_default()
                    .push(Self::classify(&e));
                Ok(ServerCertVerified::assertion())
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// `Conn_t` amont — partagé par `Arc` entre les IO lecteur/écrivain
/// (≙ même `io->FD`/`conn` côté amont). `conn` sous `Mutex` (ops
/// synchrones uniquement, jamais à travers un `.await`) ; `sock`
/// hors mutex : `ready()` n'a pas besoin du `conn`.
pub struct TlsConn {
    /// Machine à états TLS + buffers plaintext/ciphertext.
    pub conn: Mutex<ClientConnection>,
    /// Socket sous-jacente (readiness + raw io).
    pub sock: TcpStream,
}

impl std::fmt::Debug for TlsConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsConn").finish_non_exhaustive()
    }
}

/// Shim `std::io::Read` sur le `try_read` non-bloquant de tokio
/// (≙ `read(2)` avec `EAGAIN`).
struct SockRead<'a>(&'a TcpStream);
impl Read for SockRead<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.try_read(buf)
    }
}
/// Shim `std::io::Write` (≙ `write(2)` non-bloquant).
struct SockWrite<'a>(&'a TcpStream);
impl Write for SockWrite<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.try_write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl TlsConn {
    /// `a_Tls_read` : plaintext bufferisé d'abord, puis réseau
    /// (`read_tls` + `process_new_packets`) — `WANT_READ` ≙ `WouldBlock`,
    /// `Ok(0)` ≙ `close_notify` EOF.
    ///
    /// # Errors
    /// Erreur socket ou erreur TLS fatale (alerte protocolaire).
    pub fn try_read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let mut conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for _ in 0..2 {
            match conn.reader().read(buf) {
                Ok(n) => return Ok(n),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    match conn.read_tls(&mut SockRead(&self.sock)) {
                        Err(e) => return Err(e), // WouldBlock inclus
                        Ok(_) => {
                            conn.process_new_packets().map_err(io::Error::other)?;
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::from(io::ErrorKind::WouldBlock))
    }

    /// `a_Tls_write` : plaintext → `write_tls` flush (best-effort —
    /// les records restants ressortiront au write suivant).
    ///
    /// # Errors
    /// Erreur d'écriture plaintext (`WouldBlock` si le record buffer
    /// rustls est plein ≙ `WANT_WRITE` amont).
    pub fn try_write(&self, buf: &[u8]) -> io::Result<usize> {
        let mut conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let n = conn.writer().write(buf)?;
        let _ = conn.write_tls(&mut SockWrite(&self.sock));
        Ok(n)
    }

    /// `mbedtls_ssl_close_notify` seul — le fd reste ouvert
    /// (≙ `Tls_close_by_key` hors `connecting` : le `dClose` est
    /// laissé au teardown IO amont).
    pub fn close_notify(&self) {
        let mut conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.send_close_notify();
        let _ = conn.write_tls(&mut SockWrite(&self.sock));
    }

    /// `mbedtls_ssl_close_notify` + shutdown socket.
    ///
    /// # Errors
    /// Erreur du shutdown socket sous-jacent.
    pub fn shutdown_both(&self) -> io::Result<()> {
        {
            let mut conn = self
                .conn
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            conn.send_close_notify();
            let _ = conn.write_tls(&mut SockWrite(&self.sock));
        }
        socket2::SockRef::from(&self.sock).shutdown(std::net::Shutdown::Both)
    }
}

/// Événement handshake → thread principal (≙ `Tls_handshake_cb`
/// appelant `a_Http_connect_done`).
#[derive(Debug)]
pub enum TlsEvent {
    /// Handshake TLS terminé — la décision cert reste à prendre
    /// (`Tls_check_certificate` ≙ [`TlsEngine::after_handshake`]).
    Handshook {
        sock_id: i32,
        /// ≙ `conn->ssl` a reçu un certificat pair (vérif. possible).
        has_cert: bool,
        /// ≙ log `version, cipher` amont (`MBEDTLS_SSL_HANDSHAKE_OVER`
        /// + `CERT_STATUS_RECEIVING` → affichage du cipher négocié).
        cipher: Option<String>,
    },
    /// Échec IO/protocole avant complétion.
    Failed { sock_id: i32 },
}

/// `Conn_t` côté engine — `conn_list` + `Tls_fd_map` amont : clé
/// « fd » (`sock_id`), statut `connecting` et clé serveur pour
/// `reset_server_state` au `Tls_close_by_key`.
struct TlsConnEntry {
    /// `c->url` réduit à la clé `Server_t` (host-ci, port).
    key: (String, u16),
    /// Tâche handshake en cours (abort ≙ `a_IOwatch_remove_fd`).
    abort: Option<tokio::task::AbortHandle>,
    /// `conn->connecting` — handshake pas terminé.
    connecting: bool,
}

/// `TlsEngine` — `servers` + `conn_list` amont + config rustls.
/// Module `Chain` ; tout se passe sur le thread principal.
pub struct TlsEngine {
    config: Arc<ClientConfig>,
    verifier: Arc<DilloVerifier>,
    /// `servers` amont (find/insert « sorted » → Vec triée à l'insert).
    servers: Vec<((String, u16), TlsServer)>,
    /// `conn_list` amont : `sock_id` → entrée connexion.
    connections: HashMap<i32, TlsConnEntry>,
    /// `a_Dialog_choice` injecté.
    dialog: CertDialog,
    rt: tokio::runtime::Handle,
    event_tx: mpsc::UnboundedSender<TlsEvent>,
    event_rx: mpsc::UnboundedReceiver<TlsEvent>,
}

impl std::fmt::Debug for TlsEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsEngine")
            .field("servers", &self.servers.len())
            .finish_non_exhaustive()
    }
}

/// Clé `Server_t` = (host-ci, port) — `Tls_servers_by_url_cmp`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// `as u16` ≙ troncature `htons(uint16)` amont — fidèle.
fn server_key(url: &mut DilloUrl) -> (String, u16) {
    let host = url
        .hostname()
        .map(|h| String::from_utf8_lossy(h).to_ascii_lowercase())
        .unwrap_or_default();
    let port = url.port() as u16;
    (host, port)
}

fn build_config(verifier: &Arc<DilloVerifier>) -> Arc<ClientConfig> {
    Arc::new(
        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(verifier.clone())
            .with_no_client_auth(),
    )
}

fn make_verifier(roots: rustls::RootCertStore) -> io::Result<Arc<DilloVerifier>> {
    let inner = rustls::client::WebPkiServerVerifier::builder(Arc::new(roots))
        .build()
        .map_err(io::Error::other)?;
    Ok(Arc::new(DilloVerifier::new(inner)))
}

fn native_roots() -> rustls::RootCertStore {
    let mut roots = rustls::RootCertStore::empty();
    let mut loaded = 0usize;
    for cert in rustls_native_certs::load_native_certs().certs {
        if roots.add(cert).is_ok() {
            loaded += 1;
        }
    }
    debug!(loaded, "Tls_load_certificates: native roots");
    roots
}

impl TlsEngine {
    /// `a_Tls_init` — store CA natif + verifier « optional », dialog
    /// par défaut = Cancel (échec fermé).
    ///
    /// # Errors
    /// `Err` si le vérificateur webpki ne peut être construit.
    pub fn new(rt: tokio::runtime::Handle) -> io::Result<Self> {
        Self::with_dialog(rt, Rc::new(|_, _| 2))
    }

    /// `a_Tls_init` avec `a_Dialog_choice` injecté (UI/tests).
    ///
    /// # Errors
    /// Cf. [`TlsEngine::new`].
    pub fn with_dialog(rt: tokio::runtime::Handle, dialog: CertDialog) -> io::Result<Self> {
        let verifier = make_verifier(native_roots())?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Ok(Self {
            config: build_config(&verifier),
            verifier,
            servers: Vec::new(),
            connections: HashMap::new(),
            dialog,
            rt,
            event_tx,
            event_rx,
        })
    }

    /// `Tls_load_certificates_from_file` — ajoute des CA (PEM ou DER)
    /// et reconstruit le verifier (≙ re-parse au init amont).
    ///
    /// # Errors
    /// `Err` si le fichier est illisible ou sans certificat analysable.
    pub fn add_ca_file(&mut self, path: &std::path::Path) -> io::Result<usize> {
        let data = std::fs::read(path)?;
        let certs = pem_or_der_parse(&data);
        if certs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no certificates in file",
            ));
        }
        let mut roots = native_roots();
        let n = certs.len();
        for c in certs {
            let _ = roots.add(c);
        }
        self.verifier = make_verifier(roots)?;
        self.config = build_config(&self.verifier);
        Ok(n)
    }

    /// `a_Tls_connect_ready` — verbatim : 1re connexion autorisée pour
    /// obtenir le cert (RECEIVING), les suivantes attendent le verdict.
    pub fn connect_ready(&mut self, url: &mut DilloUrl) -> i32 {
        let key = server_key(url);
        if let Some(pos) = self.servers.iter().position(|(k, _)| *k == key) {
            let s = self.servers[pos].1.cert_status;
            let ret = if s == CERT_STATUS_RECEIVING {
                TLS_CONNECT_NOT_YET
            } else if s == CERT_STATUS_BAD {
                TLS_CONNECT_NEVER
            } else {
                TLS_CONNECT_READY
            };
            if s == CERT_STATUS_NONE {
                self.servers[pos].1.cert_status = CERT_STATUS_RECEIVING;
            }
            ret
        } else {
            self.servers.push((
                key,
                TlsServer {
                    cert_status: CERT_STATUS_RECEIVING,
                },
            ));
            TLS_CONNECT_READY
        }
    }

    /// `Tls_cert_status` (état courant pour UI/diags).
    #[must_use]
    pub fn cert_status(&self, url: &mut DilloUrl) -> i32 {
        let key = server_key(url);
        self.servers
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(CERT_STATUS_NONE, |(_, s)| s.cert_status)
    }

    /// `a_Tls_certificate_is_clean` ≙ `cert_status == CERT_STATUS_CLEAN`.
    #[must_use]
    pub fn certificate_is_clean(&self, url: &mut DilloUrl) -> bool {
        self.cert_status(url) == CERT_STATUS_CLEAN
    }

    /// `a_Tls_reset_server_state` — RECEIVING → NONE (retry quand la
    /// connexion est morte avant réception du cert).
    pub fn reset_server_state(&mut self, url: &mut DilloUrl) {
        let key = server_key(url);
        if let Some((_, s)) = self.servers.iter_mut().find(|(k, _)| *k == key) {
            if s.cert_status == CERT_STATUS_RECEIVING {
                s.cert_status = CERT_STATUS_NONE;
            }
        }
    }

    /// `Tls_user_said_no` — cert `BAD` → refus immédiat
    /// (≙ `success = FALSE` → `reset_server_state` + `connect_done`).
    #[must_use]
    pub fn user_said_no(&mut self, url: &mut DilloUrl) -> bool {
        self.cert_status(url) == CERT_STATUS_BAD
    }

    /// `a_Tls_connect` — crée la `ClientConnection`, wrap le stream en
    /// `IoStream::Tls` (`Arc` ≙ fd/conn partagé) et enregistre l'entrée
    /// `conn_list`. `Err` ≙ `success = FALSE` amont (`Tls_user_said_no`,
    /// `mbedtls_ssl_setup`/`set_hostname` échoués) — l'appelant fait
    /// alors `reset_server_state` + `connect_done(fd, FALSE)`.
    /// Le handshake démarre à [`TlsEngine::begin_handshake`] (permis à
    /// l'appelant de swapper le stream dans la table I/O avant tout
    /// trafic — ≙ ordre `Tls_conn_new` → premier `mbedtls_ssl_handshake`).
    ///
    /// # Errors
    /// `Err` si `stream` n'est pas `Plain`, si le hostname n'est pas
    /// un `ServerName` valide, si le cert serveur est `BAD`, ou si la
    /// connexion rustls échoue.
    pub fn connect(
        &mut self,
        sock_id: i32,
        stream: IoStream,
        url: &mut DilloUrl,
    ) -> io::Result<IoStream> {
        let IoStream::Plain(sock) = stream else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Tls connect expects a plain stream",
            ));
        };
        if self.user_said_no(url) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "certificate previously rejected by user",
            ));
        }
        let host = url
            .hostname()
            .map(|h| String::from_utf8_lossy(h).into_owned())
            .unwrap_or_default();
        let name = ServerName::try_from(host)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let conn = ClientConnection::new(self.config.clone(), name).map_err(io::Error::other)?;
        let tls = Arc::new(TlsConn {
            conn: Mutex::new(conn),
            sock,
        });
        self.connections.insert(
            sock_id,
            TlsConnEntry {
                key: server_key(url),
                abort: None,
                connecting: true,
            },
        );
        Ok(IoStream::Tls(tls))
    }

    /// Arme la tâche de handshake (`Tls_handshake` amont armé par
    /// `a_IOwatch_add_fd(fd, want, Tls_handshake_cb)`).
    pub fn begin_handshake(&mut self, sock_id: i32, tls: &Arc<TlsConn>) {
        let h = self
            .rt
            .spawn(handshake_task(sock_id, tls.clone(), self.event_tx.clone()));
        if let Some(e) = self.connections.get_mut(&sock_id) {
            e.abort = Some(h.abort_handle());
        }
    }

    /// `conn->connecting = FALSE` — appelé sur [`TlsEvent::Handshook`]
    /// ou [`TlsEvent::Failed`] avant la décision `connect_done`.
    pub fn mark_established(&mut self, sock_id: i32) {
        if let Some(e) = self.connections.get_mut(&sock_id) {
            e.connecting = false;
            e.abort = None;
        }
    }

    /// `Tls_close_by_key` — `reset_server_state` + (`connecting` →
    /// abort handshake) + retrait de l'entrée. Retourne
    /// `Some(connecting)` si une connexion existait : l'appelant
    /// applique alors `IoEngine::remove_socket` (`dClose` amont) si
    /// `true`, `IoEngine::close_notify` sinon (le fd vit dans
    /// `IoEngine` — hors `connecting` le `dClose` reste au teardown IO).
    pub fn close_by_fd(&mut self, sock_id: i32) -> Option<bool> {
        let e = self.connections.remove(&sock_id)?;
        self.reset_server_state_by_key(&e.key);
        if e.connecting {
            if let Some(h) = e.abort {
                h.abort();
            }
        }
        Some(e.connecting)
    }

    /// `a_Tls_reset_server_state` par clé serveur (usage `close_by_fd`).
    fn reset_server_state_by_key(&mut self, key: &(String, u16)) {
        if let Some((_, s)) = self.servers.iter_mut().find(|(k, _)| k == key) {
            if s.cert_status == CERT_STATUS_RECEIVING {
                s.cert_status = CERT_STATUS_NONE;
            }
        }
    }

    /// `Tls_check_certificate` — sur [`TlsEvent::Handshook`] : lit les
    /// warnings du verifier pour ce host, applique le dialog amont,
    /// met à jour `cert_status`. `has_cert` ≙ `peer_certificates()`.
    /// Retourne `true` ≙ `ret == 0` (connexion acceptée).
    pub fn after_handshake(&mut self, url: &mut DilloUrl, has_cert: bool) -> bool {
        let title = format!(
            "Dillo TLS security warning: {}",
            url.hostname()
                .map(|h| String::from_utf8_lossy(h).into_owned())
                .unwrap_or_default()
        );
        let host = url
            .hostname()
            .map(|h| String::from_utf8_lossy(h).into_owned())
            .unwrap_or_default();
        let warnings = self.verifier.take_warnings(&host);
        let mut choice = -1i32;
        let ret = if !has_cert {
            choice = (self.dialog)(
                &title,
                "No certificate received from this site. Can't verify who it is.",
            );
            choice == 1
        } else if warnings.is_empty() {
            true // st == 0 amont
        } else {
            let msg = bad_cert_msg(&warnings);
            choice = (self.dialog)(&title, &msg);
            choice == 1
        };

        let key = server_key(url);
        if let Some((_, s)) = self.servers.iter_mut().find(|(k, _)| *k == key) {
            s.cert_status = if choice == -1 {
                CERT_STATUS_CLEAN
            } else if choice == 1 {
                CERT_STATUS_USER_ACCEPTED
            } else {
                CERT_STATUS_BAD
            };
        }
        ret
    }

    /// Vide le canal d'événements (dispatch séparé, comme
    /// `IoEngine::drain_events` — voir [`crate::io::poll_io`]).
    pub fn drain_events(&mut self) -> Vec<TlsEvent> {
        let mut v = Vec::new();
        while let Ok(ev) = self.event_rx.try_recv() {
            v.push(ev);
        }
        v
    }
}

/// `Tls_make_bad_cert_msg` — condensé (le wording exact amont varie
/// selon les bits de statut mbedtls ; le vocabulaire est conservé).
fn bad_cert_msg(warnings: &[CertWarning]) -> String {
    let mut parts = Vec::new();
    for w in warnings {
        parts.push(match w {
            CertWarning::Expired => "The certificate has expired.",
            CertWarning::NameMismatch => "The certificate is not valid for this host.",
            CertWarning::Untrusted => "The certificate is not trusted.",
            CertWarning::NotYetValid => "The certificate start date is in the future.",
            CertWarning::Other => "The certificate could not be verified.",
        });
    }
    format!("The remote system cannot be trusted\n{}", parts.join("\n"))
}

/// `Tls_handshake_cb` amont — pilote `conn` via readiness jusqu'à
/// `!is_handshaking()` (≙ `mbedtls_ssl_handshake` rappelé par les
/// watches `WANT_READ`/`WANT_WRITE`).
async fn handshake_task(sock_id: i32, tls: Arc<TlsConn>, tx: mpsc::UnboundedSender<TlsEvent>) {
    loop {
        {
            let c = tls
                .conn
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !c.is_handshaking() {
                // ≙ `HANDSHAKE_OVER` : cert pair + cipher pour le
                // chemin `Tls_examine_certificate` / le log amont.
                let has_cert = c.peer_certificates().is_some();
                let cipher = c
                    .negotiated_cipher_suite()
                    .map(|cs| format!("{:?}", cs.suite()));
                let _ = tx.send(TlsEvent::Handshook {
                    sock_id,
                    has_cert,
                    cipher,
                });
                return;
            }
        }
        let (wants_r, wants_w) = {
            let c = tls
                .conn
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (c.wants_read(), c.wants_write())
        };
        let interest = match (wants_r, wants_w) {
            (true, false) => Interest::READABLE,
            (false, true) => Interest::WRITABLE,
            // les deux ou ni l'un ni l'autre → prudence ≙ watch duplex
            _ => Interest::READABLE | Interest::WRITABLE,
        };
        if tls.sock.ready(interest).await.is_err() {
            let _ = tx.send(TlsEvent::Failed { sock_id });
            return;
        }
        let res = {
            let mut conn = tls
                .conn
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut r: io::Result<()> = Ok(());
            if conn.wants_write() {
                r = conn.write_tls(&mut SockWrite(&tls.sock)).map(|_| ());
            }
            if r.is_ok() && conn.wants_read() {
                let _ = conn.read_tls(&mut SockRead(&tls.sock));
                // WouldBlock → prochain tour
            }
            if r.is_ok() {
                r = conn
                    .process_new_packets()
                    .map_err(io::Error::other)
                    .map(|_| ());
            }
            r
        };
        if res.is_err() {
            let _ = tx.send(TlsEvent::Failed { sock_id });
            return;
        }
    }
}

/// Parse PEM (multi-certs) ou DER nu — ≙ `mbedtls_x509_crt_parse_file`.
fn pem_or_der_parse(data: &[u8]) -> Vec<CertificateDer<'static>> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let mut out = Vec::new();
    let text = String::from_utf8_lossy(data);
    if text.contains(BEGIN) {
        let mut rest = text.as_ref();
        while let Some(s) = rest.find(BEGIN) {
            rest = &rest[s + BEGIN.len()..];
            let Some(e) = rest.find(END) else {
                break;
            };
            let b64: String = rest[..e].chars().filter(|c| !c.is_whitespace()).collect();
            rest = &rest[e + END.len()..];
            if let Some(der) = b64_decode(&b64) {
                out.push(CertificateDer::from(der));
            }
        }
    } else if !data.is_empty() {
        out.push(CertificateDer::from(data.to_vec()));
    }
    out
}

/// Base64 standard (≙ parsing PEM amont).
#[allow(clippy::cast_possible_truncation)]
// `as u8` isole l'octet émis : `acc` conserve l'historique des bits,
// l'octet utile occupe toujours les 8 bits bas après le shift.
fn b64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc = 0u32;
    let mut nbits = 0u32;
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        acc = (acc << 6) | val(c)?;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((acc >> nbits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn url(s: &str) -> DilloUrl {
        crate::url::url_new(s.as_bytes(), None).unwrap()
    }

    #[test]
    fn connect_ready_state_machine() {
        let runtime = rt();
        let mut eng = TlsEngine::new(runtime.handle().clone()).unwrap();
        let mut u = url("https://example.com/");
        // 1re fois : crée le serveur RECEIVING → READY
        assert_eq!(eng.connect_ready(&mut u), TLS_CONNECT_READY);
        assert_eq!(eng.cert_status(&mut u), CERT_STATUS_RECEIVING);
        // tant que le cert n'est pas décidé → NOT_YET
        assert_eq!(eng.connect_ready(&mut u), TLS_CONNECT_NOT_YET);
        // reset → NONE puis → READY (re-RECEIVING)
        eng.reset_server_state(&mut u);
        assert_eq!(eng.connect_ready(&mut u), TLS_CONNECT_READY);
        // BAD → NEVER
        {
            let key = server_key(&mut u);
            if let Some((_, s)) = eng.servers.iter_mut().find(|(k, _)| *k == key) {
                s.cert_status = CERT_STATUS_BAD;
            }
        }
        assert_eq!(eng.connect_ready(&mut u), TLS_CONNECT_NEVER);
        // autre port → serveur distinct ; host insensible casse
        let mut u2 = url("https://EXAMPLE.com:444/");
        assert_eq!(eng.connect_ready(&mut u2), TLS_CONNECT_READY);
    }

    #[test]
    fn after_handshake_dialog_paths() {
        let runtime = rt();
        // dialog « Continue » programmé
        let mut eng = TlsEngine::with_dialog(runtime.handle().clone(), Rc::new(|_, _| 1)).unwrap();
        let mut u = url("https://example.com/");
        eng.connect_ready(&mut u);
        // pas de cert + Continue → accepté USER_ACCEPTED
        assert!(eng.after_handshake(&mut u, false));
        assert_eq!(eng.cert_status(&mut u), CERT_STATUS_USER_ACCEPTED);

        // dialog « Cancel » → BAD
        let mut eng2 = TlsEngine::with_dialog(runtime.handle().clone(), Rc::new(|_, _| 2)).unwrap();
        let mut u = url("https://example.com/");
        eng2.connect_ready(&mut u);
        assert!(!eng2.after_handshake(&mut u, false));
        assert_eq!(eng2.cert_status(&mut u), CERT_STATUS_BAD);
        assert!(!eng2.certificate_is_clean(&mut u));
    }

    #[test]
    fn after_handshake_clean_cert() {
        let runtime = rt();
        let mut eng = TlsEngine::new(runtime.handle().clone()).unwrap();
        let mut u = url("https://example.com/");
        eng.connect_ready(&mut u);
        // cert + aucun warning → CLEAN
        assert!(eng.after_handshake(&mut u, true));
        assert_eq!(eng.cert_status(&mut u), CERT_STATUS_CLEAN);
        assert!(eng.certificate_is_clean(&mut u));
    }

    #[test]
    fn b64_roundtrip() {
        assert_eq!(b64_decode("aGVsbG8="), Some(b"hello".to_vec()));
        assert_eq!(b64_decode("AAAA"), Some(vec![0, 0, 0]));
        assert_eq!(b64_decode("QQ=="), Some(vec![b'A']));
    }
}

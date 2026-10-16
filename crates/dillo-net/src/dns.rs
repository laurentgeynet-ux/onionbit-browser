//! Port of `src/dns.c` + `src/dns.h` — résolveur DNS non-bloquant avec
//! cache et file FIFO partagée par hostname.
//!
//! Mapping :
//! - `D_DNS_MAX_SERVERS` = 4 canaux (build amont threaded par défaut ;
//!   [`Dns::new_sync`] = chemin `!D_DNS_THREADED` pour les tests).
//! - Thread `Dns_server` → `tokio::task::spawn_blocking` +
//!   `std::net::ToSocketAddrs` (getaddrinfo via std).
//! - `dns_notify_pipe` + `Dns_timeout_client` → `mpsc` interne + [`Dns::poll`]
//!   appelé par la boucle d'événements (single-writer préservé).
//! - `DilloHost {af, alen, data[16]}` → [`IpAddr`] (`dillohost_to_string`
//!   → `Display` ; les chaînes d'erreur `inet_ntop` sont inatteignables
//!   en Rust — divergence documentée).
//! - `cb_func(status, addr_list, cb_data)` → `FnOnce(i32, Option<Vec<IpAddr>>)`
//!   ; le `void* cb_data` devient la capture de la closure.
//!
//! Quirks amont préservés : cache sans TTL ni éviction, scan linéaire
//! ASCII-insensible, partage de canal par hostname, file d'attente `−2`
//! FIFO, `status` = code `getaddrinfo`.

use std::collections::VecDeque;
use std::net::{IpAddr, ToSocketAddrs};

use dillo_common::dlib;
use tokio::sync::mpsc;
use tracing::{debug, trace};

/// `D_DNS_MAX_SERVERS` — build threaded amont.
pub const MAX_SERVERS: usize = 4;

/// `DnsCallback_t` — `(status, addr_list)` invoquée sur le thread
/// principal (`poll`), jamais sur la tâche de résolution.
/// `status` = 0 succès, sinon code `getaddrinfo` amont (≙ errno/std).
pub type DnsCallback = Box<dyn FnOnce(i32, Option<Vec<IpAddr>>)>;

/// `GDnsQueue` — entrée de file ; `channel == -2` = en attente.
struct QueueEntry {
    channel: i32,
    hostname: String,
    cb: Option<DnsCallback>,
}

/// `DnsServer` — canal de résolution.
#[derive(Default)]
struct Server {
    state: ServerState,
    hostname: String,
}

#[derive(Default, PartialEq)]
enum ServerState {
    #[default]
    Idle,
    Processing,
}

/// Résultat posté par la tâche de résolution (`Dns_server` amont).
struct DnsDone {
    channel: usize,
    hostname: String,
    result: Result<Vec<IpAddr>, i32>,
}

/// `Dns` — état complet du sous-système (statiques amont → instance).
pub struct Dns {
    /// `dns_cache` — paires (hostname ascii-ci → adresses), jamais évincées.
    cache: Vec<(String, Vec<IpAddr>)>,
    /// `dns_queue` — callbacks en attente (FIFO).
    queue: Vec<QueueEntry>,
    /// `dns_server[]` — canaux.
    servers: Vec<Server>,
    /// `true` = mode `D_DNS_THREADED` (`spawn_blocking`) ;
    /// `false` = résolution synchrone (`!D_DNS_THREADED` amont).
    threaded: bool,
    /// `dns_notify_pipe[1]` — côté émission des tâches.
    done_tx: mpsc::UnboundedSender<DnsDone>,
    /// `dns_notify_pipe[0]` — vidé par [`Dns::poll`].
    done_rx: mpsc::UnboundedReceiver<DnsDone>,
    /// `tokio` runtime pour `spawn_blocking` (`None` en mode sync).
    rt: Option<tokio::runtime::Handle>,
    /// File des résultats arrivés en mode sync (exécutés à la demande).
    sync_done: VecDeque<DnsDone>,
}

impl std::fmt::Debug for Dns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dns")
            .field("cache", &self.cache.len())
            .field("queue", &self.queue.len())
            .finish_non_exhaustive()
    }
}

/// `getaddrinfo` côté serveur : `AF_UNSPEC` + `SOCK_STREAM`,
/// erreur → `Err(code)` (1 générique ; les codes `gai` précis n'ont pas
/// d'équivalent std — divergence documentée).
fn resolve_host(hostname: &str) -> Result<Vec<IpAddr>, i32> {
    // hints.ai_socktype = SOCK_STREAM → port 0 suffit à l'équivalent
    match (hostname, 0u16).to_socket_addrs() {
        Ok(iter) => {
            let hosts: Vec<IpAddr> = iter.map(|sa| sa.ip()).collect();
            if hosts.is_empty() {
                Err(0) // amont : hosts==NULL → status reste `error`…
            } else {
                Ok(hosts)
            }
        }
        Err(_) => Err(1),
    }
}

impl Dns {
    /// `a_Dns_init` mode threaded — nécessite un runtime tokio
    /// (`spawn_blocking`). `rt` peut être `Handle::current()`.
    #[must_use]
    pub fn new(rt: tokio::runtime::Handle) -> Self {
        Self::with(rt, true)
    }

    /// Chemin `!D_DNS_THREADED` : `Dns_server` exécuté inline à
    /// `Dns_server_req` — déterministe, sans runtime (tests, headless).
    #[must_use]
    pub fn new_sync() -> Self {
        let (done_tx, done_rx) = mpsc::unbounded_channel();
        Self {
            cache: Vec::new(),
            queue: Vec::new(),
            servers: (0..MAX_SERVERS).map(|_| Server::default()).collect(),
            threaded: false,
            done_tx,
            done_rx,
            rt: None,
            sync_done: VecDeque::new(),
        }
    }

    fn with(rt: tokio::runtime::Handle, threaded: bool) -> Self {
        let (done_tx, done_rx) = mpsc::unbounded_channel();
        Self {
            cache: Vec::with_capacity(16),
            queue: Vec::with_capacity(16),
            servers: (0..MAX_SERVERS).map(|_| Server::default()).collect(),
            threaded,
            done_tx,
            done_rx,
            rt: Some(rt),
            sync_done: VecDeque::new(),
        }
    }

    /// `a_Dns_resolve` — hit cache → callback immédiat ; doublon en file →
    /// partage du canal ; nouveau → canal libre ou file `−2`.
    pub fn resolve(&mut self, hostname: &str, cb: DnsCallback) {
        // cache : scan linéaire ascii-insensible (amont)
        if let Some((_, addrs)) = self
            .cache
            .iter()
            .find(|(h, _)| dlib::str_ascii_casecmp(h.as_bytes(), hostname.as_bytes()) == 0)
        {
            cb(0, Some(addrs.clone()));
            return;
        }
        if let Some(i) = self.queue_find(hostname) {
            let channel = self.queue[i].channel;
            self.queue_add(channel, hostname, cb);
            return;
        }
        if let Some(channel) = self
            .servers
            .iter()
            .position(|s| s.state == ServerState::Idle)
        {
            self.queue_add(i32::try_from(channel).unwrap_or_default(), hostname, cb);
            self.server_req(channel, hostname);
        } else {
            self.queue_add(-2, hostname, cb);
        }
    }

    /// `Dns_queue_add`.
    fn queue_add(&mut self, channel: i32, hostname: &str, cb: DnsCallback) {
        self.queue.push(QueueEntry {
            channel,
            hostname: hostname.to_string(),
            cb: Some(cb),
        });
    }

    /// `Dns_queue_find` — index ou `None` (amont `-1`).
    fn queue_find(&self, hostname: &str) -> Option<usize> {
        self.queue
            .iter()
            .position(|e| dlib::str_ascii_casecmp(e.hostname.as_bytes(), hostname.as_bytes()) == 0)
    }

    /// `Dns_queue_remove`.
    fn queue_remove(&mut self, index: usize) {
        if index < self.queue.len() {
            self.queue.remove(index);
        }
    }

    /// `Dns_server_req` — marque le canal et lance la résolution.
    fn server_req(&mut self, channel: usize, hostname: &str) {
        self.servers[channel].state = ServerState::Processing;
        self.servers[channel].hostname = hostname.to_string();
        if self.threaded {
            let tx = self.done_tx.clone();
            let host = hostname.to_string();
            if let Some(rt) = &self.rt {
                rt.spawn_blocking(move || {
                    let result = resolve_host(&host);
                    let _ = tx.send(DnsDone {
                        channel,
                        hostname: host,
                        result,
                    });
                });
            }
        } else {
            let result = resolve_host(hostname);
            self.sync_done.push_back(DnsDone {
                channel,
                hostname: hostname.to_string(),
                result,
            });
        }
    }

    /// `Dns_cache_add`.
    fn cache_add(&mut self, hostname: String, addrs: Vec<IpAddr>) {
        trace!(cache = self.cache.len() + 1, "dns cache_add");
        self.cache.push((hostname, addrs));
    }

    /// `Dns_serve_channel` — sert tous les callbacks du canal.
    fn serve_channel(&mut self, channel: usize, result: &Result<Vec<IpAddr>, i32>) {
        let (status, addrs) = match result {
            Ok(v) => (0, Some(v.clone())),
            Err(e) => (*e, None),
        };
        let mut i = 0;
        while i < self.queue.len() {
            if self.queue[i].channel == i32::try_from(channel).unwrap_or_default() {
                let cb = self.queue[i].cb.take();
                self.queue_remove(i);
                if let Some(cb) = cb {
                    cb(status, addrs.clone());
                }
            } else {
                i += 1;
            }
        }
    }

    /// `Dns_assign_channels` — canaux libres → file `−2` (FIFO, partage
    /// par hostname identique ascii-ci).
    fn assign_channels(&mut self) {
        for ch in 0..self.servers.len() {
            if self.servers[ch].state != ServerState::Idle {
                continue;
            }
            let Some(i) = self.queue.iter().position(|e| e.channel == -2) else {
                return;
            };
            let host = self.queue[i].hostname.clone();
            for e in &mut self.queue {
                if e.channel == -2
                    && dlib::str_ascii_casecmp(e.hostname.as_bytes(), host.as_bytes()) == 0
                {
                    e.channel = i32::try_from(ch).unwrap_or_default();
                }
            }
            self.server_req(ch, &host);
        }
    }

    /// `Dns_timeout_client` — à appeler depuis la boucle d'événements :
    /// vide le canal de notification, sert les canaux résolus, réassigne.
    ///
    /// En mode sync, joue aussi les résolutions en attente (`poll` sert
    /// de point de progression, comme le pipe amont sert de wakeup).
    pub fn poll(&mut self) {
        let mut done: Vec<DnsDone> = Vec::new();
        while let Ok(d) = self.done_rx.try_recv() {
            done.push(d);
        }
        while let Some(d) = self.sync_done.pop_front() {
            done.push(d);
        }
        for d in done {
            let ch = d.channel;
            if let Ok(addrs) = &d.result {
                self.cache_add(d.hostname.clone(), addrs.clone());
            }
            self.serve_channel(ch, &d.result);
            self.servers[ch].state = ServerState::Idle;
            debug!(channel = ch, host = %d.hostname, "dns resolved");
        }
        self.assign_channels();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    type Recorder = Rc<RefCell<Vec<Option<usize>>>>;
    fn rec() -> (impl Fn(i32, Option<Vec<IpAddr>>) + 'static, Recorder) {
        let hits = Rc::new(RefCell::new(Vec::new()));
        let h2 = hits.clone();
        (
            move |_st, addrs| h2.borrow_mut().push(addrs.map(|a| a.len())),
            hits,
        )
    }

    #[test]
    fn sync_resolve_localhost() {
        let mut dns = Dns::new_sync();
        let (cb, hits) = rec();
        dns.resolve("localhost", Box::new(cb));
        dns.poll();
        assert_eq!(hits.borrow().len(), 1);
        assert!(hits.borrow()[0].is_some_and(|n| n >= 1));
    }

    #[test]
    fn cache_hit_is_immediate() {
        let mut dns = Dns::new_sync();
        dns.cache_add("example.com".to_string(), vec!["1.2.3.4".parse().unwrap()]);
        let (cb, hits) = rec();
        dns.resolve("EXAMPLE.COM", Box::new(cb)); // ascii-ci
        assert_eq!(*hits.borrow(), vec![Some(1)]);
        // rien dans la file
        assert!(dns.queue.is_empty());
    }

    #[test]
    fn queue_shares_channel_same_hostname() {
        let mut dns = Dns::new_sync();
        let (cb1, h1) = rec();
        let (cb2, h2) = rec();
        dns.resolve("localhost", Box::new(cb1));
        dns.resolve("LOCALHOST", Box::new(cb2)); // même canal (ascii-ci)
        assert_eq!(dns.queue.len(), 2);
        assert_eq!(dns.queue[0].channel, dns.queue[1].channel);
        dns.poll();
        assert_eq!(h1.borrow().len(), 1);
        assert_eq!(h2.borrow().len(), 1);
    }

    #[test]
    fn waiting_queue_minus2_is_fifo() {
        // forcer 5 requêtes distinctes > MAX_SERVERS → la 5e attend en −2
        let mut dns = Dns::new_sync();
        // simuler canaux occupés : on injecte directement
        for (i, s) in dns.servers.iter_mut().enumerate() {
            s.state = ServerState::Processing;
            s.hostname = format!("h{i}");
        }
        let (cb, _h) = rec();
        dns.resolve("queued.example", Box::new(cb));
        assert_eq!(dns.queue[0].channel, -2);
        // libère le canal 0 → assign_channels le reprend
        dns.servers[0].state = ServerState::Idle;
        dns.assign_channels();
        assert_eq!(dns.queue[0].channel, 0);
    }

    #[test]
    fn failed_resolution_status() {
        let mut dns = Dns::new_sync();
        let (cb, hits) = rec();
        // nom invalide pour to_socket_addrs → Err
        dns.resolve("not a host name!!", Box::new(cb));
        dns.poll();
        assert_eq!(*hits.borrow(), vec![None]);
    }
}

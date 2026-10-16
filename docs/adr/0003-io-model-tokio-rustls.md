# ADR-0003 — Modèle I/O : tokio + rustls, single-writer pour le layout

- **Statut** : accepté · **Date** : 2026-10-16

## Contexte

Dillo upstream est mono-thread événementiel : boucle FLTK +
`iowatch` dispatchant des callbacks `IO` par fd, plus un thread
helper DNS (`gethostbyname` bloquant). Parsing, décodage d'images et
layout sont incrémentaux (chaînes de callbacks `Cache`/`chain.c`).

## Décision

1. **Tokio** pour tout l'I/O (`dillo-net`) : `iowatch`/`IO.c` →
   `AsyncRead`/`AsyncWrite` + tasks ; `dns.c` →
   `tokio::net::lookup_host` ; `timeout.cc` → `tokio::time` +
   timers winit côté UI.
2. **Single-writer** : l'arbre de widgets dw, le doctree et le style
   engine vivent sur la task UI ; réseau/décodeurs livrent des
   chunks par `mpsc`. La chaîne `chain.c` (producteur→filtres→
   consommateur) devient un pipeline typé de channels — la
   granularité par chunk est **préservée** (rendu progressif =
   comportement).
3. **TLS = rustls** uniquement (remplace `tls_openssl.c` +
   `tls_mbedtls.c`, lignes `won't port — replaced by rustls`).
   Provider crypto : `ring` toléré (asm précompilé, pas de C
   compilé dans notre arbre) ; si le « 100 % Rust » se durcit,
   bascule possible vers un provider pur-Rust — point rouvert si
   exigence stricte.
4. **HTTP gardé maison** : `http.c` porté (formatage de requêtes,
   redirects, auth digest/basic) — pas de reqwest/hyper : le format
   wire exact est partie intégrante de la parité, et hyper lierait
   trop de surface.
5. Injection : tout accès socket passe par des traits dans
   `dillo-net` (connectors) pour tests in-process et futur
   sandboxing.
6. **Idle-callback → winit** : `Fl::add_idle` (layout/rendu
   progressif quand la queue OS est vide) se traduit en
   `ControlFlow`/`AboutToWait` + `UserEvent` côté winit — tranches
   de travail bornées, réveil sur `UserEvent` plutôt que `Poll`
   permanent (pas de spin CPU une fois la page posée). Validé en
   phase 0.5.

## Conséquences

- Le layout synchrone n'est jamais bloqué par une socket ; les
  re-layouts incrémentaux reproduisent l'idle-callback FLTK via une
  file de « layout jobs » sur la task UI.
- Pas de `Arc<Mutex>` sur l'arbre de widgets — règle dure.

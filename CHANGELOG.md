# Changelog

All notable changes to DilloRust are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Repository scaffold: mission contract (`AGENTS.md`), feasibility
  study (`docs/feasibility.md`), phase plan (`ROADMAP.md`), full
  upstream→Rust file mapping (`docs/PORTING.md`).
- ADRs 0001–0005: port scope & fidelity, GUI stack
  (winit+softbuffer+tiny-skia), I/O model (tokio+rustls), DPI
  subsystem, widget-tree ownership (open, prototype-gated).
- 12 parity-audit skills in `.devin/skills/` (report/apply two-mode
  protocol, `docs/audits/` outputs).
- Cargo workspace : 11 crates squelettes (`dillo-common` → `dillo`,
  + `dillo-dw-proto` pour l'ADR-0005), `#![forbid(unsafe_code)]`,
  lints workspace (unsafe forbidden, clippy pedantic warn), CI
  GitHub Actions (fmt/clippy/test/doc + MSRV 1.85 sur ubuntu+windows).
- `dillo-common::dlib` : port de `dlib/dlib.c` — `Dstr`, `Dlist<T>`,
  helpers `dStr*`/`dIs*` ASCII, `parse_rc_line`, `getline` (14 tests).
- `dillo-common::lout` : port de `lout/` —
  `misc` (`SimpleVector`, `NotSoSimpleVector` fidèle avec extra-
  arrays, `StringBuffer`, `BitSet` avec sa sur-allocation amont,
  `round_int`), `unicode` (décodeur UTF-8 lenient + table alpha
  verbatim), `object` (traits + enum `Value`), `signal` (`Emitter<R>`
  sur `Weak<RefCell<R>>`, `emit_bool` sans court-circuit),
  `identity` (trait `Identifiable`). `container`/`debug`/`msg` →
  won't port (collections std / `tracing`). 15 tests ajoutés.
- `dillo-dw-proto` : prototype ADR-0005 (3 options × scénario
  `queue_resize` en pleine itération) → **option D retenue** :
  arène `Layout` + `WidgetId` + `enum WidgetKind` + `LayoutCtx`.
  B (`Rc`/`Weak`) a paniqué `BorrowMutError` au premier essai puis
  exigé try_borrow+defer partout ; A (`dyn`) paie take/replace +
  downcast sans gain. ADR-0005 amendé `accepté` + règles de port
  `dw/` (liens en ids, take/copy avant récursion, events drainés).
- `dillo-platform::golden` : harness golden-render headless —
  `Pixmap` offscreen tiny-skia, diff pixel-à-pixel (compte, bbox,
  delta canal), PNG load/save, `assert_golden` avec régénération
  via `DILLO_UPDATE_GOLDEN=1`. Deps : `tiny-skia` 0.11 (ADR-0002),
  `png` 0.17 (pur Rust — stockage des images de référence).
- **Phase 0.5 — tranche verticale** : `dillo` bin ouvre une fenêtre
  winit, rend blocs+liens via tiny-skia/cosmic-text, navigue par
  clic (`file://`). Modules : `dillo-html::minidoc` (parseur
  tolérant h1-6/p/a/entités), `dillo-net::fetch` (`file://` +
  `resolve_href` URL-semantics), `dillo-platform::surface`
  (winit+softbuffer+pixmap blit). `ControlFlow::Wait` sans spin.
  Deps : `winit` 0.30, `softbuffer` 0.4, `cosmic-text` 0.12
  (ADR-0002). Pilote — remplacé par dw phase 4.
- `dillo-common::utf8` : port de `src/utf8.cc`/`utf8.hh` —
  `end_of_char`, `decode` lenient (fallback octet brut type
  ISO-8859-1), `encode` aux formes historiques 1–6 octets de
  `fl_utf8encode`, `test` (catégories 0–4), `ideographic`,
  `combining_char`, `char_count`, constantes U+FFFD/U+200B (8 tests).
- `dillo-common::misc` : port de `src/misc.c`/`misc.h` —
  `escape_chars`, `expand_tabs` (taquets 8 + queue de diacritiques
  combinants), sniffing `get_content_type_from_data`,
  `parse_content_type`/`cmp`/`check`, `parse_geometry`,
  `parse_search_url`, `encode_base64`, `parse_content_disposition`
  (filename sanitizé), `file_to_vec`. Quirks amont reproduits et
  documentés : `content_type_check` `-1` inatteignable ; le sniffing
  classe `text/plain` tout buffer UTF-8-valide (même octets de
  contrôle) (12 tests).
- `dillo-common::colors` : port de `src/colors.c` — table
  `EXTENDED_COLOR` verbatim (147 entrées triées), `color_parse`
  (`#`/`0x`/nom/hex nu + statut err 0/1/2), `color_vc` avec scores
  `distance2`/`distance3` verbatim (4 tests).
- `dillo-net::url` : port de `src/url.c`/`url.h`/`src/IO/Url.h` —
  `DilloUrl` (champs owned au lieu de vues NUL), parse lenient,
  `resolve_relative` RFC-3986 avec les mêmes hacks amont (dot-
  segments, hack `/` pré-authority, truncate `query_sep`), encodage
  `%XX` des chars illégaux, heuristique `http:`/`//` par défaut,
  `hostname`/`port` lazy (ipv6 `[]`, défauts 80/443), `url_cmp`
  (ordre amont), `decode`/`encode_hex_str`, `strip_delimiters`,
  `host_type`, `host_public_suffix` + liste TLD, `set_ismap_coords`.
  Switch HSTS isolé : `switch_to_https` reproduit la transform
  (`:80`→`:443`, port, scheme) — le déclencheur `prefs`/`hsts` se
  branchera avec leurs ports. 18 tests dont la batterie RFC-3986
  §5.4 complète (normal + abnormal).
- `dillo-browser::prefs` : port de `src/prefs.c` + `prefsparser.cc` —
  `Prefs` owned (pas de global), `Default` = `a_Prefs_init` verbatim
  (~110 champs, UA `Dillo/3.3.0-rc1`), `parse_dillorc` ligne à ligne
  via `dlib::parse_rc_line`, `parse_option` = symbol table → match
  avec les 12 types (BOOL/COLOR/STRING/STRINGS/URL/INT32/DOUBLE/
  FRACTION_100/GEOMETRY/PANEL_SIZE), sémantique « 1re occurrence
  vide le défaut » pour les listes, `strtod`/`strtol` lenients.
- `dillo-net::decode` : port de `src/decode.c` — `DecodeTransfer`
  (chunked : `strtol` hex sur buffer non-borné → whitespace C/`\n`,
  signe, préfixe `0x`, troncature `long`→`int` ; fin sur longueur 0
  ou garbage), `Decode` (gzip : en-tête RFC1952 + FHCRC + trailer
  CRC32/ISIZE vérifiés à la main — `miniz_oxide` ne gère pas le
  wbits=31 ; deflate : fallback raw sur `Z_DATA_ERROR` avec rejeu
  de l'input ; brotli ; charset `encoding_rs` avec skip 1 octet
  par `EILSEQ`). Deps : `flate2` (rust_backend), `brotli` 8.x,
  `encoding_rs`, `crc32fast`.
- `dillo-net::chain` : port de `src/chain.c`/`chain.h` — CCC
  (`Concomitant Control Chain`) verbatim : arène `LinkId` + free-list
  (ADR-0005), `new_link`/`link_new`/`unlink`/`free`, callbacks FWD/BCK
  + `bfcb` (flag end/abort effacé entre les deux sens comme amont),
  flags `CCC_Stopped`/`Ended`/`Aborted`, `LocalKey`→`Box<dyn Any>`,
  `void* Data1/Data2`→`ChainData` (`None`/`Buf`/`Int`/`Str`/`Handle`/
  `Owned` partagé par `Rc`), `check` + log d'avertissement verbatim.
  Callbacks `Rc<dyn Fn>` clonés avant appel → réentrance/unlink-safe.
- `dillo-net::dns` : port de `src/dns.c` — cache d'adresses, file FIFO
  avec déduplication insensible à la casse, `NUM_DNS_SERVERS` = 4
  canaux de résolution, serve/channel `-1`/`-2` verbatim ; threads
  amont → `tokio::task::spawn_blocking`, notification → mpsc +
  `Dns::poll` (callbacks non-`Send`, tout reste sur le thread
  principal). 5 tests (cache, dédup, FIFO, échec, sync).
- `dillo-net::io` : port de `src/IO/IO.c` + `iowatch.cc`/`IO.h` —
  `a_IO_ccc` verbatim (branches 1B/1F/2B/2F : write par `OpSend`
  buffer/`"FD"`, read par events), `IoEngine` = `ValidIOs` +
  `ValidSocks` (table fd entiers ≙ numéros amont). `Fl::add_fd` →
  tâches tokio (`ready().await` + `try_read`/`try_write`), callbacks
  `IO_fd_*_cb` → `IoEvent` mpsc dispatchés par `poll_io` (garde
  `IO_get` reproduite contre la réentrance — un `OpSend` peut avoir
  aborté la chaîne). Fidélité fd partagé : `"FD"` → `clone_socket`
  (≙ `dup` via `socket2`), `IO_StopRd`/`IO_StopWr` retirent la watch
  sans fermer, seul `IO_StopRdWr` fait `dClose` (`shutdown(Both)`).
  Déviation corrigée vs BUG amont : le double-close de la race Abort
  disparaît par ownership. Dep : `socket2` 0.6.5. Test de régression
  `shared_fd_survives_writer_close` sur vraie socket loopback.
- `dillo-net::mime` : port de `src/IO/mime.c` — `MimeRegistry`
  (listes minor/major ordonnées), `get_viewer` verbatim : scan unique
  MinSize/MajSize avec le quirk du `'/'` initial (`MajSize` = 2e
  slash), `dStrnAsciiCasecmp` préfixe dans les deux sens ; `Viewer_t`
  → enum `ViewerId` en attendant les vrais modules (4 tests dont
  quirks). `src/IO/proto.c` = won't port (stub vide amont).
- `dillo-net::about` : `ABOUT_SPLASH` verbatim (VERSION 3.3.0-rc1).
- `dillo-net::tls` : port de `src/IO/tls.c`/`tls.h` + backend
  `tls_mbedtls.c` — `TlsEngine` = `Tls_servers` par (host-ci, port)
  avec state machine `connect_ready` (NONE/RECEIVING/CLEAN/BAD/
  USER_ACCEPTED → `CONNECT_NEVER`/`NOT_YET`/`READY`), `DilloVerifier`
  = `MBEDTLS_SSL_VERIFY_OPTIONAL` (webpki enregistre `CertWarning`s
  au lieu d'échouer — la décision reste post-handshake via
  `after_handshake` = `Tls_check_certificate` + `a_Dialog_choice`
  injecté), `TlsConn` partagé par `Arc`+`Mutex` ≙ `io->FD`/`conn`
  partagé lecteur+écrivain (handshake conduit en tâche tokio ≙
  `Tls_handshake_cb` sous iowatch ; `Mutex` jamais tenu à travers
  `.await`), `IoStream::Tls` dispatché dans `try_read`/`try_write`/
  `ready`/`try_clone`/`shutdown_both` (`send_close_notify` ≙
  `mbedtls_ssl_close_notify`), `read_task` réordonné drain-puis-wait
  = ordre `read()` amont + couvre le plaintext bufferisé sans
  readiness. CA : `rustls-native-certs` ≙ `Tls_load_certificates` +
  `add_ca_file` PEM/DER ≙ `Tls_load_certificates_from_file`. Deps :
  `rustls` 0.23.46 (provider `aws-lc-rs` — tension « 100 % natif »
  documentée ADR-0003, `rustls-rustcrypto` alpha-only), `rustls-
  native-certs` 0.8. `tls_openssl.c` → won't port (backend
  alternatif). 4 tests (b64, state machine, cert clean, dialog).
- `dillo-net::http` : port de `src/IO/http.c` — transport pur (la
  lecture/parse de réponse reste chez `cache.c`, pas porté).
  `HttpEngine` = `ValidSocks` (zombies `TO_BE_FREED` en file ≙
  `dFree` différé) + `servers` par (host-ci, port, https) + `fd_map`.
  `Http_socket_enqueue` verbatim (non-images devant le 1er
  `WEB_Image` valide — la socket entrante testée sans `valid`,
  zombies de file visibles comme amont). `connect_queued_sockets` :
  balayage zombies, `TLS_CONNECT_*` par socket TLS, `active_conns`
  ≤ `http_max_conns`, `running_the_queue` anti-réentrance, retrait
  du serveur vide. Proxy : env `http_proxy` > prefs, `no_proxy`
  suffix-match ascii-ci avec quirk `dStrsep` (token vide = match
  tout), `CONNECT host[:443]` + `Proxy-Authorization` base64,
  réponse accumulée jusqu'à `\r\n\r\n` avec statut à l'index 9.
  `make_query_str` verbatim : URI absolue proxy ( '#' tronqué),
  Accept par type (`WEB_*`), referer `host`/`path`/masqué, E2E →
  `Pragma`+`Cache-Control: no-cache`, boundary multipart extrait
  de `data[2..'\r')`, POST `Content-Length` = `data.len`.
  `reply_complete` → keep-alive reuse (fd transféré au socket en
  file, `active_conns` déplacé, `fd_map` retiré puis re-rempli,
  `connect_done(TRUE)`) ; `reuse_compatible` : TLS+proxy exige même
  host:port. `a_Http_ccc` 4 branches verbatim + `"FD"` ; connect
  non-bloquant + retry `addr_list` → `connect_task` tokio ≙
  `Http_connect_socket_cb` + `SO_ERROR` ; upgrade TLS = swap du
  stream `IoStream` dans `IoEngine` + `begin_handshake` ≙
  `a_Tls_connection` + `Tls_handshake_cb` ; `Tls_check_certificate`
  (USER_ACCEPTED court-circuite) puis `connect_done`. `DilloWeb` →
  `HttpWeb` (url/requester/flags/valid/MSG_BW) ; `cookies`/`auth`/
  `prefs` injectés (deps descendantes — défauts build `!HAVE_COOKIES`
  amont). Infra associée : `IoMsg::Swap` + `take`/`swap`/`remove`/
  `close_notify` dans `IoEngine`, `close_by_fd → Option<connecting>`
  (emprunts modules séquentiels), `DnsCallback` chain-aware +
  `dns_resolve`/`poll_dns`. 11 tests : goldens GET/POST/proxy/
  CONNECT, quirks `no_proxy`, priorité de file, zombie+sweep,
  reuse_compatible, wire loopback end-to-end (requête octet-pour-
  octet), limite `max_conns` en vrai.

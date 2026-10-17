# Changelog

All notable changes to DilloRust are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `dillo-dw` style+interfaces image (phase 4, lot dw-1a) :
  `imgbuf` trait (`dw/imgbuf.hh` — verbatim, `Rc<dyn Imgbuf>`),
  `imgrenderer` (`ImgRenderer` + `ImgRendererDist`, snapshot Rc),
  `style` complet modulo draw_* : tous les enums, `Length`
  packing verbatim + `roundInt` ≙ `(d>0?d+0.5:d-0.5)`, `Box`,
  `StyleAttrs` (`initValues`/`resetValues`/`sizeDiffs`≙false,
  `equals` ptr-identité `Rc::ptr_eq`, `hashValue` somme
  `(intptr_t)`→adresses), `Style` + `styleTable` thread_local
  (déduplication ≙ pool statique), `Font`/`FontAttrs`,
  `Tooltip` (+trait `TooltipHooks` ≙ virtuals), `StyleImage`
  (partiel), `numtostr` verbatim (romain/alpha/`"****."`,
  `num=0`→`"."` quirk préservé).

- `dillo-dw` fondations (phase 4, lot dw-0) : `types`
  (`dw/types.cc/.hh` — `Rectangle`/`Circle`/`Polygon`/`Region`
  avec quirks verbatim : borne `y>=maxy` asymétrique, fusion
  forcée ≤16 rect, ray-casting sans garde sommet ; `Allocation`/
  `Requisition`/`Extremes`, `Content` union→enum, stacking
  contexts `HashSet<WidgetId>`), `events` (`dw/events.hh`),
  `tools` (`SizeParams`, refs `Widget*`→`WidgetId`),
  `style::Color`/`Shading`/`shadeColor` verbatim (+`view` seam :
  3 primitives `draw_*` pour `Shape::draw` — interface
  `dw/view.hh` complétée avec le lot layout).


- `dillo-images` décodeurs : `gif` (LZW maison amont porté verbatim
  — spill lines, `orig_code==last_code`, entrelacé 4 passes,
  `state=999`), `png` (crate `png` — sig check, `png_composite` +
  LUT gamma `png_set_gamma(2.2,·)` reproduits, gris→RGB manuel),
  `jpeg` (`jpeg-decoder` — decode-at-EOI, CMYK_INV ≙ Adobe),
  `webp` (`image-webp` — RIFF-complete check, composite
  `(c*a+bg*(255-a))/0xFF` verbatim), `svg` (`resvg`/`usvg` — attente
  `"</svg>"`, `dpi` ≙ `nsvgParse`, premultiplied→straight→composite
  amont). Décodeurs câblés dans la table dicache (≙ table statique
  amont) + `register_image_viewers` ≙ les `Mime_add_minor_type` de
  `a_Mime_init`. Déviations « decode-at-completion » (pas
  d'affichage progressif des lignes, multi-scan/interlace
  raffiné non reproduit) documentées par module pour
  `audit-09-images`.
- `dillo-net::mime` : port de `src/IO/mime.c` — registre
  `MimeMinItems`/`MimeMajItems`, `a_Mime_get_viewer` verbatim
  (quirk : clé plus courte matche via `MinSize`), `MimeViewer`/
  `ViewerOutcome` injectés (viewers texte → `dillo-html` plus tard).
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
- `dillo-net::{domain,hsts,history}` : pré-requis de `cache.c`/
  `capi.c`. `domain.c` : `Domain` (règles `origine→destination` +
  `default_deny`) — parsing `domainrc` verbatim (strtok, texte
  surnuméraire = warn mais règle ajoutée), `Domain_match` (`*`,
  exact ci, `.suffixe` frontière `diff==-1` sans le point),
  `permit` : short-circuit, `data:`/hôtes vides, match qui inverse
  `default_deny`. `hsts.c` : liste triée ci de politiques,
  `hsts_set` (attr/value/eat_value verbatim, `max-age`
  `strtol`+ERANGE→INT_MAX, `=0` retire, `preload` ignoré, garde
  `cert_clean` injectée ≙ `a_Tls_certificate_is_clean`), marche des
  domaines parents `strchr(host+1,'.')` + `subdomains`,
  `future_time` saturé ≙ hack `tm_sec=INT_MAX`, `preload` fichier
  `domain true|false` `ONE_YEAR`, `enabled` injecté ≙ pref.
  `history.c` : `History` thread_local ≙ globales, `add_url` dédup
  `url_cmp`+fragment → idx, titres avec fallback `URL_STR`,
  `set_title_by_url` scan inverse. 18 tests. Registres
  `thread_local` = globales amont (single-writer main).
- `dillo-net::{auth,digest,dpip,cookies}` : lot B des pré-requis
  `cache.c`/`capi.c`. `auth.c`/`auth.h` : parse de challenges
  `WWW-Authenticate` verbatim (cb-0 n'interrompt que le token en
  cours, `stale`/`algorithm` non quotés, `realm` ci), arbre
  `AuthHost`→`AuthRealm` (chemins `URL_PATH` complets,
  `path_is_inside` = préfixe + frontière `/`, longest-path),
  `stale` → refresh nonce sans dialog, `AuthDialog` injecté ≙
  `a_Dialog_user_password`, Digest avant Basic, `dStrshred` du mot
  de passe. `digest.c` : MD5 via `md-5`, cnonce `fastrand` ≙
  `rand()%16`, A1/A2 verbatim, ordre de champs IE + `nc=%08x`,
  `opaque` NULL-vs-"" distingué (`Option`). `dpip/dpip.c` (helpers
  protocole) : `build_cmd` (`%s`→`'val'` + stuffing `'`→`''`, EOT
  ` '>'`) et `get_attr_l` (machine SEEK_NAME→FOUND + destuff). 
  `cookies.c` : client `cookies.dpi` — parseur `cookiesrc`
  (`DEFAULT` + longest-domain-first, `.dom` suffixe sans apex),
  DENY→drop, cross-org non-racine→`""`, canal `DpiSend` injecté ≙
  `a_Dpi_send_blocking_cmd`, disabled sans contenu. Flags `URL_*`
  ajoutés à `url.rs` (`URL_Post` pour digest). 18 nouveaux tests.
- `dillo-net::{timeout,web,cache,capi}` : lot C — glue centrale
  navigateur. `timeout.cc` : file thread-local one-shot ≙
  `Fl::add_timeout`, `poll_timeouts` drainé main-thread (réentrant),
  `a_Timeout_remove` no-op amont porté, `actually_remove` par id.
  `web.cc` : `dispatch_by_type` — root→seam `WebDispatch`, non-root→
  `image/*` uniquement (filtre mime, pas `WEB_Image` — quirk amont) ;
  `ValidWebs` ≙ `HttpWeb::valid` partagé `Rc`/`Cell`. `cache.c` :
  entrées triées `url_cmp`, clients à clés, `process_dbuf` verbatim
  (header jusqu'à `\n\n`, strip `\r`/NUL, unfold garde l'espace de
  continuation — quirk, `100 Continue` reset), `parse_header` complet
  (30x→REDIRECT/FORCE/TEMP + blocage domainrc/dpi, 401→challenges,
  Set-Cookie→`cookies_set`, HSTS, Content-Length→GOT_LENGTH, HTTP/1.0
  retire KEEP_ALIVE, Content-Type hdr/meta/norm/det, Content-
  Disposition), chaîne transfer→content→charset→`UTF8Data` lazy
  refcountée, `process_queue` verbatim (dispatch, `AbortEntry`+
  `OfferDownload`, savelink différé, redirect→`nav.push`+
  `redirect_level`, page « blocked redirection » `text/html`, auth via
  timeout 0), delayed queue ≙ `timeout_add(0)`, `about:cache`/
  `about:dicache` `CA_InternalUrl` jamais retirés. Seams
  `BrowserOps`/`UiOps`/`NavOps`/`DicacheOps`/`WebDispatch` injectées.
  `capi.c` : `capi_conn_t`→`Rc<RefCell>` ≙ `Ref`+`CapiConns`, abort
  différé `pending_abort` si CCC en vol (`RefCell` impose le report ≙
  synchrone amont), CCC 4 branches verbatim (`"FD"`→sock_fd+relais,
  `"DpidOK"`→`conn_resume` des `PENDING`, `"DpidERROR"`/`"Both"`,
  `"send_page_2eof"`→`process_dbuf`+`"reply_complete"`,
  `send_status_message`/`chat`/`dialog`/`reload_request`→seams,
  `IO_CLOSE`/`IO_ABORT`→`process_dbuf`) ; `open_url` dispatch
  (download→fichier/dpi, `url_uses_dpi` verbatim `?`/`bm`→`bookmarks`/
  `proto.*`, `dpi_verify_request` GET|POST depuis page dpi seulement,
  `request_permitted` mixed-content https sauf `data:` + domainrc
  sauf cache-hit, http→2 branches recv-avant-send ≙ course DNS) ;
  seams `DpiOps`/`dpi_ccc` injectées (phase DPI). 24 nouveaux tests
  (137 total `dillo-net`).
- `dillo-common::{bitvec,klist}` + `dillo-browser::{nav,actions,
  bookmark,dpiapi}` : lot D. `bitvec.c` : quirk `BVEC_SIZE=1` (1
  octet/bit, bit 0 seul). `klist.c` : clés `i32` compteur, wrap
  complet à 0 seulement (clés négatives émises entre-temps),
  `Clean`. `nav.c` : `Nav` = champs `bw->nav_*`/`meta_refresh_*`
  amont ; pile verbatim, `expect_done` (ops pile uniquement si
  `expecting` — back/forw n'empilent pas), repush/reload/
  redirection0 différés `NavSched`, scroll saved/restored,
  `Nav_save_cb` via `HttpWeb::stream`/`saved_bytes` (Cell+RefCell,
  ouvert par capi WEB_Download) ; seams `NavBw`/`NavUi`/`NavSched`.
  `actions.c` : `label:cmd` strtok-verbatim, `Command` détaché +
  `DILLO_PID`. `bookmark.c` : machine chat `bookmarks` (H/I/O).
  `dpiapi.c` : dialog dpip → `DialogChoice` → `cmd=answer`. 18
  nouveaux tests.
- `dillo-images::{imgbuf,image,dicache}` : lot E (noyau images).
  `imgbuf.cc` : buffer RGB `Rc`-partagé, cascade de clamps
  `excessiveImageDimensions`/`MAX_WIDTH`/`MAX_HEIGHT`/`gamma`
  verbatim, `rgb_line` (INDEXED/GRAY/CMYK_INV `*white/0x100`/RGB).
  `image.cc` : `DilloImage` + seam `ImgRenderer`≙`I2IR`, machine
  `IMG_*`, multi-scan `BitVec`/`ScanNumber`. `dicache.c` : registre
  thread-local trié, versions `DIF_Last`/`DIF_Valid`, `surv_cleanup`,
  `decoded_size` incrémental, trait `ImageDecoder`≙`Decoder`/
  `DecoderData` fusionnés (take/appel/restore anti-réentrance
  `RefCell`), `dicache_image` + `ImageFactory`≙`a_Image_new_with_dw`,
  `about:dicache` verbatim, impl `cache::DicacheOps`. Glue :
  `HttpWeb::image` (`Any`≙`web->Image`), `cache_close_client`,
  `NavBw::bg_color`≙`a_Web_new(bw->bgColor)`. 16 nouveaux tests.
### dw-1b — imgbuf dw/imgbridge aligné
- `[dw]::imgbuf` trait complet (`set_cmap`/`copy_row`/`new_scan`/
  `get_scaled_buf`/`get_row_area`/`root_w·h`/`similar_buf`/`copy_to`/
  `ref·unref`/`last_reference`/`set_delete_on_unref`/`width·height`).
- `[img]::imgbuf` impl le trait dw : init **222** amont, arbre de
  scaled-buffers ≙`fltkimgbuf` (`copiedRows`, formules map root↔scaled
  verbatim, `newScan`/`copyRow` propagés aux fils), `getRowArea`,
  `copyTo` clip+blit, `setCMap` vide ≙`FltkImgbuf` RGB-only.
- `[img]::image` : seam renderer passée à `Rc<dyn Imgbuf>` (≙
  `I2IR::setBuffer`), `Bitvec` dérive `Debug`/`Clone`.
- **Déviation** : `scaleMode` amont `BEAUTIFUL_GAMMA` → `SIMPLE`
  (pixels non gamma-corrigés) — `BEAUTIFUL` en phase platform.


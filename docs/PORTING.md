# PORTING.md — Mapping C/C++ → Rust

Ground truth : `D:\Projet\dillo` (read-only). Statuts :
`not started` → `in progress` → `ported` (compile + tests unitaires)
→ `verified` (parité comportementale/golden render) — ou
`won't port` + raison.

Légende cible : `[common]`=dillo-common · `[net]`=dillo-net ·
`[css]`=dillo-css · `[html]`=dillo-html · `[dw]`=dillo-dw ·
`[img]`=dillo-images · `[plat]`=dillo-platform ·
`[browser]`=dillo-browser · `[dpi]`=dillo-dpi · `[bin]`=dillo (facade/bin)

## dlib/ — utilitaires C

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dlib/dlib.c` + `dlib.h` | `[common]::dlib` | ported | `Dstr`/`Dlist<T>`/`dStr*`/`dIs*`/`parse_rc_line`/`getline` ; `dMalloc`/`dClose`/`dUsleep`/`dGetcwd` non portés (std) |

## lout/ — utilitaires C++

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `lout/container.cc` + `.hh` | collections std | won't port | containers non-typés → `Vec`/`HashMap`/`VecDeque` |
| `lout/debug.hh` + `debug_rtfl.hh` | `tracing`/`debug_assert` | won't port | macros debug → `tracing`, `_MSG` → `debug!` |
| `lout/identity.cc` + `.hh` | `[common]::lout::identity` | ported | `Identifiable` trait (`class_name`/`ancestors`/`instance_of`) |
| `lout/misc.cc` + `.hh` | `[common]::lout::misc` | ported | `SimpleVector`/`NotSoSimpleVector`/`StringBuffer`/`BitSet`/`round_int` ; `ZoneAllocator` non porté (propriété Rust) ; sur-allocation BitSet reproduite |
| `lout/msg.h` | `tracing` | won't port | macros MSG → `tracing::{debug,info,error}` |
| `lout/object.cc` + `.hh` | `[common]::lout::object` | ported | `Object`/`Comparable` traits + enum `Value` (remplace wrappers Pointer/Integer/…/Pair des argv de signaux) |
| `lout/signal.cc` + `.hh` | `[common]::lout::signal` | ported | `Emitter<R>` + `Weak<RefCell<R>>` ; `emit_bool` sans court-circuit (note amont préservée) ; `ObservedObject`/`DeletionEmitter` → `Drop` |
| `lout/unicode.cc` + `.hh` | `[common]::lout::unicode` | ported | décodeur UTF-8 lenient (octet brut en fallback), table `is_alpha` verbatim, `next_utf8_char`/`num_utf8_chars` (+variantes `len` fidèles) |

## src/IO/ — couche I/O

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `src/IO/IO.c` + `IO.h` | `[net]::io` | ported | `a_IO_ccc` verbatim (1B/1F/2B/2F) ; `IOData`→`IoEngine` ; fd = `sock_id` table + clones `socket2` (fd partagé lecteur/écrivain — `IO_StopRd|Wr` ne ferment pas, seul `StopRdWr` fait `dClose`+shutdown) ; canal `IoMsg` (`Data`/`Swap`) vers tâches armées ≙ `a_Tls_connection(fd)` par-fd (upgrade TLS en place) |
| `src/IO/Url.h` | `[net]::url` (types) | ported | voir `src/url.c` — un seul type `DilloUrl` |
| `src/IO/about.c` | `[net]::about` | ported | `ABOUT_SPLASH` verbatim (VERSION=3.3.0-rc1 interpolée) |
| `src/IO/control.c` + `.h` | `[net]::control` | not started | contrôle shell → UI |
| `src/IO/dpi.c` | `[net]::dpi` | not started | client dpi:// |
| `src/IO/http.c` | `[net]::http` | ported | transport pur (le parsing réponse vit dans `cache.c` — pas porté) : `SocketData`/`Server`/`fd_map` verbatim, file par (host-ci,port,https) avec priorité non-images, `max_conns`+zombies `TO_BE_FREED` balayés, proxy env>prefs + `no_proxy` suffix-match (tokens vides matchent — quirk `dStrsep`), tunnel `CONNECT` + `:443`, query builder verbatim (Accept par type, referer host/path, E2E→no-cache, multipart boundary `data[2..'\r')`), keep-alive `reply_complete`→reuse avec transfert de fd, CCC 4 branches + `"FD"`, upgrade TLS par swap de stream ; `connect` non-bloquant + retry IP → `connect_task` tokio ; `DilloWeb`/`cookies`/`auth`/`prefs` injectés (deps descendantes) |
| `src/IO/iowatch.cc` + `.hh` | `[net]::io` | ported | `Fl::add_fd` → tâches tokio (`ready`+`try_read`/`try_write` = `read(2)`/`write(2)` non-bloquants) ; callbacks `IO_fd_*_cb` → `IoEvent` mpsc + `poll_io` (thread principal, single-writer) |
| `src/IO/mime.c` + `.h` | `[net]::mime` | ported | `MimeMinItems`/`MimeMajItems` registres ordonnés, `a_Mime_get_viewer` verbatim (MinSize=`;`/` `, MajSize=1er `/`, mineur→majeur, `dStrnAsciiCasecmp` quirk : clé **plus courte** matche le nom — `image/p`≡`image/png`) ; `Viewer_t`→`MimeViewer`(`Rc<dyn Fn>`)+`ViewerOutcome`(`call`,`view:Box<Any>`) — viewers injectés : `dillo-images::register_image_viewers` ≙ les `Mime_add_minor_type("image/*")` de `a_Mime_init` |
| `src/IO/proto.c` | — | won't port | stub vide amont (commentaire seul, aucun code) |
| `src/IO/tls.c` + `tls.h` | `[net]::tls` | ported | dispatcher verbatim : `TlsEngine` = `Tls_servers` (clé host-ci+port ≙ `htons`), state machine `connect_ready` NONE/RECEIVING/CLEAN/BAD/USER_ACCEPTED → `CONNECT_NEVER/NOT_YET/READY` ; `DilloVerifier` ≙ `VERIFY_OPTIONAL` (webpki enregistre les warnings au lieu d'échouer ; décision post-handshake `after_handshake` = `Tls_check_certificate` + `a_Dialog_choice` injecté) ; `a_Tls_connection` → `TlsConn` (`Arc`+`Mutex`, fd partagé lecteur/écrivain) intégré à `IoStream::Tls` ; handshake en tâche tokio ≙ `Tls_handshake_cb` ; CA natives `rustls-native-certs` ≙ `Tls_load_certificates` ; tension « 100 % rust natif » : provider `aws-lc-rs` (C/asm) — `rustls-rustcrypto` alpha-only, cf. ADR-0003 |
| `src/IO/tls_mbedtls.c` + `.h` | `[net]::tls` | ported | backend porté : `DilloVerifier` = `VERIFY_OPTIONAL` (warnings collectés, pas de rejet immédiat), `CertStatus` NONE→RECEIVING→CLEAN/BAD/USER_ACCEPTED verbatim, `after_handshake` = `Tls_check_certificate`+dialog ; glue rustls ≠ mbedtls (API seule) |
| `src/IO/tls_openssl.c` + `.h` | — | won't port | backend alternatif amont — rustls couvre les deux (ADR-0003) |

## src/ — parsing & engines

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `src/html.cc` + `html.hh` | `[html]::parser` | not started | machine d'état tag-soup tolérante |
| `src/html_common.hh` | `[html]::common` | not started | |
| `src/html_charrefs.h` | `[html]::entities` | not started | table entités → codegen/static |
| `src/doctree.hh` | `[html]::doctree` | not started | |
| `src/plain.cc` | `[html]::plain` | not started | doc text/plain |
| `src/table.cc` + `table.hh` | `[html]::table` | not started | glue HTML→dw table |
| `src/form.cc` + `form.hh` | `[html]::form` | not started | modèle forms + submit |
| `src/css.cc` + `css.hh` | `[css]::css` | not started | decl sets, matching, specificity |
| `src/cssparser.cc` + `cssparser.hh` | `[css]::parser` | not started | tokenizer CSS tolérant |
| `src/styleengine.cc` + `styleengine.hh` | `[css]::styleengine` | not started | cascade → dw::core::style |
| `src/utf8.cc` + `utf8.hh` | `[common]::utf8` | ported | end_of_char/decode/encode(1-6 octets)/test/ideographic/combining/char_count ; constantes U+FFFD & U+200B |

## src/ — réseau & services document

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `src/url.c` + `url.h` | `[net]::url` | ported | parse lenient + résolution relative (batterie RFC-3986), hostname/port lazy, encodage chars illégaux, heuristique `http:`, public-suffix/TLD, `cmp` ; switch HSTS isolé dans `switch_to_https` (branchement prefs/hsts reporté à leur port) |
| `src/dns.c` + `dns.h` | `[net]::dns` | ported | cache + dédup insensible casse, file FIFO `-2`, `NUM_DNS_SERVERS`=4 canaux ; threads → `spawn_blocking`, callbacks chain-aware main-thread via `dns_resolve`/`poll_dns` (`DnsCallback` reçoit `&mut Chain`) |
| `src/decode.c` + `decode.h` | `[net]::decode` | ported | chunked (strtol hex non-borné, troncature `long`→`int`, quirks whitespace/`0x`), gzip (en-tête RFC1952 + CRC/ISIZE vérifiés à la main sur `flate2` rust_backend), deflate→raw fallback sur `Z_DATA_ERROR`, brotli (crate `brotli`), charset via `encoding_rs` (labels WHATWG — couvre iconv courant ; skip 1 octet/EILSEQ, pas WHATWG-maximal) |
| `src/chain.c` + `chain.h` | `[net]::chain` | ported | CCC verbatim : arène `LinkId` (ADR-0005), FCB/BCB/BCFB, flags `Ended`/`Aborted`, `LocalKey`→`Any`, `void* Data1/2`→`ChainData` typé (`Buf`/`Int`/`Str`/`Handle`/`Owned` Rc) |
| `src/cache.c` + `cache.h` | `[net]::cache` | ported | entrées triées (`url_cmp`), file clients avec clés `ClientKey`, `process_dbuf` verbatim (accumulation header jusqu'à `\n\n`, strip `\r`/NUL, unfold quirk = `\n` retiré mais espace de continuation gardé, `100 Continue` repart à zéro), `parse_header` complet (30x+Location→REDIRECT/FORCE/TEMP avec blocage domainrc+dpi, 401→auth challenges, Set-Cookie→`cookies_set` via `DpiSend` injecté, HSTS, Content-Length→GOT_LENGTH+expected, `Connection:`→KEEP_ALIVE (HTTP/1.0 le retire), Content-Type hdr/meta/norm/det, Content-Disposition→dtype), chaîne décodeurs transfer→content→charset→`UTF8Data` lazy (refcount), `process_queue` verbatim (dispatch `web::dispatch_by_type`, `AbortEntry`+`OfferDownload` si pas de viewer/mismatch/huge, `savelink` différé, redirect→`nav.push` avec `redirect_level`, page « blocked redirection » forcée en `text/html`, auth→`auth_entry` via timeout 0), delayed queue ≙ `a_Timeout_add(0)`, `about:cache`/`about:dicache` injectés `CA_InternalUrl` jamais retirés, hits/refcount verbatim ; seams `BrowserOps`/`UiOps`/`NavOps`/`DicacheOps`/`WebDispatch` injectées (deps descendantes) |
| `src/capi.c` + `capi.h` | `[net]::capi` | ported | `capi_conn_t`→`Rc<RefCell<CapiConn>>` ≙ `Ref`+`CapiConns` (validité par présence), `conn_abort_by_url` (ABORTED + abort différé `pending_abort` si CCC en vol — ≙ synchrone amont, `RefCell` impose le report), CCC 4 branches verbatim : 1B→`http_ccb`/`dpi_ccc`, 1F `"FD"`→`sock_fd`+relais 2B, `"DpidOK"`→`conn_resume` (rejoue `datastr` des `PENDING`), `"DpidERROR"`→`set_msg`, `"Both"`→abort 2B ; 2F `"send_page_2eof"`→`process_dbuf(IO_READ)`+`"reply_complete"` si done&valid, `"send_status_message"`/`"chat"`/`"dialog"`/`"reload_request"`→seams, `IO_CLOSE`/`IO_ABORT`→`process_dbuf` ; `open_url` dispatch : `WEB_Download` (cache→fichier sinon downloads-dpi), `url_uses_dpi` verbatim (`?` si pas de `/`, `bm`→`bookmarks`, `proto.<scheme>`), `dpi_verify_request` (GET|POST dpi seulement depuis page dpi, `/vsource/` GET excepté, `stack_size`+`top_url`≙`NAV_TOP_UIDX`), `request_permitted` (mixed-content https→!https sauf `data:`, domainrc sauf cache-hit), http→2 branches CCC (recv avant send — course DNS amont) ; seams `DpiOps`/`UiOps`/`NavOps`/`dpi_ccc` injectées (phase DPI) |
| `src/dicache.c` + `dicache.h` | `[img]::dicache` | ported | `CachedIMGs` triée (url_cmp,version), versions + `DIF_Last`/`DIF_Valid`, `RefCount`≙clients cache (pas l'`Rc`), `surv_cleanup`≙3, `dicache_size_total` invariant, `set_parms/set_cmap` (`bg_index`→bg, `3*num_colors_max` zéroé), `new_scan`/`write` (bitvec de lignes), `close`/`abort`, `a_Dicache_callback` verbatim (décodeur take/appel/restore hors emprunts — anti-réentrance `RefCell`, propagation `BitVec`+`ScanNumber` vers `DilloImage` multi-scan), `dicache_image` (factory seam `ImageFactory`≙`a_Image_new_with_dw`, decoders via `DecoderFactory`≙`a_Xxx_new`), `cleanup`/`freeall`/`invalidate_entry`, `about:dicache` verbatim (`dStr_shorten` milieu) ; seam `cache::DicacheOps` implémentée (`DicacheOpsImpl`) |
| `src/web.cc` + `web.hh` | `[net]::web` | ported | `a_Web_dispatch_by_type` : root→seam `WebDispatch` injectée, non-root→uniquement `image/*` (filtre **mime**, pas `WEB_Image` — quirk amont vérifié) ; `ValidWebs` ≙ `HttpWeb::valid` (`Cell` partagé `Rc`), `web_free` ≙ `invalidate`+`a_Bw_remove_client` (au dequeue client cache) |
| `src/nav.c` + `nav.h` | `[browser]::nav` | ported | `Nav` = champs `bw->nav_*`/`meta_refresh_*` amont (futur champ de `BrowserWindow`, `bw: u32` = id) ; pile verbatim (`stack_move_ptr` bornes muettes, truncate/append/get+save_scroll_pos/`stack_clean` dédup adjacent), `open_url` verbatim (sauve scroll sauf `IgnoreScroll`, `MustLoad` = force|url≠|frag≠, `stop_clients(BW_Root|BW_Img)`+cleanup, `web_new`+`WEB_RootUrl`+`capi_open_url`+`add_client`+`add_url`), `expect_done` verbatim (ops pile **uniquement si `expecting`** — back/forw n'empilent pas : `expect` n'est appelé que par push/reload/redirection0 — quirk amont), masque flags avant `history_add_url`, goto_old_scroll vs fragment vs `0,0`, repush (`ReloadFromCache`)/reload (`ReloadPage|E2EQuery`, POST→`dialog_choice` sauf `dpi:/vsource/`, retire `SpamSafe`)/redirection0 (`meta_refresh_url`+status 2) différés via `NavSched` ≙ `a_Timeout_add(0)`, back/forw (`_nt`→seam UI), jump, home, `Nav_save_cb` (`web->stream`/`SavedBytes`≙`Cell`+`RefCell<File>` sur `HttpWeb`, msg « File saved » via `cache_ui_msg`) + wrappers `nav_get_buf`/`unref_buf`/`get_content_type`/`set_vsource_url` ; seams `NavBw`/`NavUi`/`NavSched` injectées |
| `src/history.c` + `history.h` | `[net]::history` | ported | `History` en `thread_local` ≙ tableau global ; `add_url` dédup `url_cmp`+fragment → `idx`, `get_title(_by_url)` avec fallback `URL_STR` (`force`), `set_title_by_url` scan inverse + `MSG_ERR` ; `dReturn` → `Option` |
| `src/auth.c` + `auth.h` | `[net]::auth` | ported | parse challenge verbatim (`Auth_parse_token_value` : le cb-0 n'interrompt que le token en cours — `nonce` suivant lu quand même ; `stale`/`algorithm` non quotés ; `realm` ci, autres tokens cs ; offset `Digest `=7/`Basic `=6), arbre `AuthHost`(scheme+authority ci)→`AuthRealm` (chemins complets `URL_PATH`, `path_is_inside` = préfixe+frontière `/`, longest-path), `stale`→nonce sans dialog, dialog injecté `AuthDialog` ≙ `a_Dialog_user_password`, Digest essayé avant Basic, `dStrshred` password |
| `src/digest.c` + `digest.h` | `[net]::digest` | ported | MD5 via crate `md-5` (≙ `md5.c` bundled), cnonce 32-hex (`fastrand` ≙ `rand()%16`), A1/A2/response verbatim, ordre champs IE (`username,realm,nonce,uri,cnonce,nc,algorithm,response,opaque,qop`), `opaque` `Option` (NULL-vs-"" amont), `nc=%08x`, POST selon `URL_Post`, MD5-sess/auth-int calculés mais jamais activés amont |
| `src/md5.c` + `md5.h` | — | won't port | crate `md-5` |
| `src/cookies.c` + `cookies.h` | `[net]::cookies` | ported | client du serveur `cookies.dpi` (stockage = plugin, phase DPI) : parseur `cookiesrc` verbatim (`DEFAULT DENY|ACCEPT|ACCEPT_SESSION`, longest-domain-first, `.dom`→suffixe sans apex — différence `domain.c`), DENY→drop, requester cross-org non-racine→`""`, canal `DpiSend` injecté ≙ `a_Dpi_send_blocking_cmd`, disabled si pas de contenu |
| `src/domain.c` + `domain.h` | `[net]::domain` | ported | `Domain` en `thread_local` ≙ globales ; `strtok` tokens, texte surnuméraire = warn mais règle ajoutée (quirk), `Domain_match` verbatim (`*`, exact ci, `.suffixe` avec `diff==-1`→compare sans le point) ; `permit` : short-circuit vide, `data:`/host vide, `same_organization`, match inverse `default_deny` |
| `src/hsts.c` + `hsts.h` | `[net]::hsts` | ported | liste triée ci (insert-sorted ≙ `dList`), `future_time` saturé ≙ `tm_sec=INT_MAX`+mktime (2038) ; `hsts_set` verbatim : `parse_attr`/`parse_value`/`eat_value`, `max-age` digit+`strtol`+ERANGE→INT_MAX, `max-age=0`→remove, `preload` ignoré ; `require_https` exact + marche parents `strchr(h+1,'.')` + `subdomains`, expiré→retiré ; `cert_clean`/`enabled`/preload injectés (TlsEngine + prefs non globaux) ; `switch_to_https` amont = appelant (url.c:441) |
| `src/bookmark.c` + `bookmark.h` | `[browser]::bookmark` | ported | client chat dpi `bookmarks` : `Bookmarks` ≙ statiques `cmd1..4`+`bw` (lazy `build_cmd`), machine verbatim (init→`Hi server`, `H`→`I want to set…`, `I`→`Sure it is!`, `O`→cmd4+reset), `add` = `history_get_title_by_url(url,1)` + `cmd=add_bookmark url=… title=…`, `server_said` = cible `DpiOps::chat_add` (le format fichier bm vit dans `dpi/bookmarks.*` — phase DPI) |
| `src/bw.c` + `bw.h` | `[browser]::bw` | not started | BrowserWindow glue |
| `src/actions.c` + `actions.h` | `[browser]::actions` | ported | `label:cmd` `strtok`-verbatim (`:`→label, `""`→reste — le 2e `:` reste dans cmd), listes prefs `link_actions`/`page_actions`, `actions_run` ≙ fork+`system` enfant + `DILLO_PID` → `Command` shell détaché sans wait |
| `src/bitvec.c` + `bitvec.h` | `[common]::bitvec` | ported | quirk verbatim : `BVEC_SIZE=sizeof(uchar_t)=1` → 1 octet/bit, seul bit 0 utilisé (`vec[pos] & 1`), `new` alloue `num_bits+1`, bornes muettes ≙ `dReturn_if_fail` |
| `src/klist.c` + `klist.h` | `[common]::klist` | ported | `Klist<T>` : clés `i32` compteur (1re=1), wrap **complet** seulement à `0` → `Counter=1,Clean=0` (clés négatives émises entre INT_MAX→0 — quirk amont), saute clés vivantes tant `!Clean`, liste triée ≙ `dList_insert_sorted`, vide→`Clean=1` |
| `src/misc.c` + `misc.h` | `[common]::misc` | ported | escape/tabs/sniff Content-Type/parse type+cmp+check/geometry/search_url/base64/disposition/file2dstr ; quirks amont documentés (check -1 inatteignable, sniff UTF-8 écrase binaire ASCII) |
| `src/colors.c` + `colors.h` | `[common]::colors` | ported | table 147 couleurs verbatim (EXTENDED), parse `#`/`0x`/nom/hex nu + err 0/1/2, `color_vc` scores verbatim |
| `src/prefs.c` + `prefs.h` | `[browser]::prefs` | ported | `Prefs` (pas de global — owned par le navigateur), défauts `a_Prefs_init` verbatim, UA `Dillo/3.3.0-rc1` |
| `src/prefsparser.cc` + `.hh` | `[browser]::prefsparser` | ported | fusionné dans `prefs::parse_dillorc`/`parse_option` — symbol table → match, STRINGS vident le défaut à la 1re occurrence, types BOOL/COLOR/STRING/STRINGS/URL/INT32/DOUBLE/FRACTION_100/GEOMETRY/PANEL_SIZE |
| `src/paths.cc` + `paths.hh` | `[browser]::paths` | not started | chemins ~/.dillo → dirs |
| `src/dpiapi.c` + `dpiapi.h` | `[browser]::dpiapi` | ported | `Dpiapi` ≙ static `dialog_server` ; `dialog` = `get_attr` title/msg/alt1..5 → seam `DialogChoice` (modal → réponse immédiate) → `answer_cb` renvoie `cmd=answer to_cmd=dialog msg=N` via `capi_dpi_send_cmd` ; cible `DpiOps::dialog` côté capi |
| `src/timeout.cc` + `timeout.hh` | `[net]::timeout` | ported | file thread-local ≙ `Fl::add_timeout` : `timeout_add`/`timeout_repeat` one-shot, `poll_timeouts` drainé depuis le thread principal (réentrants — callback peut réarmer), `a_Timeout_remove` no-op amont (« WTF? FIXME » quirk porté), `actually_remove` par id ≙ `{cb,cbdata}` |
| `src/version.cc` + `version.hh` | `[bin]::version` | not started | |

## src/ — UI shell (chrome)

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `src/ui.cc` + `ui.hh` | `[browser]::ui` | not started | chrome/toolbars/tabs (ADR-0002) |
| `src/uicmd.cc` + `uicmd.hh` | `[browser]::uicmd` | not started | commandes UI |
| `src/keys.cc` + `keys.hh` + `keysrc` | `[browser]::keys` | not started | key bindings + keysrc file |
| `src/menu.cc` + `menu.hh` | `[browser]::menu` | not started | menus contextuels |
| `src/dialog.cc` + `dialog.hh` | `[browser]::dialog` | not started | dialogues (auth, prefs) |
| `src/findbar.cc` + `findbar.hh` | `[browser]::findbar` | not started | |
| `src/tipwin.cc` + `tipwin.hh` | `[browser]::tipwin` | not started | tooltips |
| `src/xembed.cc` + `xembed.hh` | — | won't port | embedding X11 spécifique |
| `src/dillo.cc` | `[bin]` main | not started | entry point |
| `src/dilloc.c` | `[bin]::dilloc` | not started | utilitaire message |
| `src/binaryconst.h`, `dgif.h`, `djpeg.h`, `dpng.h`, `dwebp.h`, `dsvg.h`, `pixmaps.h`, `msg.h`, `list.h`, `domainrc`, `keysrc`, `dillorc` | données/config | not started | headers/data → intégrés aux modules |

## src/ — images

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `src/image.cc` + `image.hh` | `[img]::image` | ported | `DilloImage` (layout≙handle opaque, `img_rndr`→trait `ImgRenderer`≙`I2IR` set_buffer/draw_row/finish/fatal), états `IMG_*`, `bitvec`+`scan_number`, `image_ref/unref`≙`Rc`, `set_parms` (params morts url/version/type amont ignorés — verbatim), `write`/`close`/`abort` ; `web->Image`≙`HttpWeb::image` (`Any`, détaché des emprunts) |
| `src/imgbuf.cc` + `imgbuf.hh` | `[img]::imgbuf` | ported | `Imgbuf` buffer RGB `3*w*h` init **222** amont, `Rc`≙refcount+`deleteOnUnref`, cascade clamps verbatim (area>6000²→1×1, w/h>0x8000, gamma≤0→1), `rgb_line` verbatim (INDEXED/GRAY/RGBW `*white/0x100`/RGB, NOTSET→noir), arbre scaled-buffers ≙`fltkimgbuf` : `copiedRows` par buffer, `get_scaled_buf` formules de map verbatim (root↔scaled int), `newScan`/`copyRow` propagent aux fils déjà créés (lignes déjà copiées), `getRowArea`/`copyTo` (clip + blit), `setCMap` vide ≙`FltkImgbuf` RGB-only ; impl `dillo-dw::imgbuf::Imgbuf` (dw-1b ; receveurs `&self` mutabilité intérieure ≙ méthodes via pointeurs partagés amont, dw-1c) ; `scaleMode` amont `BEAUTIFUL_GAMMA`→`SIMPLE` ici (**déviation** : pixels non gamma-corrigés, `BEAUTIFUL` en phase platform) |
| `src/gif.c` | `[img]::gif` | ported | LZW maison porté verbatim : machine d'états push (sig→LSD→blocks/ext→LZW→`state=999`), `Gif_sequence` complet (traversée arrière, spill lines multi-lignes, quirk `orig_code==last_code`/`last_byte_ptr`), entrelacé 4 passes avec quirk `pass>3` sans incrément, GCE transparence, cmap global/local + `ColorMap_ofs`, `emit_line`→`dicache_write`, `IMAGE_MAX_AREA` ; tests : GIF 1×1 réel end-to-end incrémental, mauvaise signature, dimensions suspectes |
| `src/png.c` | `[img]::png` | ported | glue ≙ libpng progressif → crate `png` (décode complet à la 1re tentative réussie — `IoError(UnexpectedEof)` ≙ suspend) : sig check 8 octets verbatim, `EXPAND|STRIP_16`≙`png_set_expand`/`strip_16`, gris→RGB manuel ≙ `png_set_gray_to_rgb`, composite alpha `png_composite` verbatim (`+128`,`t>>8`), LUT gamma ≙ `png_set_gamma(2.2,file_gamma)` si gAMA, `file_gamma=1/2.2` sinon ; **déviations documentées** : pas d'affichage progressif par ligne, Adam7 une écriture/ligne (pas de `new_scan`), gamma 8-bit vs 16 amont |
| `src/jpeg.c` | `[img]::jpeg` | ported | glue ≙ libjpeg source suspendable → `jpeg-decoder` (decode-at-EOI `FF D9`, tentative finale à `CA_Close`) : `num_components`→GRAY/RGB/`CMYK_INV` (imgbuf `rgb_line` quirk `/0x100` préservé), `IMAGE_MAX_AREA`, gamma fixe `1/2.2` (« \todo Gamma » amont) ; **déviations documentées** : pas de scanline progressif, multi-scan `buffered_image`/`new_scan` non reproduit, JPEG sans EOI amont affiche le partiel → ici tentative decode final |
| `src/webp.c` | `[img]::webp` | ported | glue ≙ `WebPINewDecoder`/`WebPIUpdate` → `image-webp` : `WebPGetFeatures`≙parse header (échec→attente `IS_init`), RIFF `len>=8+riff_size` ≙ `SUSPENDED|OK`, `has_alpha`→RGBA→composite verbatim `(c*a+bg*(255-a))/0xFF` (÷255 tronqué — PAS `png_composite`), `y`≙`webp->y` anti-réémission, gamma `1/2.2` ; **déviation** : pas d'émission progressive `last_y` |
| `src/svg.c` | `[img]::svg` | ported | glue ≙ nanosvg → `resvg`/`usvg` : `strstr("</svg>")` verbatim, `usvg::Options.dpi`≙`Image->dpi`, taille tronquée ≙ cast C `unsigned`, `IMAGE_MAX_AREA`, render identité ≙ `nsvgRasterizeXY(0,0,1,1)`, RGBA premultiplied→straight→composite `(c*a+bg*(255-a))/0xFF` verbatim ; **déviations** : rasteriseur ≠ nanosvg (pixels non bit-identiques), `currentColor`=`fg_color` non injectable dans usvg |
| `src/nanosvg.h`, `nanosvgrast.h` | — | won't port | remplacé par resvg/usvg (pur Rust) |

## dw/ — moteur de layout (cœur)

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dw/core.hh` | `[dw]::core` | not started | namespaces/traits centraux |
| `dw/types.cc` + `.hh` | `[dw]::types` | ported | Rect/Circle/Polygon/Region, Alloc/Req/Extremes, Content, stacking ctx ; `Shape::draw` via seam `View` ; quirks verbatim (borne poly `y>=maxy`, merge ≤16) |
| `dw/style.cc` + `.hh` | `[dw]::style` | ported | enums, `Length` verbatim, `Box`, `StyleAttrs`, `Style`+table tl, `Font`/`Tooltip`, `numtostr`, `Color`/`Shading`, `StyleImage`+`ImgRendererDist`+`StyleImgRenderer` (buffer tuilé `MIN/OPT_BG_IMG_*` verbatim), `External(Widget)ImgRenderer` traits + helpers, `drawBorder{Top,Bottom,Left,Right}` trapezes verbatim (solid/dotted/dashed/inset/outset/ridge/groove/double), `drawBackground`/`drawBackgroundImage`/`calcBackgroundRelatedValues` (repeat X/Y, positions %/abs, `drawBackgroundLineByLine=false` porté verbatim) |
| `dw/events.hh` | `[dw]::events` | ported | ButtonState + Mouse/Button/Motion/Crossing (`Widget*`→`Option<WidgetId>`) + `ButtonEventType` |
| `dw/tools.cc` + `.hh` | `[dw]::tools` | ported | SizeParams verbatim (refs→`WidgetId`) |
| `dw/platform.hh` | `[dw]::platform` (trait) | ported | trait `Platform` verbatim (attach/detach, textWidth/case/glyph, DPI, idle, font/color/tooltip/imgbuf factories, selection) — impl `[plat]` en phase platform |
| `dw/view.hh` | `[dw]::view` (trait) | ported | trait `View` complet verbatim (set_layout/canvas/cursor/bg, scrolling, start/finish/queue_draw, draw_point/line/typed/rect/arc/polygon/text/wrapped/image, clipping views) — `Layout*`→handle `usize` (modèle D) |
| `dw/widget.cc` + `.hh` | `[dw]::widget` + `[dw]::layout` | ported | nœud arène `WidgetNode`+`WidgetKind` (modèle D), flags verbatim, parent/quasi/container/generator, hiérarchie (top_level/level/nearest_ancestor), `intersects`, queueDraw/queueResize(Fast)+mark*/containerSizeChanged verbatim, `sizeRequest`/`getExtremes`+cache `SizeParams` (intrinsic=-1 fallback verbatim), `calcExtraSpace`, `sizeAllocate`(+`queueDrawExcept` ancienne/nouvelle zone), `getMinWidth`/`getAvailWidth`/`getAvailHeight` (viewport/parent/quasi), `calcWidth`/`calcFinalWidth`/`calcHeight`/`adjustHeight`, `correctRequisition(Viewport)`/`correctExtremes`/`correctReqAspectRatio` 3 passes verbatim, defaults `OfChild` (effContainer = quasiParent→container→parent), applyPerWidth/Height, `setStyle`/`setBgColor`/`getBgColor`/`getFgColor` remonte ancêtres, `drawBox`/`drawWidgetBox`/`drawInterruption`, button/enter/leave dispatch (`x_tooltip`), `WidgetImgRenderer`+`~Widget`+`container_add_child` seam ; `numSizeRequestReferences`/getExtremesReference + `iterator`/`stackingContextMgr` |
| `dw/layout.cc` + `.hh` | `[dw]::layout` | ported | verbatim : attach/detachView (projection état + `setLayout` en dernier), addWidget/setWidget/removeWidget/detachWidget, queueDraw(+isEmpty)/queueDrawExcept(4 rects), queueResize→idle+`cancelQueueDraw`+`emitResizeQueued`, resizeIdle verbatim (QUEUED→NEEDS, boucle `resizeIdleId!=-1`, limites 100/1000, canvasHeightGreater+containerSizeChanged, scrollbarOnLeft), scrollTo0/scrollIdle/adjustScrollPos/calcScrollInto/scrollPosChanged/viewportSizeChanged verbatim, draw (bg image puis toplevel, defer drawAfterScrollReq), anchors (add/change/remove/updateAnchor), setCursor/updateCursor, setBgColor/setBgImage+`LayoutImgRenderer` (état sync — `layout*` amont → `Weak` arène, modèle D), events button/motion/enter/leave/getWidgetAtPoint/moveToWidget(piste verbatim)/processMouseEvent(clamp visible+link fallback), emitLink*/connectLink, `drain_idles` seam (≙ toolkit amont), `textZone` alloc non porté (sans effet), `use` délègue findtext/selection |
| `dw/iterator.cc` + `.hh` | `[dw]::iterator` | ported | trait `DwIterator` ≙ classe `Iterator` (`Widget*`→`WidgetId`, accès arène par `&Layout`/`&mut`), `EmptyIterator`/`TextIterator` verbatim + `ChildIterator` seam (≙ contenu `WIDGET_IN_FLOW` des conteneurs — kinds concrets = itérateurs dédiés), `WidgetKind::Text` seam de test (usage `TextIterator` anticipé mais commenté amont), `iterator_scroll_to` verbatim (rect englobant, clamp viewport sur saut de ligne), `DeepIterator` (pile niveau-indexée ≙ `put(it,level-1)`, `searchDownward`/`searchSideward`/`getRespectiveParent` verbatim, `next`/`prev` descente/remontée verbatim), `CharIterator` verbatim (BREAK→'\n', `space`→' ', NUL≙hors-slice) ; dispatch `Layout::w_iterator` |
| `dw/ui.cc` + `.hh` | `[dw]::ui` | not started | ResourceFactory/Embed widgets |
| `dw/textblock.cc` + `.hh` | `[dw]::textblock::{mod,words,breaking,sizing,draw,iter}` | ported | verbatim : modèle Word/Line/Paragraph/BadnessAndPenalty + Anchor + DivChar + renderers Word/SpaceImgRenderer (ExternalWidgetImgRenderer ≙ trait), ctor + sendSelectionEvent (hit-test mot/caractère + itérateur réel), addText*/addSpace/addParbreak(propagation ancêtre verbatim)/addLinebreak/addWidget/addAnchor/fillWord/fillSpace/setBreakOption/calcTextSize*/calcSizeOfWidgetInFlow/findSizeRequestReference, sizeRequestImpl/getExtremesSimpl/sizeAllocateImpl/calcExtraSpaceImpl + getAvail*OfChild + mark*/resizeDrawImpl/oofSizeChanged/widgetRefSizeChanged/getGenerator* + find*/yOffset*/calcVerticalBorder/getWordExtremes/queueDrawRange/updateReference + decorateText/drawText/drawWord*/drawSpace/drawLine/drawLevel/getWidgetAtPointLevel (SL_OOF_REF verbatim-mort IMPL_POS=false), hoverTooltip statique verbatim, handOverBreak/flush/changeLinkColor/changeWordStyle/removeChild (no-op verbatim), ~Textblock→cleanupWord |
| `dw/textblock_linebreaking.cc` | `[dw]::textblock::breaking` | ported | verbatim : wordWrap/addLine/wordWrapWidgetAndHyphenate/addLine(wordWidget...)/splitWord/processWord/hyphenateWord(TeX patterns)/calcPenaltyIndex(DIV_CHARS verbatim)/addOptionLinebreaks/justification cumulative (rounding-errors doc)/alignLine/mustBorderBeRegarded |
| `dw/textblock_iterator.cc` | `[dw]::textblock::iter` (`TbInFlow` + `OofAwareIterator`) | ported | verbatim : content words/getWidget/getAnchorInfo/getBookmark, positions/next/prev, highlight/unhighlight (DEBUT/FIN mot + linesOfWord + firstWordOfLine + END_OF_WORD), getAllocation (lignes non allouées = valeurs comme addLine verbatim), findLineIndex(WhenNotAllocated), scrollInto |
| `dw/alignedtextblock.cc` + `.hh` | `[dw]::textblock::listitem` | ported | verbatim : List partagée Rc≙refcount, updateValue→setMaxValue sur tous les membres, leftInnerPadding/line1Offset, unref→slot NULL |
| `dw/hyphenator.cc` + `.hh` | `[dw]::hyphenator` | ported | verbatim : Trie + loaders .pat/.exc/.trie (insertWord verbatim, priorités chiffres, odd/even), hyphenateWord pour textblock ; chemins via DILLO_LIBDIR |
| `dw/ruler.cc` + `.hh` | `[dw]::ruler` | ported | verbatim : block-level/usesAvailWidth, sizeRequest=availWidth-boxDiff, ascent=boxOffsetY, draw→drawWidgetBox (bypass OOFAware), getWidgetAtPoint=inAllocation (bypass), EmptyIterator |
| `dw/bullet.cc` + `.hh` | `[dw]::bullet` | ported | verbatim : sizeRequest=(xHeight*4/5, xHeight), extremes identiques, draw Disc/Circle→arc(filled/open) Square→rect (centre+rayon=min(w,ascent)/2), EmptyIterator |
| `dw/listitem.cc` + `.hh` | `[dw]::textblock::listitem` | ported | verbatim : usesMaxGeneratorWidth, initWithText→hasListitemValue+updateValue, ctor setRefTextblock→List partagée |
| `dw/findtext.cc` + `.hh` | `[dw]::findtext` | ported | verbatim : KMP next-table, match case-sens/insens + whitespace-equiv (insens), fwd/back, resetSearch+unhighlight, wrap-around SUCCESS/RESTART/NOT_FOUND |
| `dw/selection.cc` + `.hh` | `[dw]::selection` | ported | verbatim : reset/resetLink, buttonPress/Release/Motion, linkButton/activateLink/switchLinkToSelection(clone+take), adjustSelection, correctCharPos+END_OF_WORD (1<<30), highlight/unhighlight incrémental from/to, copy→clipboard platform |
| `dw/image.cc` + `image.hh` | `[dw]::image` | not started | widget image |
| `dw/imgbuf.hh` | `[dw]::imgbuf` trait | ported | trait `Imgbuf` (`set_cmap`/`copy_row`/`new_scan`/`get_scaled_buf(Rc<Self>)`/`get_row_area`/`root_w/h`/`similar_buf`/`copy_to`/`ref/unref`/`last_reference`/`set_delete_on_unref`/`width/height`) — impl concrète `[img]::imgbuf` (dw-1b aligné) |
| `dw/imgrenderer.cc` + `.hh` | `[dw]::imgrenderer` | ported | trait `ImgRenderer` + `ImgRendererDist` (snapshot Rc) — impls dw (`StyleImgRenderer`…) à venir |
| `dw/table.cc` + `table.hh` | `[dw]::table` | not started | algo table maison |
| `dw/tablecell.cc` + `tablecell.hh` | `[dw]::table` | not started | |
| `dw/simpletablecell.cc` + `.hh` | `[dw]::table` | not started | |
| `dw/alignedtablecell.cc` + `.hh` | `[dw]::table` | not started | |
| `dw/table_iterator.cc` | `[dw]::table` | not started | |
| `dw/outofflowmgr.cc` + `.hh` | `[dw]::oof` | ported | trait `OofMgr` (getSize/getExtremes/addWidgetOOF/sizeAllocate*/tellPosition/markSizeChange/markExtremesChange/getLeftOOFBorder/getRightOOFBorder/hasClearLeft/Right/moveExternalIndices/getClearPosition/sizeAllocate*/draw/getWidgetAtPoint/containerMustAdjustExtraSpace/containerSizeChangedForChildren/doWidgetsOverlap), index OOFM_* + `OOFM_NAME` verbatim |
| `dw/oofawarewidget.cc` + `.hh` + `oofawarewidget_iterator.cc` | `[dw]::oof` | ported | `OofAwareData` (oofContainer/outOfFlowMgr dans le kind), `getOOFContainer`/`getOOFContainerParent`/`getOOFMIndex`/`getWidgetOOF`/`getWidgetOOFInFlow`/`notifySetParent`/`notifySetAsTopLevel`/`initOutOfFlowMgrs`/`addWidgetOOF`/`addWidgetInFlow`/`ensureOOFReferences`/`createOOFWidgetRef`/`markSizeChange(Extremes)`/`containerSizeChangedForChildren`/`sizeAllocateImpl`/`correctRequisitionByOOF`/`correctExtremesByOOF`/`drawOOF`/`getWidgetOOFAtPoint`/`doesWidgetOOFInterruptDrawing`/`oofSizeChanged`/`moveExternalIndices` verbatim + `OofAwareIterator` (sections IN_FLOW→FLOATS→ABS→REL→FIXED, parentRef encodé subRef/oofmIndex, compare cross-mgr verbatim) ; `~OOFAwareWidget` ≙ `widget_delete` (destruction enfants OOF) |
| `dw/ooffloatsmgr.cc` + `.hh` | `[dw]::oof::floats` | ported | `OofFloatsMgr` : floats triés left/right (bsearch GB compare verbatim, `findFloatIndex`/`findInList` dichotomie), `Float` arena (realSize/realCBAllocation/generators/dirty/covers/accumulateHeight), `t1` initial + `updateGenerators`/`createGenerator`, `tellPosition` (side lists, `ensureFloatSize`/`isAllocated`, clamp largeur verbatim), `collides`/`getSide`/`getCombinedPosition`, `getBorder`/`getGB`/`getClearPosition`, `getSize`/`getExtremes`/`addWidgetOOF`/`mark*`/`moveExternalIndices`/`getFloatsAtPoint`/`sizeAllocate*` verbatim |
| `dw/oofpositionedmgr.cc` + `.hh` | `[dw]::oof` | ported | `IMPL_POS=false` amont : instancié vide et **inaccessible** (`getOOFMIndex`≡FLOATS ou assert) → `None` dans `out_of_flow_mgr` (consommateurs NULL-check amont → identique observablement) ; vrai port si `IMPL_POS` un jour |
| `dw/oofposabsmgr.cc` + `.hh` | `[dw]::oof` | ported | idem — `None` (mort amont, `IMPL_POS=false`) |
| `dw/oofposabslikemgr.cc` + `.hh` | `[dw]::oof` | ported | idem — `None` |
| `dw/oofposrelmgr.cc` + `.hh` | `[dw]::oof` | ported | idem — `None` |
| `dw/oofposfixedmgr.cc` + `.hh` | `[dw]::oof` | ported | idem — `None` |
| `dw/stackingcontextmgr.cc` + `.hh` | `[dw]::stacking` | not started | z-order — seams `SC_BOTTOM`/`SC_TOP` posées dans `oof.rs` |
| `dw/regardingborder.cc` + `.hh` | `[dw]::oof` | ported | tag interface vide amont (ctor/dtor debug only) → `is_regarding_border_kind` |
| `dw/tools.cc` + `.hh` | `[dw]::tools` | not started | misc draw helpers |
| `dw/tools.cc` + `.hh` | `[dw]::tools` | not started | misc draw helpers |

## dw/ — couche FLTK (remplacée, ADR-0002)

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dw/fltkplatform.cc` + `.hh` + `fltkcore.hh` | `[plat]::platform` | not started | spec du trait — impl winit/skia |
| `dw/fltkviewbase.cc` + `.hh` | `[plat]::view` | not started | |
| `dw/fltkviewport.cc` + `.hh` | `[plat]::viewport` | not started | scroll/damage |
| `dw/fltkflatview.cc` + `.hh` | `[plat]::flatview` | not started | |
| `dw/fltkui.cc` + `.hh` | `[plat]::ui` | not started | ResourceFactory FLTK |
| `dw/fltkcomplexbutton.cc` + `.hh` | `[plat]::button` | not started | |
| `dw/fltkimgbuf.cc` + `.hh` | `[plat]::imgbuf` | not started | pixbuf adapter |
| `dw/fltkmisc.cc` + `.hh` | `[plat]::misc` | not started | |
| `dw/fltkpreview.cc` + `.hh` | `[plat]::preview` | not started | print preview — peut être différé |
| `dw/preview.xbm` | — | won't port | asset FLTK |

## dpi/ — plugins

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dpi/dpiutil.c` + `.h` | `[dpi]::util` | not started | escaping/filtering |
| `dpi/bookmarks.c` | `bin dpi-bookmarks` | not started | |
| `dpi/cookies.c` | `bin dpi-cookies` | not started | |
| `dpi/datauri.c` | `bin dpi-datauri` | not started | |
| `dpi/downloads.cc` | `bin dpi-downloads` | not started | |
| `dpi/file.c` | `bin dpi-file` | not started | |
| `dpi/ftp.c` | `bin dpi-ftp` | not started | |
| `dpi/hello.c` | `bin dpi-hello` | not started | plugin test |
| `dpi/vsource.c` | `bin dpi-vsource` | not started | view-source |

## dpid/ — démon

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dpid/main.c` | `bin dillo-dpid` | not started | |
| `dpid/dpid.c` + `dpid.h` | `[dpi]::daemon` | not started | registry/services |
| `dpid/dpid_common.c` + `.h` | `[dpi]::common` | not started | |
| `dpid/dpi.c` + `dpi.h` | `[dpi]::daemon::service` | not started | |
| `dpid/dpi_socket_dir.c` + `.h` | `[dpi]::sockdir` | not started | socket dir / AF_UNIX |
| `dpid/dpidc.c` | `bin dpidc` | not started | client cmdline |
| `dpid/misc_new.c` + `.h` | `[dpi]::misc` | not started | |
| `dpid/dpidrc.in` | config template | not started | |

## dpip/ — protocole

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dpip/dpip.c` + `dpip.h` | `[net]::dpip` + `[dpi]::transport` | in progress | helpers proto portés : `a_Dpip_build_cmd` (`%s`, stuffing `'`→`''`, EOT ` '>'`) + `a_Dpip_get_attr_l` (machine `SEEK_NAME/MATCH_NAME/SKIP_VALUE/SKIP_QUOTE`, destuff, quote de fin non-stuffée suivie d'un espace) ; reste : socket `a_Dpi_send_blocking_cmd` (phase DPI) |

## Non-porté (build/meta)

`Makefile.am`, `configure.ac`, `autogen.sh`, `Doxyfile`,
`dillo.desktop`, `icons/`, `dillo-install-hyphenation`,
`install-dpi-local`, `test/Makefile.am` — `won't port` (build
amont remplacé par Cargo).

## Synthèse

| Statut | Fichiers |
|---|---|
| not started | ~150 lignes de mapping |
| ported | 0 |
| verified | 0 |
| won't port | tls_mbedtls, tls_openssl, md5.c, nanosvg*.h, xembed.cc, preview.xbm, build/meta |

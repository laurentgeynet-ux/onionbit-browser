# Audit A01 — Chaîne I/O, HTTP & décodage transfert

Passe 2 — après portage de `cache.c`, `capi.c`, `control.c`, `dpi.c`,
`hsts.c`, `auth.c`, `cookies.c`, `domain.c` et apply des audits A02/A11
(jusqu'à `99f6d73`). Passe 1 : après `da29394`, D1–D5 résolus
(section Apply conservée en bas). Mode : `rapport`.

Périmètre couvert : `src/IO/{IO.c,IO.h,iowatch.cc,http.c,mime.c,proto.c,
about.c,tls.c,tls_mbedtls.c,dpi.c,control.c}` + `src/{chain.c,decode.c,
dns.c,url.c,cache.c,capi.c,hsts.c}` → `crates/dillo-net/src/{io,chain,
http,mime,about,tls,decode,dns,url,cache,capi,hsts,control,dpi}.rs`
(+ glue `dillo-browser/src/glue.rs`, `dillo/src/main.rs` pour les seams).

Les verdicts `conforme` de la passe 1 ont été **revérifiés** (lecture
croisée à jour) ; cette passe ajoute l'audit des zones qui étaient
`N-A` : redirects, cache lifecycle, keep-alive/`reply_complete`,
control, dpi client.

## Verdicts checklist

- **Framing HTTP** : `conforme` — `make_query_str` reproduit l'ordre
  exact amont (`GET/POST uri HTTP/1.1`, `Host`, `User-Agent`,
  `Accept` par `WEB_*`, `Accept-Language`, `Accept-Encoding`,
  `Auth`, `DNT`, `Proxy-Authorization`, `Referer`, `Connection`,
  `Content-*`, `Pragma`/`Cache-Control`, cookies) — `http.rs:475-587`
  vs `src/IO/http.c:405-470`. Tests : `make_query_str_*_golden`,
  `wire_end_to_end_get` (GET octet-pour-octet sur loopback). L1/L5.
- **Redirects (cache)** : `conforme` — parsing 30x → `Location`
  résolue sur la base, `CA_Redirect`/`CA_ForceRedirect` (301)/
  `CA_TempRedirect` (302,307) verbatim (`cache.rs:779-806` vs
  `cache.c:779-830`) ; `cache_redirect` : garde `SPAM_Safe`,
  `redirect_level >= 5` → `CA_RedirectLoop` + reset, conditions
  `FORCE|TEMP || !Data->len || Data->len < 1024` + `WEB_ROOT_URL`,
  `TEMP` → `E2EQuery` sur la cible, `a_Nav_push` ≙ `nav.push`
  (`cache.rs:1450-1484` vs `cache.c:Cache_redirect`) ;
  `entry_pos_with_redirect` suit ≤3 sauts et stoppe sur
  `CA_RedirectLoop` (`cache.rs:411-430` vs
  `Cache_entry_search_with_redirect`) ; page de blocage domainrc
  servie en `text/html` forcé (`cache.rs:1535-1580`).
  Tests : `redirect_30x_sets_location_and_flags`,
  `redirect_chain_followed_with_limit`,
  `blocked_redirect_keeps_location_and_serves_block_page`. L3/L5.
- **`decode.c` chunked** : `conforme` — `chunk_remaining` inclut le
  CRLF (+2), consommé octet par octet **sans validation** `\r\n`,
  en-tête par `memchr('\n')` + `strtol` lenient (whitespace/sign/
  `0x` tolérés, troncature `long`→`int`, `0`/non-hex → `finished`),
  `leftover` conserve les lignes partielles — `decode.rs:120-169`
  vs `src/decode.c:34-91`. Tests : `chunked_one_shot`,
  `chunked_byte_by_byte`, `chunked_split_body_and_crlf`,
  `chunked_extension_and_hex`, `chunked_garbage_header_finishes`,
  `chunked_leading_ws_and_int_truncation`. L1/L2.
- **gzip/deflate/brotli/charset** : `conforme` — zlib
  `Z_DATA_ERROR` → swap raw-deflate **avec replay de l'input du
  call** (verbatim), gzip mono-membre (pas d'`inflateReset` amont
  comme ici), `EILSEQ` → U+FFFD + skip 1 octet via `encoding_rs`
  (`decode.rs:200-460` vs `decode.c:142-260`). Tests :
  `deflate_raw_fallback`, `gzip_golden_vector`,
  `charset_eilseq_skips_one_byte`, `charset_malformed_tail_no_panic`,
  `charset_partial_sequence_leftover`. L1/L2.
- **Granularité chunks (rendu progressif)** : `conforme` — un seul
  `OpSend` par drain EAGAIN (`IO.c:157-219` ≙ `IoEvent::ReadData`
  par `read_task`), `IOBufLen` 8192 des deux côtés ; `poll_io`
  draine `ReadData` puis `Eof`/`ReadErr` dans l'ordre (data avant
  close, comme amont). L4/L5.
- **DNS** : `déviation D12` — cache ascii-insensible sans TTL,
  `queue_find` dédup in-flight, canal idle → serveur, sinon `-2`
  FIFO, `assign_channels` partage par hostname (`dns.rs:162-321` vs
  `dns.c:370-471`). Un écart de réentrance subsiste (voir D12).
  Tests : `cache_hit_is_immediate`,
  `queue_shares_channel_same_hostname`,
  `waiting_queue_minus2_is_fifo`, `failed_resolution_status`. L3/L4.
- **TLS** : `conforme` (écart assumé) — `TLS_CONNECT_*` +
  `CERT_STATUS_*` verbatim, `DilloVerifier` ≙ `VERIFY_OPTIONAL`,
  `close_by_fd` connecting/established ≙ `a_Tls_close_by_fd`,
  `IoMsg::Swap` ≙ `a_Tls_connection(fd)` — `tls.rs`,
  `http.rs:1490-1584`. Backend rustls ≠ openssl/mbedTLS
  (wire-visible par nature, ADR-0003). Tests :
  `connect_ready_state_machine`, `after_handshake_*`. L2/L5.
- **`chain.c`** : `déviation D10` — `link_new`/`bcb`/`fcb`/`bfcb`/
  flags `Ended`/`Aborted`/`check` portés verbatim
  (`chain.rs:281-361` vs `chain.c:120-220`), mais la garde d'entrée
  `a_Chain_check` n'est appelée que dans `io_ccc`/`http_ccc`
  (`io.rs:775`, `http.rs:1651`) — absente de `capi_ccc_fn`
  (`capi.rs:287`) et `dpi_ccc` (`dpi.rs:573`) alors que l'amont la
  pose sur `a_Capi_ccc` (`capi.c:645`) et `a_Dpi_ccc` (`dpi.c:652`).
- **Backpressure écriture** : `conforme` — `wr_tx` accumule comme
  `dStr_append_l(io->Buf)` (non borné des deux côtés), écriture
  partielle + re-attente EAGAIN, aucun octet perdu — `io.rs:414-423,
  653-680` vs `IO.c:222-262`. L4.
- **Backpressure lecture** : `conforme` — D2 vérifié corrigé :
  `IoEvent` borné à `channel(8)`, `send().await` suspend la tâche →
  fenêtre TCP restaurée (`io.rs`, test `event_channel_is_bounded`).
- **Proxy** : `conforme` — env `http_proxy` > prefs, `no_proxy`
  suffix-match ascii-ci avec quirk token vide, `CONNECT host:443` +
  `Proxy-Authorization`, réponse `dStr` jusqu'à `\r\n\r\n`, statut
  `[9]=='2'` — `http.rs` vs `http.c:1045-1075`. Tests :
  `must_use_proxy_suffix_and_empty_token_quirk`,
  `make_query_str_proxy_absolute_uri`, `get_connect_str_golden`. L5.
- **`about:`/`mime`/dispatch** : `conforme` — `ABOUT_SPLASH` verbatim,
  `MimeRegistry` quirks préservés ; `proto.c` = won't port (stub
  amont). `internal_url` injecte `about:cache`/`about:splash` avec
  `GOT_HEADER|GOT_LENGTH|INTERNAL_URL` (`cache.rs:1006-1009` ≙
  `cache.c:134`); `about:` ouvre le cache sans connexion CCC
  (`capi.rs:1064-1071`).
- **`cache.c` `process_dbuf`** : `conforme` — boucle en-tête
  100-continue (`get_header`/`parse_header` itératifs), pipeline
  `TransferDecoder`→`ContentDecoder`→`Data` (+`CharsetDecoder`→
  `UTF8Data` sur le flot décodé, comme `str`/`dstr2` amont),
  `done` = `GOT_LENGTH && TransferSize >= ExpectedSize` annulé si
  `!KeepAlive` (« let IOClose finish it later »), `IOClose` →
  `finish_msg`, `IOAbort` → `CA_Aborted` + « Premature close » si
  données, sinon retrait des clients (`cache.rs:1329-1447` vs
  `cache.c:1040-1130`). `process_queue` : busy-guard
  « Caught busy », détection type via `get_content_type_from_data`
  une seule fois (`type_mismatch` local ≙ amont), boucle clients
  avec detach/réinsertion ≙ fixups `--i` (`cache.rs:1594-1900`).
  Tests : `dbuf_header_then_body_sends_to_client`,
  `dbuf_split_header_completes_on_second_read`,
  `informational_100_resets_header`, `not_found_sets_flag`,
  `abort_marks_entry_and_removes_empty_clients`,
  `process_queue_serves_client_after_dequeued_sibling`,
  `non_root_type_mismatch_aborts_entry`,
  `set_cookie_scans_past_unauthorized_client`. L2/L3/L5.
- **Keep-alive / `reply_complete` / socket_reuse** : `déviation D8`
  — `2F "send_page_2eof"` → `process_dbuf(IORead)` puis, si
  `finished && conn_valid && InfoRecv`, `bcb(OpSend,"reply_
  complete")` (`capi.rs:609-629` vs `capi.c` branche 2F) ;
  `socket_reuse` transfère le fd à un socket queued compatible
  (même host/port/TLS+proxy), `active_conns` déplacé, `socket_free`
  (retrait `fd_map`) puis `fd_map.insert` + `connect_done(TRUE)`
  dans l'ordre amont (`http.rs:1393-1471` vs `http.c:840-890`) ;
  flags `CA_KeepAlive` : défaut posé à la création, retiré sur
  HTTP/1.0 ou `Connection: close`, reposé sur `keep-alive` 1.0
  (`cache.rs:831-841` vs `cache.c:845-853`). L'ordre interne de
  `socket_free` diverge (D8). Tests : `reuse_compatible_rules`,
  `queue_respects_max_conns`,
  `socket_free_queued_makes_zombie_then_sweep`. L3/L4.
- **`capi.c` connexions/refcount/dispatch** : `conforme` modulo
  D10 — `conn_ref`/`unref` ≙ `CapiConns` + `Ref` (`capi.rs:184-204`),
  branches 1B/2B/1F/2F verbatim (`OpStart` chaîne http_ccb ou
  dpi_ccc, relais `"FD"` 1F→2B, `"DpidOK"` → resume pending,
  `"Both"` propage l'abort, `nav_cancel_expect`, unref+free_link),
  `conn_abort_by_url` stoppe au **premier** match (≙ `break`
  `capi.c:214`), `open_url` : branche réception avant émission
  (course DNS amont), `use_cache` seulement si conn valide
  (`capi.rs:940-1086` vs `capi.c` `a_Capi_open_url`). `stop_client`
  : unique + (`force` || `BufSize==0`) → `conn_abort_by_url` puis
  `cache_stop_client` (`capi.rs:1172-1179`). Tests : treize tests
  `capi::tests` dont `pending_abort_drains_after_op_start`,
  `conn_abort_by_url_stops_at_first_match`,
  `request_permitted_uses_mapped_cached_flag`. L3.
- **`control.c`** : `déviation D6 + D11` — machine à états
  commande/corps/réponse, EOF/erreur, reply bufferisée, `wait`
  différé + timeout injecté, `send/quit/cmd/dump/hdump/load/
  rawload` verbatim (`control.rs` vs `src/IO/control.c`) — sauf les
  constantes de flags injectés (D6) et le parsing strict du délai
  `wait` (D11). La glue pompe les sockets acceptés et arme les
  timeouts (`glue.rs:1281-1288`). Tests : `ping_pong`,
  `dump_hdump_and_load`, `wait_deferred_then_notify`,
  `wait_timeout_replies_error`,
  `wait_finished_immediate_and_double_wait`. L2/L3.
- **`dpi.c` client** : `conforme` modulo D10 — `dpi_ccc` branches
  commande/réception, tokeniseur `dpip` byte-exact (tags `<'`,
  `'>`, `send2eof`), auth `shared_key`, `DpidOK`/`DpidERROR`
  (`dpi.rs` vs `src/IO/dpi.c`, `dpi/dpip.c`). Tests :
  `token_tag_split`, `token_send2eof_raw`,
  `token_tag_partial_waits`, `comm_keys_parse`. L5.
- **Glue boucle d'événements** : `conforme` — `poll_timeouts()` +
  `capi_poll()` par tick (`glue.rs:1098-1099`) ≙ timers FLTK +
  `a_IOwatch`/`Dns_timeout_client` ; delayed-queue du cache via
  `timeout_add(0.0)` + `delayed_idle` verbatim (`cache.rs:1945-1987`
  vs `cache.c:1517-1547`).

## Déviations

### D6 — `control.rs` : constantes `ca_flags` erronées — **fonctionnel**

`control.rs:124-129` définit `GOT_HEADER = 0x10`, `GOT_LENGTH = 0x20`.
Amont (`cache.h`) : `CA_GotHeader = 0x1`, `CA_GotLength = 0x4` — et
`cache.rs:42-46` a les bonnes valeurs. `load`/`entry_inject` passent
donc `0x30` (= `CA_Redirect|CA_ForceRedirect`) : dans
`cache_entry_inject` (`cache.rs:1203`), `flags & GOT_LENGTH` est faux
→ la commande **`load` se comporte comme `rawload`** (corps re-parsé
comme réponse HTTP brute au lieu d'être injecté directement) et les
bits redirect ne sont jamais appliqués (entrée réinitialisée).
**Fix** : supprimer `control::ca_flags`, utiliser
`crate::cache::ca_flags::{GOT_HEADER, GOT_LENGTH}` aux lignes 396/840.
**Test** : `load` via FakeOps doit appeler `entry_inject` avec
`flags == 0x1|0x4` ; injection `cache_entry_inject` directe (pas de
re-parse) observable sur `cache_get_flags`.

### D7 — HSTS jamais appliqué : seam `cert_clean` non câblée — **fonctionnel**

Amont `a_Hsts_set` consulte `a_Tls_certificate_is_clean(url)` en direct
(`hsts.c:208`). Notre port injecte le verdict via
`cache_set_cert_clean` (`cache.rs:333,1042-1044`) — mais **aucun
appelant** ne l'invoque (`main.rs` câble `IoEngine`/`Dns`/`HttpEngine`/
`TlsEngine` par chaîne, `glue.rs` les seams UI/nav/dpi, jamais
`cert_clean`) → `clean = false` → tout en-tête
`Strict-Transport-Security` est ignoré (`cache.rs:851-852`) :
`hsts_set` no-op, aucune politique jamais enregistrée.
**Fix** : ajouter un registre de statuts cert consultable (≙
`Tls_certificates` amont) ou router via le `TlsEngine` de la connexion
courante ; câbler `cache_set_cert_clean` dans `main.rs`.
**Test** : injecter `cert_clean = |_| true`, réponse https avec
`Strict-Transport-Security` → `hsts_require_https(host)`.

### D8 — `socket_free` : retrait de `socks` trop tardif — **état-interne / L3**

Amont : `a_Klist_remove(ValidSocks, SKey)` **en premier**
(`http.c:301`), puis cleanup — une réentrance sur le même `SKey`
trouve `NULL` → no-op. Nous : `eng.socks.remove(&skey)` en fin de
fonction (`http.rs:811`) ; durant le cleanup (`connect_abort`,
`tls.close_by_fd`, `connect_queued_sockets` → callbacks), un
`socket_free(skey)` réentrant ré-exécute tout — `active_conns -= 1`
deux fois, `fd_map.remove` double, requeue double.
**Fix** : `let sd = eng.socks.remove(&skey)` en tête, travailler sur
la valeur extraite.
**Test** : `socket_free` réentrant simulé (cleanup qui rappelle
`socket_free`) → une seule décrémentation.

### D9 — `fd_map.insert` silencieux vs `assert` amont — **cosmétique / garde**

Amont `Http_fd_map_add_entry` : `MSG_ERR` + `assert(0)` si le fd est
déjà mappé (`http.c:191-194`). `HashMap::insert` réécrit en silence
(`http.rs:1468`, `1505`) — un sock_id recyclé rebinds sans trace.
**Fix** : `debug_assert!(eng.fd_map.insert(...).is_none())` +
`warn!` release.
**Test** : insertion doublon → avertissement/assert en debug.

### D10 — `a_Chain_check` absent de `capi_ccc_fn` et `dpi_ccc` — **état-interne / L3**

Amont garde **tous** les CCC : `a_Capi_ccc` (`capi.c:645`),
`a_Dpi_ccc` (`dpi.c:652`), `a_IO_ccc`, `a_Http_ccc` font
`dReturn_if_fail(a_Chain_check(...))` — un op sur lien
`Ended|Aborted` est ignoré avec warning. `io_ccc`/`http_ccc` l'ont
(`io.rs:775`, `http.rs:1651`) ; `capi_ccc_fn` (`capi.rs:287`) et
`dpi_ccc` (`dpi.rs:573`) ne l'ont pas. Atténué par les filtres
`flags` dans `bcb`/`fcb` et le `local_key → None` sur lien libéré,
mais les appels directs (`conn_abort_inner`, drain `pending_abort`)
court-circuitent ces filtres.
**Fix** : `if !chain.check("a_Capi_ccc", op, branch, dir, info) {
return; }` en tête de `capi_ccc_fn` ; idem `dpi_ccc`.
**Test** : `capi_ccc_fn(OP_SEND, 1, BCK, lien_ended)` → no-op +
warning.

### D11 — `wait <garbage>` : `parse` strict vs `atof` — **tolérance / L1-L5**

`control.rs:422-425` : `cmd[5..].trim().parse().unwrap_or(0.0)`.
Amont `atof(&cmd[5])` consomme le **préfixe** numérique :
`wait 5abc` → 5.0 s ; `wait 1e2junk` → 100 s ; chez nous ces chaînes
→ `0.0` → **attente infinie** (`pending_wait_timeout = None`).
`wait abc`/`wait -3` convergent (les deux → infini). Protocole de
contrôle machine (`dillorc` remote) — risque faible mais réel.
**Fix** : parser le préfixe `strtod`-compatible (espaces, signe,
mantisse, exposant optionnel) au lieu de `str::parse`.
**Test** : `wait 5abc` → `take_wait_timeout() == Some(5.0)`.

### D12 — DNS : re-résolution sur échec pendant un callback — **L3**

Amont `Dns_serve_channel` invoque `cb_func` **avant** de retirer
l'entrée (`dns.c:408-413`) : une `a_Dns_resolve` réentrante pour le
même host (pendant le callback) trouve l'entrée → partage la réponse
en vol, **même sur échec** (l'entrée ajoutée est servie dans la même
passe). Nous : `serve_channel` collecte et retire toutes les entrées
**avant** d'invoquer (`dns.rs:259-269`) → réentrant sur succès → hit
cache (couvert, `cache_add` avant serve, ≙ `dns.c:465`) ; réentrant
sur **échec** → file vide → nouvelle requête DNS (lookup doublé).
Effet observable : une requête DNS supplémentaire dans un coin de
réentrance ; pas de corruption.
**Fix** : invoquer avant retrait, ou conserver une entrée marqueur
pendant la passe.
**Test** : callback qui re-résout le même host sur échec → une seule
requête `resolve_host`.

### D13 — `io::submit` : octets stagés perdus si FD absent — **tolérance / L3**

`io.rs:358-370` : `staged = mem::take(&mut io.buf)` puis, si
`stream.is_none()`, early-return qui `drop` `staged` — `io->Buf`
amont conserve les données (`IO.c` `IO_submit` abandonne sans toucher
le buffer). Fenêtre : `write_data` stage avant armement (`wr_tx`
absent) puis `submit` sur io sans socket → les octets stagés sont
perdus.
**Fix** : restaurer `io.buf = staged` dans la branche d'échec.
**Test** : stage + `submit` sans fd → `io.buf` intact, flushable au
`submit` suivant.

## Invariants de frontière

- `http.c` reste transport pur ; `send_page_2eof`/`reply_complete`/
  redirects/keep-alive sont honorés **désormais** par `cache.rs`+
  `capi.rs` (verdict de passe 1 levé).
- `CapiConn` : `Ref` amont ≙ `Rc<RefCell<..>>` + `refs` — registre
  `conns` tant qu'une branche vit ; `conn_valid` ≙ présence dans
  `CapiConns`. **`pending_abort`** : abort demandé pendant un CCC en
  vol est **différé** et drainé à la sortie du top-level
  (`capi.rs:296-321`) — amont appelle `a_Capi_ccc(OpAbort)` en direct
  ; adaptation requise par `RefCell` (emprunt exclusif). Effet
  observable : l'abort termine l'op courante avant de défaire —
  ordre interne légèrement différé, état final identique.
- `dpi`/`downloads`/`vsource` sans `dpi_ccc` injecté → abort propre
  ≙ « serveur non démarré » amont (`capi.rs:437-452`).
- `dpi:/` non-vsource : `E2EQuery` posé via `Rc::get_mut` si possible,
  sinon `cache_entry_remove_by_url` — même effet observable
  (`capi.rs:997-1003`).
- `Cache` thread_local, single-writer UI ; callbacks clients
  détachés avant invocation (réentrance sûre) puis réinsérés.
- `cache_set_cert_clean` documentée mais **non câblée** (cf. D7) —
  l'invariant « HSTS ⇔ cert clean » tient dans le module, pas au
  runtime.
- `IO_close_fd` : `dClose` uniquement `IO_StopRdWr` — `StopRd`/
  `StopWr` gardent le socket partagé vivant (test
  `shared_fd_survives_writer_close`).
- `conn_resume` efface `PENDING` avant l'envoi (amont : après le
  `bcb`) — si un callback réentrant rappelle resume, nous ne
  renvoyons pas ; équivalent observable.

## Fichiers du fix (passe apply suivante)

- `crates/dillo-net/src/control.rs` : D6 (`ca_flags`, l.124-129,
  usages l.396/840), D11 (parsing `wait`, l.422-425).
- `crates/dillo/src/main.rs` (+ évent. `dillo-net/src/tls.rs`) : D7
  (câblage `cache_set_cert_clean`).
- `crates/dillo-net/src/http.rs` : D8 (`socket_free` l.720-813 —
  retrait en tête), D9 (`fd_map.insert` l.1468/1505).
- `crates/dillo-net/src/capi.rs` (l.287) + `crates/dillo-net/src/
  dpi.rs` (l.573) : D10 (`chain.check` en entrée).
- `crates/dillo-net/src/dns.rs` : D12 (`serve_channel` l.248-270).
- `crates/dillo-net/src/io.rs` : D13 (`submit` l.351-371).

## Non-bugs tranchés

- `Http_server_get` get-or-create : `servers.get` sans création —
  fuite amont non reproduite, observable identique (passe 1).
- `proxy_reply_recv` borne `size` vs `strlen` : équivalent dans notre
  pipeline (passe 1).
- `wait abc` → `parse` échoue → 0.0 ≙ `atof` → 0 → attente infinie
  des deux côtés (seul le cas *préfixe valide + suffixe garbage*
  diverge → D11).
- `strcmp(Data2, tag)` amont sur `Data2 == NULL` : crash amont, nous
  `as_str() → None` → no-op — dégradé plus sûr conservé.
- `conn_resume` : ordre de clear `PENDING` (avant envoi vs après
  `bcb` amont) — idempotent, pas de double envoi possible.
- `capi_open_url` `Rc::get_mut` → `cache_entry_remove_by_url` en
  fallback : `E2EQuery` n'est lu que pour l'effet « entrée retirée »
  en aval — observable identique.
- `socket_reuse` assigne `new.sd.sock_id = old_sock` avant
  `socket_free(old)` : `fd_map[old]` est retiré dans `socket_free`
  (l.752) puis ré-inséré vers `new_skey` (l.1468) — ordre amont
  « free → activate → add_entry » préservé ; la fenêtre
  `fd_map[old]` absente correspond à celle d'amont.
- `dns` : `cache_add` avant `serve_channel` ≙ `dns.c:465` — un
  resolve réentrant sur succès hit le cache (pas de doublon).
- Chunked « trailers » après `0\r\n` restent dans `leftover` et sont
  perdus — identique amont. Gzip multi-membres : second membre
  ignoré des deux côtés (pas d'`inflateReset`).

## Test gap analysis (passe 2)

- Absent : `load` transmet les bons flags à `entry_inject` (D6).
- Absent : seam `cert_clean` câblée → HSTS enregistré (D7) — test
  d'intégration binaire/glue, pas seulement unitaire.
- Absent : `socket_free` réentrant / `fd_map` doublon (D8/D9).
- Absent : `capi_ccc_fn`/`dpi_ccc` sur lien `Ended|Aborted` (D10).
- Absent : `wait 5abc`/`wait 1e2junk` → délai `atof`-compatible (D11).
- Absent : resolve DNS réentrant sur échec partage la réponse (D12).
- Absent : `submit` sans fd conserve `io.buf` (D13).
- Toujours absent (reporté passe 1) : `wire_end_to_end_get` étendu à
  keep-alive réel — `reply_complete` → réutilisation du fd sur un
  second GET (D8 fixé d'abord).
- Présents (filet existant) : redirects, chunked/gzip/charset
  tolérance, DNS FIFO/partage, TLS state machine, IO partagé,
  cache lifecycle, control wait/replies — 167 tests `dillo-net`,
  tous verts.

## Apply — résolution (passe 1, conservée)

- **D1** — non-bug (code commenté amont, « security problem »).
- **D2** corrigé — canal `IoEvent` borné (8) + `send().await`.
- **D3** corrigé — commentaire `proxy_reply_recv` rectifié.
- **D4** corrigé — `debug_assert!(sd.is_some())` branche 2F.
- **D5** corrigé — MSG « CONNECT through proxy failed » sur `OpAbort`.
- Revérification passe 2 : D2–D5 toujours en place
  (`io.rs` canal borné + tests, `http.rs` check/`debug_assert`/MSG).
  D6–D13 documentées — **pas d'apply dans cette passe**.

# Audit A01 — Chaîne I/O, HTTP & décodage transfert

Date : après commit `da29394` (port `http.c`). Mode : `rapport`.
Périmètre couvert : `src/IO/{IO.c,IO.h,iowatch.cc,http.c,mime.c,proto.c,
about.c,tls.c,tls_mbedtls.c}` + `src/{chain.c,decode.c,dns.c,url.c}` →
`crates/dillo-net/src/{io,chain,http,mime,about,tls,decode,dns,url}.rs`.
Hors périmètre porté à ce jour : `control.c`, `dpi.c` (phase DPI),
`cache.c` (parsing réponse + redirects — consommateur en aval).

## Verdicts checklist

- **Framing HTTP** : `conforme` — `make_query_str` reproduit l'ordre
  exact amont (`GET/POST uri HTTP/1.1`, `Host`, `User-Agent`,
  `Accept` par `WEB_*`, `Accept-Language`, `Accept-Encoding`,
  `Auth`, `DNT`, `Proxy-Authorization`, `Referer`, `Connection`,
  `Content-*`, `Pragma`/`Cache-Control`, cookies) — `http.rs:475-587`
  vs `src/IO/http.c:405-470`. URI absolue proxy (`'#` tronqué),
  boundary multipart `data[2..'\r')`, `Content-Length` = `data.len`,
  `Connection: keep-alive|close` selon `http_persistent_conns`.
  Tests : `make_query_str_*_golden`, `make_query_str_accepts_and_referer`,
  `wire_end_to_end_get` (requête GET octet-pour-octet sur loopback
  réel, `http.rs:2135` env.). L1/L5.
- **Redirects** : `N-A` — logée dans `cache.c` (non porté) ; `http.c`
  amont ne fait que transporter. Invariant de frontière noté.
- **`decode.c` chunked** : `conforme` — `chunk_remaining` inclut le
  CRLF (+2), consommé octet par octet **sans validation** `\r\n`
  (`==2`→CR, `==1`→LF), en-tête par `memchr('\n')` + `strtol`
  lenient (whitespace/sign/`0x` tolérés, troncature `long`→`int`,
  `0`/non-hex → `finished`), `leftover` conserve les lignes
  partielles — `decode.rs:120-169` vs `src/decode.c:34-91`.
  Tests : truncation, extensions `;`, whitespace `\n`, non-hex.
  L1/L2.
- **gzip/deflate** : `conforme` — zlib `Z_DATA_ERROR` → swap vers
  raw-deflate (fallback verbatim), gzip = en-tête RFC1952 + raw
  inflate + trailer CRC32/ISIZE ; mono-membre comme amont (zlib
  s'arrête à `Z_STREAM_END`, pas de `inflateReset` — `decode.c:142`).
  brotli + charset via `encoding_rs` (skip 1 octet sur erreur ≙
  `EILSEQ`) — `decode.rs:200-460`. Tests multi-étapes présents. L1/L2.
- **Granularité chunks (rendu progressif)** : `conforme` — amont
  `IO_read` accumule dans `io->Buf` jusqu'à EAGAIN puis **un seul**
  `OpSend` (`IO.c:157-219`) ; `read_task` accumule dans `out` et
  émet un `IoEvent::ReadData` par drain (`io.rs:509-580`). `buf` =
  `IOBufLen` 8192 des deux côtés. L4/L5.
- **DNS** : `conforme` — cache à scan ascii-insensible,
  `Dns_queue_find` (dédup in-flight par hostname), canal idle →
  `queue_add`, sinon `-2` (file d'attente FIFO), `D_DNS_MAX_SERVERS=4`
  — `dns.rs` (`resolve`, `MAX_SERVERS`, `-2`) vs `dns.c:370-400`.
  Threads → `spawn_blocking` ; callbacks chaîne-aware côté main via
  `poll_dns` ≙ `Dns_timeout_client`. Annulation post-abort : le cb
  retombe sur `eng.socks.get(&skey)` → `None` ≙ `Klist_get_data` NULL
  (`http_dns_cb`). Test : `queue_respects_…`, 5e requête en `-2`.
  L3/L4.
- **TLS** : `conforme` (avec écart assumé) — state machine
  `CONNECT_NEVER/NOT_YET/READY` + `CERT_STATUS_*` verbatim,
  `DilloVerifier` ≙ `VERIFY_OPTIONAL` (warnings enregistrés, décision
  post-handshake + dialog injecté), `close_by_fd` distingue
  connecting/established ≙ `a_Tls_close_by_fd`, `IoMsg::Swap` ≙
  `a_Tls_connection(fd)` par-fd — `tls.rs`, `io.rs`, `http.rs:1049,
  1490`. Écart : backend rustls ≠ openssl/mbedtls (suites/ciphers
  négociés différents — ADR-0003, wire-visible par nature). L2/L5.
- **`chain.c`** : `conforme` — `link_new`/`bcb`/`fcb`/`bfcb`/`check`
  + flags `Ended`/`Aborted` portés verbatim ; callback `Rc` cloné
  avant invocation ≙ unlink-safe ; `DataBuf`/`ChainData` typés ≙
  `void*`. Tests arène/`bfcb` reset (`chain.rs:228-340`). L3.
- **Backpressure écriture** : `conforme` — `wr_tx` accumule comme
  `dStr_append_l(io->Buf)` amont (non borné des deux côtés), écriture
  partielle `drain(..n)` + re-attente EAGAIN, aucun octet perdu ni
  réordonné — `io.rs:582-680` vs `IO.c:222-262`. L4.
- **Backpressure lecture** : `déviation D2` — voir ci-dessous.
- **Proxy** : `conforme` modulo `D1` — env `http_proxy` > prefs
  (`a_Http_init`, `http.c:120-129`), `no_proxy` suffix-match
  ascii-insensible avec quirk `dStrsep` (token vide = match universel)
  — `http.rs:414` vs `http.c:670-700`. `CONNECT host:443` +
  `Proxy-Authorization`, réponse accumulée `dStr` jusqu'à `\r\n\r\n`,
  statut `[9]=='2'` (`http.rs:1765` vs `http.c:1045-1075`).
  Tests : `must_use_proxy_suffix_and_empty_token_quirk`,
  `make_query_str_proxy_absolute_uri`, `get_connect_str_golden`. L5.
- **`about:`/`mime`/dispatch** : `conforme` — `ABOUT_SPLASH` verbatim
  (45 `\n`), `MimeRegistry` quirks (MinSize/MajSize single-scan,
  prefixe bidirectionnel) ; `proto.c` = won't port (stub vide amont).
  `dpi:`/dispatch `file:` restent à câbler côté browser (frontière).

## Déviations

### D1 — `Proxy-Authorization` initiale absente — **wire-visible**

Amont `a_Http_init` : si `prefs.http_proxyuser` contient `':'`,
`HTTP_Proxy_Auth_base64 = a_Misc_encode_base64(prefs.http_proxyuser)`
(`http.c:134-135`). Notre `HttpEngine::new` part à `None`
(`http.rs:314`) — un `http_proxyuser = "u:p"` dillorc n'émet aucun
en-tête `Proxy-Authorization` tant qu'aucun `set_proxy_passwd` (407)
n'est passé.

**Fix** : dans `HttpEngine::new`, `if cfg.http_proxyuser.contains(':')
→ proxy_auth_b64 = encode_base64(user)`. `misc::encode_base64` existe.
**Test** : cfg `http_proxyuser="u:p"` + proxy → `get_connect_str`
contient `Proxy-Authorization: Basic dTpw`; GET absolue idem.

### D2 — Lecture en avance non bornée (canal `IoEvent` unbounded) — **état-interne**

Amont : `OpSend` synchrone → le thread unique ne lit qu'un drain par
tick de boucle ; le socket buffer reste plein quand le consommateur
rame → contre-pression TCP naturelle. Ici `read_task` vide le kernel
buffer dans un `mpsc::unbounded_channel` (`io.rs:338`) : si
`poll_io`/cache traîne, les octets s'accumulent en mémoire sans borne
et le flux n'est jamais ralenti.

**Fix** : canal borné (`mpsc::channel(N)`, ex. 8 événements ou ~64 KiB
cumulés) et `read_task` `send().await` — la tâche se suspend, la
fenêtre TCP se ferme : contre-pression restaurée. Attention à ne pas
borner les événements lifecycle (`ReadErr`/`Eof` doivent pouvoir
passer : envoyer via le même canal borné est sûr — le consommateur
main-thread ne bloque jamais).
**Test** : récepteur qui ne consomme pas → le producteur ne dépasse
pas N événements en vol (compteur côté `IoEngine::drain_events`).

### D3 — Commentaire `proxy_reply_recv` cite la mauvaise API amont — **cosmétique**

`http.rs:1776` commente « `dStr_append_l(reply, dbuf->Buf,
dbuf->Size)` amont » — amont est `dStr_append(sd->https_proxy_reply,
dbuf->Buf)` (`http.c:1050`), i.e. `strlen` du buffer, pas `Size`.
Dans notre pipeline `size == buf.len` toujours → comportement
identique ; garder la borne `size` (plus sûr) mais corriger le
commentaire.

### D4 — `assert(sd)` amont non reproduit sur branche 2F — **état-interne**

Amont : `sd = a_Klist_get_data(ValidSocks, SKey); assert(sd);` avant
tout `OpSend/End/Abort` 2F (`http.c:1030-1031`) — état impossible =
crash. Nous : `is_some_and` → drop silencieux (`http.rs:1683+`).
**Fix** : `debug_assert!(sd.is_some())` + conserver le dégradé release
(philosophie `dReturn_if_fail`).

### D5 — MSG « CONNECT through proxy failed » manquant sur 2F `OpAbort` — **cosmétique**

Amont logge la réponse partielle sur `OpAbort` si `https_proxy_reply`
non nul (`http.c:1107-1112`) ; nous coupons sans message. Fix :
reporter le `debug!` déjà présent sur le chemin `OpEnd`.

## Invariants de frontière

- `http.c` est **transport pur** : status-line, `Content-Length`,
  chunked-vs-length, redirects et `Location` vivent dans `cache.c`
  (port suivant). Le tag `send_page_2eof` est transmis en `ChainData`
  — le consommateur `cache` devra l'honorer (fin de page = EOF).
- `"reply_complete"` est émis par le consommateur amont (`capi`/
  `cache`) sur la branche 2B — le futur `cache.rs` devra l'envoyer
  pour que `socket_reuse` s'arme ; sinon keep-alive inerte.
- `DilloWeb`/`cookies`/`auth`/`prefs` sont injectés (`HttpWeb`,
  callbacks `cookies_fn`/`auth_fn`, `HttpConfig`) : le câblage réel
  se fera côté `dillo-browser` — défauts = builds `!HAVE_COOKIES`
  amont (pas de headers).
- `msg(root, …)` ≙ `MSG_BW` : `root==0` toujours affiché,
  `root==1` seulement si `WEB_ROOT_URL` — la livraison UI est
  différée au sink.
- `close_by_fd` est appelé depuis `socket_free` même hors upgrade TLS
  (no-op sans entrée `Tls_servers`) — même chemin amont.

## Fichiers du fix

- `crates/dillo-net/src/http.rs` : D1 (`HttpEngine::new` l.~314),
  D3 (commentaire l.~1776), D4 (`http_ccc` 2F l.~1683), D5 (2F
  `OpAbort` l.~1716).
- `crates/dillo-net/src/io.rs` : D2 (`IoEngine::submit`/`read_task`
  l.~338/509 — canal borné + `send().await`).

## Non-bugs tranchés

- `Http_server_get` get-or-**create** dans `socket_reuse` : amont
  peut recréer un `Server_t` orphelin quand la file a été balayée
  (fuite interne) ; nous utilisons `servers.get` (pas de création) —
  même résultat observable (file vide → `Drop`), fuite non reproduite.
- `proxy_reply_recv` borne à `dbuf->Size` vs `strlen(dbuf->Buf)`
  amont : équivalent dans notre pipeline (`size == buf.len`) —
  conservé (plus strict), cf. D3 pour le commentaire seul.
- Lecture multi-événements par `poll` : amont envoie `OpSend` data
  puis `OpAbort`/`OpEnd` dans le même tick ; `poll_io` draine
  `ReadData` puis `Eof`/`ReadErr` dans l'ordre — séquence préservée.
- `write_data` stage dans `io.buf` quand la tâche n'existe pas
  encore ≙ `io->Buf` rempli avant `IO_submit` (premier `"FD"` puis
  données — jamais l'inverse en pratique, mais couvert).
- `assert(sd)` vs `is_some_and` : cf. D4 — dégradé plus sûr gardé.
- Chunked « trailers » : après `0\r\n`, les lignes restantes restent
  dans `leftover` et sont perdues — identique amont.
- Gzip concaténé multi-membres : ni amont (pas de `inflateReset`)
  ni nous ne relançons — membres suivants ignorés des deux côtés.

## Test gap analysis

- Absent : `Proxy-Authorization` initiale (cf. D1 — test à créer).
- Absent : erreur de lecture → `OpAbort` 2F propagé au CCC (tester
  `ReadErr` → `fcb(Abort)` côté `poll_io`).
- Absent : 2F `OpAbort` avec `https_proxy_reply` pending (MSG).
- Absent : borne de lecture en avance (cf. D2).
- Présent mais à étendre : `wire_end_to_end_get` — couvrir POST +
  réponse `reply_complete` → réutilisation réelle du fd sur un second
  GET (keep-alive wire complet, pas seulement `reuse_compatible`).

## Apply — résolution

Appliqué directement sur `main` (passe groupée A01+A02+A11) :

- **D1** — **non-bug** : le bloc `http_proxyuser → base64` est
  *commenté* amont (`http.c:131-136`, « security problem ») — le
  rapport avait lu du code mort. Aucun changement ; l'envoi par
  défi 407 est déjà verbatim.
- **D2** corrigé — `io.rs` : canal `IoEvent` borné
  (`channel(8)` ≙ drain synchrone amont) ; `send().await` dans
  `read_task`/`write_task` restaure la contre-pression TCP.
- **D3** corrigé — `http.rs` : le commentaire de
  `proxy_reply_recv` cite `dStr_append(reply, dbuf->Buf)`
  (strlen — `Size` inutilisé amont) ; la borne `min(size, len)`
  plus sûre est conservée.
- **D4** corrigé — `http.rs` : `debug_assert!(sd.is_some())` en
  tête de la branche `(2,FWD)` ≙ `assert(sd)` amont ; release
  dégrade en drop silencieux.
- **D5** corrigé — `http.rs` : `OpAbort` 2F logue la réponse
  CONNECT partielle (« CONNECT through proxy failed. Full reply
  not received:\n… », `(nothing)` si vide).

Test : `event_channel_is_bounded` (try_send saturé → `Err`).

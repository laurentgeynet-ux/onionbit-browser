# Audit A11 (PARTIEL) — browser glue : cache / capi / web / timeout

**Mode** : rapport (investigation seule, aucune modification).
**Périmètre** : `src/cache.{c,h}`, `src/capi.{c,h}`, `src/web.{cc,hh}`,
`src/timeout.{cc,hh}` ↔ `crates/dillo-net/src/{cache,capi,web,timeout}.rs`
(+ seams `http.rs`/`chain.rs`/`io.rs`/`misc.rs`). Le reste d'A11 (nav, bw,
uicmd, dicache…) n'est pas encore porté — reporté.

**Specs consultées** : `devdoc/CCCwork.txt` (ordre de construction des
deux branches, Answer avant Query, course DNS), `devdoc/Cache.txt`
(philosophie cache, clients, redirections) — le port est cohérent avec
les deux.

## Verdicts par sous-système

| Sous-système | Verdict |
|---|---|
| `timeout.cc` → `timeout.rs` | **Conforme** (un écart de doc, voir NB-7 ; `actually_remove` par id ≙ `{cb,cbdata}`) |
| `web.cc` → `web.rs` + `HttpWeb` | **Conforme avec réserve** : filtre `image/*` porté mais en casse sensible (D9) ; `bw` absent de `web_new` (NB-9) |
| `cache.c` → `cache.rs` | **Déviations** : D1 (Location perdue sur redirect bloquée), D2 (client sauté après dequeue — le plus grave), D3 (cookies), D5 (mismatch non-root), D6 (hits interne), D7 (auth/busy) |
| `capi.c` → `capi.rs` | **Déviations** : D4 (test IsCached sur flags bruts), D8 (abort tous conns vs premier), D9 (casse), D10 (E2EQuery via `Rc::get_mut`), D13 (`pending_abort` non drainé en OpStart) |

## Tableau des déviations

| # | Sévérité | Description | Amont | Rust | Fix suggéré |
|---|---|---|---|---|---|
| D1 | **wire-visible** | `Location` n'est stockée que si la redirect est permise. Amont stocke `entry->Location` *avant* le test domainrc/dpi : une 30x bloquée garde `Location` sans `CA_Redirect` → `entry->Location && !CA_Redirect` sert la page "Dillo blocked a redirection…" (`Cache_provide_redirection_blocked_page`). En Rust, `e.location` n'est écrit que dans la branche non-bloquée → `has_location_no_redirect`/`blocked_redirect` jamais vrais pour ce cas → la page bloquée est **injouable** ; le flux part en dispatch normal (vraisemblablement abort + offre de download). | `cache.c:810` (set inconditionnel) puis blocage `cache.c:812-820` ; consommateurs `cache.c:1399,1458-1461` | `cache.rs:778-801` (`e.location = location` ligne 794 dans le `else`), `cache.rs:1701-1703,1804-1806`, `1502-1547` | Assigner `e.location = location.clone()` avant le test `blocked` (comme amont), ne garder le blocage que pour les flags `CA_Redirect/*`. |
| D2 | **wire-visible** | Boucle clients de `process_queue` : après `clients.remove(idx)`, `i = idx + 1` est posé **inconditionnellement**. Quand le client n'est pas réinséré (paths `dequeued`/`finished`), le client suivant de la même entrée est retombé à `idx` et est sauté — il n'est jamais servi. Amont fait `--i` (puis `++i` de boucle) pour compenser le shift. Conséquence : toute entrée à ≥2 clients (même URL dans 2 fenêtres, image partagée…) ne clôt qu'un client sur deux. | `cache.c:1429-1432,1471-1472` (`--i` après dequeue) | `cache.rs:1632-1633` (`i = idx + 1` après `remove`), `1757-1758` (`continue` sans réinsertion), `1796-1842` (drop à `finished`) | Poser `i = idx` quand le client n'est pas réinséré (garder `idx+1` seulement sur le path réinséré). |
| D3 | comportementale | `Set-Cookie` : amont boucle sur **tous** les clients de l'entrée et ne `break` que si un client *autorisé* a posé les cookies ; un client non-autorisé n'interrompt pas le scan. Rust `break` après le **premier** client dont l'URL matche, autorisé ou non → cookies non posés si le 1er client est tiers mais pas un suivant. | `cache.c:893-919` (break uniquement ligne 916, dans la branche permise) | `cache.rs:870-905` (`break` ligne 904 hors condition) | Ne `break` qu'après `cookies_set` (supprimer le `break` de ligne 904). |
| D4 | comportementale | `request_permitted` teste `cache_get_flags(url) & CAPI_IS_CACHED` sur les flags **bruts** `CA_*` — `0x1` = `CA_GotHeader`, pas « en cache ». Amont passe par `a_Capi_get_flags` (mappé : `IsCached` ⇔ entrée existante). Une sous-requête pour une URL en cours de header est soumise à `domainrc` en Rust alors qu'amont elle est permise. | `capi.c:379` (`a_Capi_get_flags` mappé via `Capi_map_cache_flags` `capi.c:501-517`) | `capi.rs:792` (`cache_get_flags(&web.url) & to_capi_cached()`), helper `800-802` | `capi_get_flags(&web.url) & CAPI_IS_CACHED != 0` (ou `cache_get_flags != 0`). |
| D5 | comportementale | `TypeMismatch` → `AbortEntry` n'est appliqué qu'aux clients **RootUrl** : Rust ne calcule `type_mismatch` que dans la branche `WEB_ROOT_URL`. Amont calcule `TypeMismatch` une fois (hors boucle) et l'applique dans `!Client->Callback` pour tout client — un client non-root sans callback sur entrée mismatchée abort l'entrée amont ; en Rust elle est dispatchée normalement. | `cache.c:1330,1371-1375` (msg seulement si root), `1402-1403` (abort non filtré) | `cache.rs:1664-1668` (calcul sous `web_flags & WEB_ROOT_URL`), usage `1704` | Calculer `type_mismatch` hors de la branche root (le `set_msg` reste root-only). |
| D6 | état-interne (visible sur `about:cache`) | `hits += 1` appliqué au miss interne (`about:cache`/`about:dicache` 1re charge) ; amont n'incrémente `Hits` que sur hit. Affiche Hits=1 au lieu de 0. | `cache.c:491-513` (`Hits++` ligne 496, branche hit seule) | `cache.rs:1078-1089` (`hit_url` = Some aussi pour miss interne) | N'incrémenter que dans la branche hit (`entry_pos` réussi), pas pour le miss interne. |
| D7 | comportementale (edge) | `entry->Auth` consommé avant le test `busy` : Rust `auth.take()` puis `auth_entry` logge « caught busy » et perd les défis. Amont ne vide `entry->Auth` que lors de l'armement du timeout ; si busy, les défis restent pour un déclencheur ultérieur (réouverture de l'URL). | `cache.c:1211-1228` (`entry->Auth = NULL` ligne 1226, seulement si armé) | `cache.rs:1871-1882` (`take()` avant `auth_entry`), `1457-1467` | Ne `take()` que si `!auth_busy` (déplacer le test dans le bloc de prise). |
| D8 | comportementale | `conn_abort_by_url` aborte **toutes** les conns dont l'URL matche ; amont `break` après la **première**. Deux conns sur même URL → Rust coupe les deux. | `capi.c:205-216` (`break` ligne 214) | `capi.rs:273-291` (collecte tous les `targets`) | `find` le premier match puis `break` (fidélité), ou documenter l'écart. |
| D9 | comportementale (edge) | Casse : `url_uses_dpi` teste `starts_with(b"http:"/"https:"/"about:"/"dpi:/")` en casse sensible vs `dStrnAsciiCasecmp` amont ; `dispatch_by_type` `mime.starts_with("image/")` vs `dStrnAsciiCasecmp(Type,"image/",6)` amont. `HTTP://x` → `proto.HTTP` dpi en Rust ; `Content-Type: IMAGE/PNG` non-root rejeté. | `capi.c:275-279,290` ; `web.cc:107` | `capi.rs:647,650` ; `web.rs:67` | Comparaison ASCII-CI (`eq_ignore_ascii_case` sur préfixe). |
| D10 | comportementale (latente) | `URL_E2EQuery` sur `web->url` pour les reloads `dpi:` : Rust via `Rc::get_mut` — silencieusement sauté si le `Rc` est partagé → `cache_open_url` ne voit pas le flag → ancienne entrée servie (pas de regen dpi). Amont mute `web->url` inconditionnellement. | `capi.c:438` (`a_Url_set_flags`) | `capi.rs:927-929` | Mutabilité intérieure sur `url.flags` (Cell<u32>) ou invariant d'unicité documenté+assert. |
| D13 | état-interne (edge) | `pending_abort` non drainé si posé pendant `OpStart` : `capi_ccc_fn` résout `conn` via `local_key` **à l'entrée** ; pendant OP_START le `local_key` n'est pas encore posé (`set_local_key` ligne 356) → pas de comptage `depth` → drain impossible par cet appel. | `capi.c:647-664` | `capi.rs:238-252,356` | Passer `conn` explicitement aux appels START (le `d1` contient déjà `Rc<RefCell<CapiConn>>`). |
| D11 | cosmétique | `datastr` = `String::from_utf8_lossy(data)` — payload binaire non-UTF8 altéré ; amont `dStrdup` tronque au NUL. Commandes dpi = texte → négligeable. | `capi.c:103` (`dStrdup`) | `capi.rs:1021` | Garder `Vec<u8>` pour `datastr`. |
| D12 | cosmétique (doc) | rustdoc `timeout.rs` affirme « amont `Fl::repeat_timeout` » — faux : `a_Timeout_repeat` amont = `Fl::add_timeout` (identique à `add`). L'impl Rust est fidèle, la doc décrit une déviation inexistante. | `timeout.cc:34-37` | `timeout.rs:63-69` | Corriger le commentaire. |
| D14 | cosmétique | `about:cache` : troncature URL `chars().take(60)` vs `dStr_shorten` amont (moitiés + `...`). Sortie HTML légèrement différente. | `cache.c:427` + `dlib/dlib.c:550-564` | `cache.rs:570` | Porter `Dstr::shorten` (existe : `dlib.rs:348`). |

## Non-bugs (écarts apparents fidèles)

- **NB-1** — Client détaché/réinséré vs pointeur en place : évite le vrai
  UAF amont si le callback s'auto-stoppe (amont libère `Client`+`Web`
  dans `a_Cache_stop_client` puis réutilise `Client`/`ClientWeb` —
  `cache.c:188,1446-1463`). Détaché = équivalent observable, plus sûr.
- **NB-2** — `pending_abort` : amont `a_Capi_conn_abort_by_url` aborte en
  plein vol CCC ; Rust diffère si la `Chain` est empruntée (`RefCell`).
  Depuis `process_queue` le chain n'est pas emprunté → immédiat ; les
  effets sont tous appliqués avant retour à la boucle — ordre observable
  conservé. (`capi.rs:269-303`)
- **NB-3** — `cl.buf` snapshot `Vec<u8>` vs pointeur live `data->str` :
  le callback lit en synchrone → équivalent (coût O(n) par envoi, perf).
- **NB-4** — `web_free` = `invalidate()` (`Cell<bool>` ≙ membership
  `ValidWebs`) ; `Rc` remplace `dFree` — `a_Web_valid` ≙ `valid()`.
  (`web.rs:46-57`, `http.rs:138-145`)
- **NB-5** — `null_client` Rust ≙ `None`+no-op : amont `Cache_null_client`
  n'agit que sur `CA_Close && RootUrl` (`close_client`) ; les chemins où
  amont l'installe puis écrase (blocked page, redirect non-root) sont
  reproduits par réaffectation de `cl.callback`.
- **NB-6** — `Cache_internal_url` injecte `CA_GotHeader|CA_GotLength`
  **sans** `CA_InternalUrl` → `about:cache` regénéré à chaque open
  (l'entrée est retirée par `entry_remove`) ; `about:splash` a
  `CA_InternalUrl` → jamais retiré. Porté tel quel.
  (`cache.c:368-369,451-455` ↔ `cache.rs:509-511,537-549`)
- **NB-7** — `a_Timeout_remove` = no-op (« WTF? FIXME ») porté ;
  `a_Timeout_add`/`repeat` = `Fl::add_timeout` one-shot les deux ;
  drain `poll_timeouts` = deadline min d'abord, callbacks réentrants
  réarmés dans la même passe ≙ passe FLTK. (`timeout.cc:26-52` ↔
  `timeout.rs:59-102`)
- **NB-8** — `a_Chain_check` amont (garde + warn) ≙ `check()` +
  `dReturn` implicite — le corps Rust ne l'appelle pas systématiquement
  (logs amont = diagnostique).
- **NB-9** — `a_Web_new(bw,…)` : `web_new` ne prend pas `bw`
  (`HttpWeb::new` → `bw:0`, champ pub à poser par le glue) ; champs
  `Image`/`bgColor` absents de `HttpWeb` (derrière seam `dillo-browser`
  — à prévoir). `a_Web_dispatch_by_type` exige `bw != NULL` amont —
  Rust délègue à la seam.
- **NB-10** — `conn_resume` : amont efface `PENDING` après l'émission
  (`capi.c:189`) ; Rust avant (`capi.rs:314`). Ne diverge que sur un
  `DpidOK` réentrant — négligeable.
- **NB-11** — `send_page_2eof` : Rust borne `dbuf.size` à `buf.len()` ;
  amont fait confiance à `Size` (OOB sur pair malformé). Plus sûr.
  (`capi.rs:542-543` vs `capi.c:773-775`)
- **NB-12** — Quirks portés verbatim : clé client wrap à `i32::MAX`→1,
  unfold « retire `\n` garde l'espace » (« par tial »), strip `\r`/NUL
  sans reset du compteur `N`, `strtol` tolérant, `Cache_entry_add`
  warn-and-replace, flags `CA_*`/`CAPI_*`/`IO_*` identiques, gabarit
  `about:cache`, `100 Continue` reset complet.
- **NB-13** — `dtype`/`dfilename` : amont « dernier client root gagne »
  (réécrit par itération) ; Rust « premier gagne » (`is_none()`) — même
  entrée → même valeur, équivalent.
- **NB-14** — `delayed` keyed par URL ≙ pointeur faible amont (entrées
  uniques par URL) ; drain retire sur retour non-NULL **et** NULL —
  amont s'en remet à `Cache_entry_remove` (qui retire de DelayedQueue) ;
  converge.

## Conformité détaillée (points vérifiés OK)

- `process_dbuf` : boucle header `\n\n`, strip `\r`/NUL, unfolding,
  100-continue reset+rejeu du reste, `TransferSize` brut, `done` =
  `GotLength && TransferSize>=Expected`, forcé false si `!KeepAlive`,
  `IOAbort` (empty→clients retirés sans callback — `CA_Abort` jamais
  émis, TODO amont respecté ; data→`finish_msg`). `cache.c:960-1133`
  ↔ `cache.rs:692-715,1296-1414`.
- `parse_header` : ordre des branches identique (status idx 9, 30x/401/
  404, Warning, HTTP/1.0→KeepAlive, `Connection:`, HSTS, TE, CL ignoré
  si TE présent, cookies, CE, HUGE_FILESIZE, Content-Type via
  `set_content_type`, Content-Disposition, `ref_data`).
  `cache.c:784-957` ↔ `cache.rs:750-935`.
- `set_content_type` : priorité Norm>Meta>Hdr>Det, META-charset-seul →
  `Det+ctype`, workaround `text/xhtml`→`application/xhtml+xml`,
  charset→decoder+invalide UTF8Data. `cache.c:607-661` ↔
  `cache.rs:172-184,939-982`.
- `Cache_redirect` : SpamSafe, `redirect_level>=5`→`RedirectLoop`,
  seuil `!len || len<1024`, `E2EQuery` sur `TempRedirect`,
  `a_Nav_push`/`NavOps::push` requester=url d'origine, non-root =
  log seul. `cache.c:1139-1185` ↔ `cache.rs:1417-1453`.
- `capi_open_url` : ordre download→dpi→http→about, `reload` =
  `!IsCached || E2E` (ici via `capi_get_flags` mappé — correct),
  downloads-dpi quand non caché, branche 2B **avant** 1B (course DNS —
  devdoc/CCCwork), `conn_valid` avant `cache_open_url`, `web_free` si
  `!use_cache`. `capi.c:394-496` ↔ `capi.rs:870-1008`.
- CCC `a_Capi_ccc` : 4 branches conformes — 1B (start link http/dpi,
  send→bcb, end/abort→bcb+unref+free), 1F (`FD`→sock_fd+relais 2B,
  `DpidOK`→resume, abort→`IOAbort`+DpidERROR/`Both`+cancel_expect),
  2B (send relais `FD` seul, abort→bcb+`entry_remove_by_url`),
  2F (`send_page_2eof`→`process_dbuf`+`reply_complete` si done&valid,
  status/chat/dialog/reload, end→`IOClose`+propagation 1B,
  abort→`IOAbort`+`Both`→1B). Pas de `OpEnd` 2B amont → `default` warn
  ≙ `_` log Rust. `capi.c:640-835` ↔ `capi.rs:336-634`.
- `dpi_verify_request` : `dpi:` sans GET|POST OK ; GET `/vsource/` OK ;
  sinon referer=top nav doit être `dpi:` ; non-dpi toujours permis.
  `capi.c:233-265` ↔ `capi.rs:674-716`.
- `dpi_send_source` : deux `:` requis, `start_send_page` + data ou
  `DpiError`. `capi.c:323-344` ↔ `capi.rs:737-765`.
- `stop_client` : unique→`force || BufSize==0`→abort conn.
  `capi.c:623-635` ↔ `capi.rs:1095-1102`.
- `ValidWebs`/`a_Web_free` ≙ `valid`/`invalidate`, `web_free` au dequeue
  client. `web.cc:152-162` ↔ `web.rs:55-57`, `cache.rs:466-471,1751,1835`.
- `timeout` : one-shot, no-op remove, `actually_remove` (par id ≙
  `{cb,data}` — seul usage amont : `IO/control.c:444`), drain FIFO par
  deadline, réarmement réentrant. `timeout.cc` ↔ `timeout.rs`.

## Test gap analysis (goldens manquants)

1. **Multi-client même entrée** (D2) : 2 clients même URL, données +
   close → vérifier que *les deux* reçoivent `CA_SEND`+`CA_CLOSE`.
   Aucun test existant ne couvre ≥2 clients actifs simultanés.
2. **Redirect bloquée domainrc** (D1) : 30x + `domain_permit` faux →
   page HTML "blocked a redirection" servie au client root — à goldenner
   (HTML exact amont `cache.c:1295-1300`).
3. **Cookies multi-clients** (D3) : 2 clients même URL, 1er non
   autorisé (requester cross-org), 2e autorisé → `Set-Cookie` posé.
4. **Mixed content + entry en vol** (D4) : requester https, sous-
   ressource http en cours de réception (GotHeader=0) → permis amont.
5. **TypeMismatch non-root** (D5) : image servie `text/html` réel →
   abort amont / dispatch Rust.
6. **Auth busy** (D7) : 401 × 2 en vol → 2e défi conservé amont.
7. **Hits interne** (D6) : `about:cache` 1re charge → Hits=0 amont.
8. **`dpi:` reload E2E** (D10) : `Rc` partagé → flag perdu (latent).
9. Casse : `HTTP://` scheme, `IMAGE/PNG` content-type non-root (D9).
10. `pending_abort` pendant OpStart (D13) — abort différé jamais drainé.
11. Golden `about:cache`/`about:dicache` (markup, `dStr_shorten`).
12. DelayedQueue drain avec réarmement réentrant pendant le drain.

## Suites non testables ici

`a_Bw_*`/`a_Nav_*`/`a_UIcmd_*`/`a_Dicache_*` sont des seams (`u32` bw) —
la parité observable complète (status bar, nav stack, dicache) dépend
du glue `dillo-browser` non encore porté → reporté avec le reste d'A11.

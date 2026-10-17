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

---

## Addendum — `bw` / `nav` / `keys` / `uicmd` (phase 6, lot chrome-1)

**Périmètre ajouté** : `src/bw.{c,h}` ↔ `bw.rs` (572 l.),
`src/nav.{c,h}` ↔ `nav.rs` (854 l.), `src/keys.{cc,hh}`+`keysrc` ↔
`keys.rs` (927 l.), `src/uicmd.{cc,hh}` ↔ `uicmd.rs` (2340 l.).

### Verdicts

| Module | Verdict |
|---|---|
| `bw.c` → `bw.rs` | **Conforme** — mutabilité intérieure ≙ pointeur nu amont ; registre id ≙ `bws`/`num_bws` |
| `nav.c` → `nav.rs` | **Conforme** — pile verbatim, seams fusionnées (NB-10) |
| `keys.cc` → `keys.rs` | **Conforme** — table + parser + decode UTF-8 + quirk mods droppés verbatim |
| `uicmd.cc` → `uicmd.rs` | **Conforme avec seams** — logique verbatim, FLTK → `UiShell` |

### Points vérifiés verbatim (relecture croisée)

- `UIcmd_find_search_str` : `strcspn(str," ")`, garde
  `len>0 && str[len]!='\0'`, préfixe `dStrnAsciiCasecmp(str,search,len)`,
  skip si `search[len]!=' '` (match partiel d'une autre entrée),
  `search_url_idx = p` — **quirk amont `bool_t` préservé**
  (`prefs.rs` : `search_url_idx: bool`, assignation `p != 0`).
- `a_UIcmd_open_urlstr` : strip délimiteurs, `file:` casse-insensible,
  `ch=='\0'||'.'` → `Paths::getOldWorkingDir()` ≙
  `UiShell::old_working_dir`, sinon `file:` comme base.
- `UIcmd_open_url_nbw` : `new_tab_page != "about:blank"` → URL par
  défaut ; push + `set_location_text` + `focus_main` / sinon
  `focus_location` + `set_buttons_sens`.
- `a_UIcmd_browser_window_new` : `ww/wh<=0` → `prefs.width/height` ;
  `old_bw==NULL` → `zoom = prefs.zoom_factor` + `xpos/ypos` ; xid→Xembed
  non porté (`won't port` — X11).
- `UIcmd_tab_new` : layout prefs verbatim (`bg_color`, `set_bg_image`
  REPEAT/SCROLL 0,0, `buffered_drawing`, `scroll_step`,
  `scroll_page_overlap`, scrollbars via seam `tab_ui_new`),
  `a_Bw_new`, titre si focus, `findbar_toggle(0)` workaround.
- `a_UIcmd_close_bw` : `stop_clients(Root|Img|Force)` → drop layout →
  `tab_ui_remove` (fenêtre détruite si 0 onglets, côté shell) → `free`.
- `a_UIcmd_set_page_title` : buffer 128, `MIN(124, 1+end_of_char(t,120))`
  + `...` — **vérifié byte-pour-byte** : `end_of_char` opère sur les
  120 premiers octets, identiques amont/port (troncature amont à 127
  n'affecte pas la plage lue). Label d'onglet = label brut (non tronqué)
  ; titre fenêtre seulement si onglet actif (`is_active_tab` ≙
  `wizard()->value()` cmp).
- `a_UIcmd_set_buttons_sens` : Stop=clients non vides, Back=`ptr>0`,
  Forw=`ptr<size-1 && !expecting` — verbatim.
- `a_UIcmd_has_finished`/`finish_loading` : clients+expecting, puis
  `control_is_waiting` → `drain_idles` ≙ `generalIdle` + `flush` +
  `control_notify_finish` ≙ `a_Control_notify_finish` (dilloc).
- `keys.rs` : `keystr` quirk `dStrndup(key, p-key-1)` porté ;
  `event_text[0]` sur texte vide = `\0` → considéré contrôle ; table
  `default_keys` 33 entrées et constantes `FL_*`/XF86 verbatim ;
  `about:keys` injecté `GotHeader|GotLength|InternalUrl`.
- `nav.rs` : `stack_ptr=-1` initial ≙ `a_Bw_new` ; `expect_done` n'agit
  sur la pile que si `expecting` (quirk : back/forw n'empilent pas) ;
  reload flags `ReloadPage|E2EQuery`−`SpamSafe`, POST→dialogue sauf
  `dpi:/vsource/` ; redirection0 `E2E|IgnoreScroll`+status 2 différé.

### Déviations structurelles (documentées, non bugs)

- **NB-10** — `NavIo` = `NavBw`+`NavUi` en **un seul** objet seam :
  amont les deux sont des fonctions globales accédant le registre bw ;
  en Rust deux `&mut` distincts vers le même registre pendant
  `nav.push`/`open_url` seraient un aliasing illégal (le callback UI
  peut muter le registre — `open_url_nt` → `tab_new`). Un objet seam
  unique reproduit le modèle global amont sans UB.
- **NB-11** — Champs `BrowserWindow` en `Cell`/`RefCell` : amont mute
  `bw->nav_*`/`bw->expect_*` pendant `a_Nav_push` via pointeur nu.
  L'intérieur mutable préserve l'ordre d'appel sans double-`RefMut`.
- **NB-12** — `UiShell` trait ≙ `UI`/`CustTabs`/`Fl_*` : chrome
  auto-dessiné (phase chrome-2 implémentera `ui.rs`). Choix de
  dialogues 1-based verbatim (`dialog_choice` → 1= premier bouton).
- **NB-13** — `Rc::get_mut`-style URL flags (D10) non concerné ici :
  `nav::reload` mute via la seam `NavIo::url_set_flags` — à auditer
  avec D10 en mode apply.

### Test gap analysis (ajout)

13. **Onglets multiples + fermeture** : 2 onglets, close du 1er →
    registre/labels/sens cohérents (couvert partiellement par
    `uicmd::tests`, golden e2e quand `ui.rs` existera).
14. **`keysrc` round-trip** : parser un `keysrc` custom (rebind +
    `nop` + commentaires + ligne malformée → warning lineno) —
    couvert (`keys::tests::parse_and_rebind`).
15. **POST reload flow complet** : `nav.reload` sur URL POST →
    `dialog_choice` ; couvert (`reload_post_asks_confirmation`).
16. **Zoom nt/nw** : préservation du zoom entre bw — couvert
    indirectement (`zoom_bounds`).

Verdict addendum : les 4 modules sont **fidèles** ; les seams
(`UiShell`, `NavIo`/`NavSched`, `DpiOps`) concentrent toute la
déviation plateforme — à repasser en `apply` après `ui.rs` (chrome-2)
pour la parité observable e2e.


## Addendum 2 — `ui.rs` (`src/ui.cc` + `uicmd.cc` Cust*) — lot chrome-2

### Surface couverte

`UI` → `Ui` (widgets logiques auto-dessinés, ADR-0002),
`CustGroupHorizontal`/`CustGroupVertical` → `Group`/`Ui::rearrange`,
`CustInput`/`TipWinInput` → `LineInput` + `cust_input_key`,
`CustProgressBox` → `WId::PageProg`/`ImgProg`, `CustTabs` +
`Fl_Wizard`/`Fl_Pack`/`Fl_Scroll`/`CustTabButton`/`Fl_Button`(close)
→ `Tabs`/`TabBtn`, `iconset`/`standard_icons`/`small_icons` →
`IconSet` + `xpm_decode` (`pixmaps.rs` généré verbatim), globaux
`tabgroups`/`UI*` → `Chrome` (impl `UiShell` + `NavSched`).

### Vérifications verbatim effectuées

- `CustGroupHorizontal::rearrange` — resizable reçoit `w - sum` des
  visibles, `resizable = NULL` en cas de débordement : reproduit et
  testé (`group_rearrange_*`).
- `UI` ctor : `cur_ui` hérite `PanelSize`/`Small_Icons`/`Panelmode`
  (verbatim), sinon prefs ; `fullwindow_start` → `UI_HIDDEN` +
  `panels_toggle`.
- Métriques panel : P_tiny 22|28, P_small 20|28, P_medium
  42·22|45·28 avec `lbl` small_icons-only ; `nh=bh`, `fh=28`,
  `sh=20`, `pw=70` — testés (`panel_metrics_verbatim`).
- `make_panel` tiny : une barre `[btns, File, Clear, Location,
  Search, Help, ImgProg, PageProg]` resizable=LocationGroup ;
  small/medium : LocBar + NavBar resizable=Spacer. **Bug corrigé
  en cours de port** : resizable pointait sur `Clear` (idx) au lieu
  de `LocationGroup` (idx+1) — détecté par le test de construction.
- `CustTabs` : `tab_w=50`, `tab_h=ui_tab_height`, `ctab_h=1`,
  `btn_w=20`, `ctl_w=22` ; strip visible à ≥2 onglets ; switch →
  `increase_focus_counter` + wrap u32→renum triée verbatim ;
  `remove_tab` actif → focus `focus_num` max ; `set_tab_label`
  ≤15 octets + `end_of_char` + `...` ; `update_pack_offset` garde
  l'actif visible. **Bug corrigé** : troncature paniquait sur
  label court (`[..15]` > len) → `min`.
- `CustInput::handle` filtres verbatim (Esc remonte, Shift+←→
  remontent, Ctrl+E/K/D/A/L consommés, Ctrl+H/O/R/Home/End remontent,
  ↑↓/PgUp/PgDn/Tab → focus main + remontée, Enter → `location_cb` +
  repli panel temporaire).
- `UI::handle` FL_SHORTCUT → `ui_shortcut` : dispatch
  `KeysCommand` complet verbatim (scroll cmds→`scroll`, HidePanels
  →findbar d'abord + reset temporary, FocusTab1-10 index
  zero-based verbatim).
- `set_page_prog`/`set_img_prog`/`set_bug_prog` : formats
  `Page
%.1f %cB` (K/M ≥1MB) / `%d of %d` / `new_w=len*9+20`
  verbatim — testés.
- `set_location` : position = `size()` si focus Location sinon 0
  (verbatim) — testé.
- `tab_ui_new` : hérite réglages panel de l'UI courante (verbatim
  `UI(w,h,cur_ui)`), attache `SharedView`+scroll prefs,
  `add_new_tab` verbatim ; `tab_ui_remove` détruit la fenêtre à 0
  onglets (verbatim `num_tabs()==0 → delete tabgroup`).

### Non-bugs / seams

- **NB-14** — ~~Dialogues et menus popup = stubs~~ → **résolu
  (chrome-3)** : `dialog.rs`/`menu.rs`/`findbar.rs`/`tipwin.rs`
  portés (addendum 3). `DialogRunner`/`DocOps` restent des seams
  (impl native au lot chrome-4/app).
- **NB-15** — `prefs.search_url_idx: Cell<bool>` : la mutation
  verbatim pendant `open_urlstr` sans `&mut Prefs` permet au
  `Rc<RefCell<Prefs>>` partagé du Chrome de coexister avec les
  `&Prefs` des appels (pas de double-emprunt).
- **NB-16** — Dessin chrome : `pm_box`/`pm_text`/`pm_icon`
  reproduisent `Fl_Widget::draw` (box 3D, icône centrée, label) —
  parité pixel non garantie (déviation ADR-0002 documentée) ;
  géométrie/états observables fidèles.
- **NB-17** — XPM décodé en `xpm_decode` (format `c #rrggbb`/`None`
  verbatim) ; `*In` calculés par `desaturate`+`color_average(bg,.14)`
  exactement comme `iconset` amont (les `*_i_xpm` embarqués restent
  inutilisés comme amont).

### Test gap analysis (ajout 2)

17. **Golden chrome** : rendre une fenêtre `Chrome` headless et
    comparer le layout de chrome à une capture amont — à faire avec
    le harness golden (hors scope unitaire).
18. **Wheel/shortcut tabs** : couvert côté `uicmd` ; le hit-test
    `tab_at`/`close_at` couvert (`tab_hit_test`).


## Addendum 3 — `menu.rs` / `dialog.rs` / `findbar.rs` / `tipwin.rs` (lot chrome-3)

### Surface couverte

- `src/menu.cc` + `menu.hh` → `menu.rs` (~950 lignes) : les 8
  popups (`page`, `link`, `image`, `form`, `file`, `bugmeter`,
  `history` back/forw, `tools`), `MenuCtx` ≙ globaux `popup_*`,
  dispatch différé, seams `DocOps` (≙ `a_Html_*` doc callbacks) et
  `UiShell::*_popup`/`panel_state`/`panel_change`.
- `src/dialog.cc` + `dialog.hh` → `dialog.rs` : `DialogState` +
  seam `DialogRunner` (≙ fenêtres FLTK) + `LineEditor` (≙
  `Fl_Input` — Ctrl-A/E/K/D, cut, clamp UTF-8).
- `src/findbar.cc` + `findbar.hh` → `findbar.rs` : `FindbarState`,
  filtre `findbar_key` verbatim (verdict Pass/Consume), métriques.
- `src/tipwin.cc` + `tipwin.hh` → `tipwin.rs` : `TipWin` partagé,
  délais verbatim, `CustButtonState`/`TipWinInputState` highlight.

### Vérifications verbatim effectuées

- Page popup : « View page source » désactivé sur `dpi:/vsource/`,
  « View page bugs » actif iff `page_bugs` ; sous-menu stylesheet
  = urls CSS en `user_data` (≙ `item->user_data()` amont) —
  **pas** l'url de page ; labels ellipsés tête 30 / queue 40.
- Link popup : actions dynamiques `a_Actions_link_get` insérées
  verbatim ; middle-click ≙
  `middle_click_opens_new_tab`/`focus_new_tab`, Shift inverse le
  focus (verbatim `!!(state & FL_SHIFT)`).
- History popup : liste `a_UIcmd_get_history` terminée par `-1` ;
  item i → `nav_jump(dir*(i+1))`, bouton 2 → nouvel onglet.
- Tools popup : toggles écrivent `prefs` verbatim
  (`load_stylesheets`, `parse_embedded_css`, `load_images`,
  `load_background_images`, `http_force_https`) + radios
  panel-size/`small_icons` → `panel_change` (≙ `a_UIcmd_*`).
- `file` popup : Exit → `defer_close_all` (≙
  `a_Timeout_add(0.0, a_UIcmd_close_all)` amont).
- `bugmeter` validate : fragment `#` tronqué, url hex-encodée dans
  le gabarit `%s` verbatim, schéma `dpi` exclu.
- `dialog` : `choice` retourne index 1-based, cancel/close→0 ;
  `user_password` verbatim ; `search` : labels
  `prefs.search_urls` + `search_url_idx` (quirk `bool_t`).
- `findbar_key` : verdicts verbatim — navigation/Shift+←→/
  Ctrl-HIJLM/Esc passent ; Ctrl-AEKD locaux ; Enter→search fwd,
  Shift+Enter→back (≙ `FL_SHIFT`).
- `tipwin` : `tip` tronqué 255 octets + frontière UTF-8, `size`
  = mesure + 8, position sous le pointeur sinon au-dessus,
  délais 0.8 s / 0.1 s (récent), cancel sur input.

### Corrections de bugs trouvées pendant le port

- `NavUi::set_buttons_sens` prend désormais `(ptr, size)` en
  snapshot : appelé depuis `Nav::*` pendant que `b.nav` est
  emprunté — verbatim amont lit `bw->nav_*` par pointeur ; le seam
  Rust ne peut pas re-emprunter (`RefCell`). Aucun écart
  observable (valeurs identiques à l'instant de l'appel).
- `NavOp::CloseBw` ajouté (≙ `a_Timeout_add(0.0, a_UIcmd_close_bw)`
  des menus) + `UiCmd::expect_done` exposé (entrée `nav.h`
  nécessaire à capi — chrome-4).

### Test gap analysis (ajout 3)

19. **DialogRunner réel** : la factory native de fenêtres
    modales reste à câbler côté app (`NullDialogRunner` en
    défaut — retour cancel ≙ fenêtre fermée, comportement sûr).
20. **DocOps réel** : `form_submit`/`reset`/`display_hiddens` et
    `load_images` attendent l'objet document `dillo-html`
    (chrome-4+ ; les tests utilisent un recorder).
21. **Popup natif** : le modèle `Popup` est dessiné par le shell
    natif (rendu chrome) — la sémantique d'activation est testée
    unitairement.

---

## Ajout 4 — chrome-4 : `paths.cc`, `control.c`, glue runtime, `dillo.cc`, `dilloc.c`

### Nouveaux modules couverts

- `src/paths.cc` → `paths.rs` : chemins `~/.dillo/*`
  (dillorc/cookies/hsts/bm/cert/keysrc/dpid_comm_keys), verbatim
  `a_Paths_init`+`freeall` ; verbatim `dGethomedir()`≙env
  `HOME`/`USERPROFILE` (Windows : pas de `getpwuid` amont non
  plus sous Win32 — cohérent).
- `src/IO/control.c` → `dillo-net::control` : machine d'état
  verbatim (ligne de commande texte `cmd ...` → ops), seams
  `CtlIo`/`CtlOps` ; `CtlListener` AF_UNIX `~/.dillo/ctl/<pid>`
  `cfg(unix)` ≙ `ENABLE_CONTROL_SOCKET` (le `#else` no-op est
  verbatim amont), `poll` pour les requêtes en attente,
  `wait` différé via `timeout`.
- `dilloc.c` → `dillo/src/dilloc.rs` : client verbatim —
  forward stdin, code retour = 1er octet de la réponse.
- `src/dillo.cc` `main` → `dillo/src/main.rs` : ordre verbatim
  des `a_*_init` (paths→prefs(dillorc)→keysrc→domainrc→capi→
  dicache→cookies→hsts→auth→control→StyleEngine UA→
  `gen_about_keys`→réglages dw→overrides CLI→factory moteurs
  tokio→fenêtre initiale→boucle→`*_freeall`) ; `Options`/
  `getCmdOption`/`makeStartUrl` (`F_OK`→`file:/`) verbatim ;
  `-x` xembed rejeté (X11-only amont de fait) ; `-geometry`
  verbatim.
- glue runtime : `dillo-browser::doc` (dispatch MIME→viewers ≙
  `a_Web_dispatch_by_type` + `Html_callback`/`Plain_callback`,
  `DocImageFactory` ≙ `a_Image_new_with_dw`, file `GlueOp` ≙
  appels synchrones amont reportés au tick),
  `dillo-browser::glue` (`ShellGlue` : événements→chrome
  verbatim `UI::handle`/`CustTabs::handle`/`win_cb`, pompe
  réseau `capi_poll`+`poll_timeouts`+`Control::poll` par tick,
  `wake_after` ≙ `Fl::wait(0.01)`, `CtlOps`, `WinOp` fenêtres
  natives), `DwApp` multi-fenêtres (`DwWin` ≙ `Fl_Double_Window`,
  tombstones ≙ ids stables, `ControlFlow::WaitUntil` ≙
  `Fl::wait(timeout)`).

### Vérifications verbatim effectuées

- `control.c` : 10 tests de machine d'état (ping/pong, dump/
  hdump/load, quit, wait immédiat/différé/timeout, unknown).
- `dillo.cc` : smoke test — démarrage réel `about:splash`,
  boucle 6 s sans crash, avertissements verbatim « Using
  internal defaults » en l'absence de fichiers de conf.
- `RefCell` seam capi : les seams lisibles pendant callbacks
  réseau réentrants (`stack_size`/`top_url`) prennent un
  snapshot `top` — verbatim amont lit `bw->nav` par pointeur.

### Test gap analysis (ajout 4)

22. **Interop dpid réelle** : le chemin complet
    `dpi.cc`→`dpi_ccc`→dpid→plugin n'est testé de bout en bout
    que via mocks in-process (pas de socket AF_UNIX sous
    Windows pour `dilloc`/plugins — voir A12 L5).
23. **`DialogRunner` natif** : idem ajout 3, point 19.
24. **`DocOps` réel** : idem ajout 3, point 20.

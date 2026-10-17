# Audit A12 — Sous-système DPI (rapport)

**Mode** : `rapport`.
**Périmètre audité** : `dpip/dpip.c` (framing), `dpid/*` (démon,
socket dir, registry, dpidc), `dpi/*` (8 plugins), `src/IO/dpi.c`
(client navigateur), `src/dpiapi.c` (frontière — côté browser
couvert par A11).
**Rust** : `crates/dillo-net/src/{dpip,dpi}.rs`,
`crates/dillo-dpi/src/{dpip,dpiutil,dpid,dpid_common,cookies,
bookmarks,dlitem}.rs` + `src/bin/{hello,datauri,vsource,file,ftp,
cookies,bookmarks,dpid,dpidc}.rs`,
`crates/dillo/src/bin/dpi-downloads.rs`.
**Hors périmètre** : `dpi/downloads.cc` côté FLTK→porté natif
(écart structurel documenté ci-dessous, pas une omission) ;
`src/cookies.c`/`src/bookmark.c` clients → A02/A11.

## Cartographie

```
navigateur                          dpid (Rust bin)
  capi::dpi_ccc ──TCP 127.0.0.1──► accept ──► registry services
  (src/IO/dpi.c)   auth+tag          │           (« * » wildcards)
      │                            ▼         spawn-on-demand
      │◄─ fd relay (« FD ») ── plugin.*.dpi (stdin=socket héritée)
      ▼                                │
  dpi.rs tokeniseur tag/raw          ▼
      │                        Dsh (DpIoTcp/DpIoSock)
      ▼                        read_token + write/flush
  capi 2F → send_page_2eof → cache process_dbuf
```

## Verdict global

**Fonctionnel, framing byte-exact, déviations structurelles
documentées.** Les huit plugins et le démon sont portés ; les
points non fidèles sont des choix ADR-0002 (FLTK→natif) ou des
équivalences POSIX→`std` explicites, pas des dérives.

## Vérifié verbatim (preuves)

- **Framing dpip** : `build_cmd` (`%s`, stuffing `'`→`''`,
  EOT ` '>'`, format contenant `'` → `None` ≙ NULL amont,
  `dpip.c:83-126` ↔ `dillo-net/src/dpip.rs:18`) et
  `get_attr_l` (FSM `SEEK_NAME/MATCH_NAME/SKIP_VALUE/SKIP_QUOTE`,
  quote finale non-stuffée *suivie d'un espace* — quirk amont
  `dpip.c:134+` ↔ `dpip.rs:57`). Tests `build_cmd_stuffs_quotes`,
  `get_attr_verbatim` = vecteurs golden extraits du C.
- **`Dsh`** (côté serveur, `[dpi]::dpip`) : buffer `wrbuf`,
  `flush_sz`, modes raw/tag (`read_token` renvoie tag ou raw
  verbatim `a_Dpip_dsh_read_token`), `check_auth`
  (`cmd='auth'` + secret par servir/shared key) — tests
  `token_tag_partial`, `token_tag_then_raw`, `write_flush`,
  `check_auth_bad_tag`.
- **`src/IO/dpi.c`** (`[net]::dpi`) : `read_comm_keys`/`mksecret`,
  `check_dpid`/`start_dpid` (backoff 250 ms verbatim), connect
  `127.0.0.1` + `TCP_NODELAY` (« avoid delays » verbatim),
  `dpi_ccc` 4 branches : 1B `OpStart`→insert socket+`DpidOK`,
  `OpSend/End/Abort`→relais IO ; 2F tokeniseur `OpSend`,
  `OpEnd/OpAbort` libèrent ; tests `token_tag_split`,
  `conn_validity_flag`, `comm_keys_parse`.
- **`capi`** : `"downloads"` → `cmd=download url= destination=
  user-agent=` verbatim (`capi.c:307-310` ↔ `capi.rs:770-786`) ;
  `web->filename` ≙ `web.filename` (issu de
  `parse_content_disposition` — règles path-traversal portées
  dans `[common]::misc` + tests `../../etc/passwd`).
- **Plugins** : `hello` verbatim ; `datauri` (base64 custom,
  strip hors `[A-Za-z0-9+/=]`, borne absolue `p+5<256`, MIME
  `text/plain;charset=US-ASCII` défaut, page d'erreur verbatim) ;
  `vsource` (numérotation, escaping `<`/`&`, CSS verbatim) ;
  `file` (tri dirs-d'abord, dotfiles cachés, sniffing
  HTML/GIF/PNG/JPEG/texte-vs-binaire, `dpi:/file/toggle`,
  métadonnées `ctime`) ; `ftp` (`wget -S -O -`, `escape_uri_str`
  + `filter_smtp_hack`, retry `/` si 0 octet, 404 verbatim) ;
  `cookies` (`cookiesrc` DEFAULT DENY/ACCEPT/ACCEPT_SESSION,
  host-only vs `.domain`, quirk path `len?len:1`, `Max-Age`>
  `Expires`, dates 2-4 digits — 5e rejeté, 70–99→1900s, LRU,
  `cookies.txt` byte-compatible, 9 tests) ; `bookmarks`
  (format `:sN:`/`sN url title` byte-identique, parse laxiste
  verbatim `modify_move`/`modify_delete` sans `=` exigé,
  normalisation ≥1 section, 5 tests).
- **`downloads.cc`** : `dlitem.rs` porte `DLItem` verbatim —
  argv `wget -U ua -c --load-cookies ~/.dillo/cookies.txt
  -O dest url-escapée`, FSM dot-progress `^ `/`^[0-9]+K`,
  `escape_tooltip` (`@`→`@@`), `secs2timestr`, rates 2 s/moyenne/
  ETA, `update_size` (premier octet → init_time+titre),
  `abort_dl`→kill (5 tests).

## Déviations documentées (pas des bugs)

- **D1 — `file.dpi`/`bookmarks.dpi`/`cookies.dpi`** : boucle
  `select` amont → accept non-bloquant + **un thread bloquant
  par client** (Rust safe, pas de `dup2`/fd juggling).
  Observablement identique : connexions concurrentes, ordre des
  réponses par-connexion inchangé. Les plugins restent
  `cfg(unix)`-orientés (stdin = socket héritée) comme amont.
- **D2 — `dpid`** : boucle `select` → tâches tokio (`accept`
  par socket + `signal` + timeout idle) ; `SIGCHLD` géré par
  `tokio::signal` au lieu du handler POSIX — plugin zombie →
  statut perdu mais pas de blocage (amont : `waitpid` dans
  le signal handler).
- **D3 — `downloads.dpi`** : fenêtre FLTK → winit + softbuffer
  + tiny-skia (ADR-0002). Géométrie verbatim (420×85, items
  400×70, colonnes verbatim) ; tooltip `prTitle` → overlay au
  survol ; `fl_choice` d'abort → dialogue natif en ligne.
  Logique téléchargement (wget, FSM log, rates) inchangée.
- **D4 — `Dpi` socket dir** : AF_UNIX verbatim POSIX ; hors
  Unix les serveurs dpi affichent l'erreur amont équivalente
  (plugins = POSIX-first upstream, ADR §9 Windows).

## Lentilles L1–L5

- **L1 fuzzing** : `get_attr_l`/`read_token` bornés, tags
  tronqués → `None` (verbatim NULL) ; `datauri` rejette les
  URI >256 chars au bon endroit ; cookies/bookmarks parseurs
  laxistes amont conservés. **Gap** : pas de fuzzer formel —
  les vecteurs golden couvrent stuffing/EOT/malformé.
- **L2 tolérance** : plugin répondant n'importe quoi →
  `Dsh`/`dpi_ccc` dégradent (`status()`/close), jamais de
  panic ; `#[forbid(unsafe_code)]` + `Result`/`Option`.
- **L3 cycle de vie** : spawn-on-demand dpid au premier accès
  (`Dpi_blocking_start_dpid` verbatim backoff) ; plugin crashé →
  socket EOF → `log_done`/`DpidERROR` remontés.
- **L4 concurrence** : plugins = processus séparés, serveurs
  multi-clients par threads bloquants (D1) — ordre par
  connexion préservé ; le navigateur reste mono-écrivain
  (chunks via canaux).
- **L5 parité wire** : vecteurs golden byte-exact pour
  `build_cmd`/`get_attr`/`cookies.txt`/bm-file/`file` HTML ;
  **pas de test d'interop C↔Rust réel** — aucun binaire C
  compilé sur l'hôte Windows (pas de toolchain autotools).
  Risque résiduel : framing relu octet par octet (verbatim),
  mais une interop vivante reste à faire sous Linux.

## Non-bugs / notes

- `check_auth` est un `bool` (amont `< 0` = échec) — mapping
  `!ok` ≙ `< 0`, identique observablement.
- `DpiBye` ignoré verbatim (`MSG got DpiBye, ignoring`).
- `download` sans `url`/`destination`/`user-agent` → drop
  verbatim (`Failed to parse` → `goto end`).

## Suites (apply éventuel)

1. Test d'interop vivant C↔Rust sous Linux (plugin C amont
   ↔ daemon Rust et réciproque) — non faisable sous Windows.
2. `dpi-downloads` : vérifier le comportement Escape/close
   avec plusieurs téléchargements concurrents réels.

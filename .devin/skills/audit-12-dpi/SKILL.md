---
name: audit-12-dpi
description: "[V8] Audit sémantique vs Dillo — Protocole dpi, démon dpid, plugins (file, datauri, downloads, bookmarks, cookies, vsource, ftp, hello)"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A12 — Sous-système DPI

**Périmètre** : `dpip/dpip.c` (framing **byte-exact**), `dpid/*`
(démon, socket dir, registry), `dpi/*` (plugins), `src/IO/dpi.c` +
`src/dpiapi.c` (côté client) — interop C↔Rust exigée (ADR-0004).

## Fichiers

- **Upstream** (`$DILLO_ROOT`→`D:/Projet/dillo`) : `dpip/dpip.{c,h}`,
  `dpid/{main,dpid,dpid_common,dpi,dpi_socket_dir,dpidc,misc_new}.c`,
  `dpi/{dpiutil,bookmarks,cookies,datauri,downloads,file,ftp,hello,
  vsource}.{c,cc}`, `src/IO/dpi.c`, `src/dpiapi.c`, `devdoc/Dpid.txt`
- **Upstream tests** : usage réel ; `dpi/hello.c` = plugin de test
- **Rust** : `crates/dillo-dpi/src/{proto,daemon,sockdir,util,
  plugins/*}.rs`, `crates/dillo-net/src/dpi.rs`,
  `crates/dillo-browser/src/dpiapi.rs`

## Checklist spécifique

- [ ] `dpip` framing : format `a_Dpip_*` — délimiteurs, escaping
      (`dpiutil` escape/unescape), attributs `key=value` —
      **byte-exact : un plugin C amont doit fonctionner avec le
      client Rust et vice-versa**
- [ ] `dpid` : socket dir discovery (`dpi_socket_dir`), enregistrement
      services `*:` (wildcards), spawn-on-demand d'un plugin absent,
      `dpidc` register/unregister
- [ ] `src/IO/dpi.c` : négociation `dpi://` côté navigateur,
      multiplexage d'un plugin entre requêtes
- [ ] `dpi/file.c` : listing répertoires (HTML généré — format
      identique), tri, liens, escaping des noms
- [ ] `dpi/datauri.c` : `data:` parsing (base64, charset, mime)
- [ ] `dpi/downloads.cc` : progression, annulation, nom fichier
      `Content-Disposition` (cf. `test/unit/disposition.c`)
- [ ] `dpi/bookmarks.c` + `dpi/cookies.c` : accès fichiers
      partagés — même format que côté navigateur
- [ ] `dpi/vsource.c` : view-source (highlight? plain ?)
- [ ] `dpi/ftp.c` : FTP client (PASV, listings → HTML)
- [ ] `dpiutil` : `escape_uri_str`, filtrage caractères —
      vecteurs byte-exact
- [ ] Cycle de vie plugin : crash plugin → dpid relance ? signale ?
      timeout d'inactivité ?
- [ ] Windows : AF_UNIX vs named-pipe, spawn sans shell
      (`CreateProcess` quoting)

## Protocole d'audit — deux modes

Défaut **`rapport`**, `apply` sur rapport relu.

### Analyse — 4 étapes + 5 lentilles

1. Cartographie Rust : proto lib, daemon, chaque plugin.
2. Upstream : `dpip.c` octet par octet, `dpid.c` registry,
   `Dpid.txt` protocole.
3. Lentilles : **L1** fuzzing (messages tronqués, attributs
   malformés, URLs géantes vers plugins) · **L2** tolérance
   (plugin qui répond n'importe quoi — client ne crashe pas) ·
   **L3** cycle de vie (plugin zombie, socket dir périmée,
   spawn race) · **L4** concurrence (plugins = processus ;
   plusieurs clients simultanés, ordre des réponses) · **L5**
   parité wire (**tests interop réels** : plugin C ↔ daemon Rust,
   daemon C ↔ client Rust).
4. Test gap analysis.

### Mode `rapport`

5. `docs/audits/reports/A12_dpi.md` — verdicts+preuves, déviations
   sketchées, frontière (IO/dpi côté net, cookies/bm partagés
   A02/A11), non-bugs.
6. Commit pathspec-limited. 7. STOP.

### Mode `apply`

8. Worktree `C:/Projet/wt/dillorust-a12` branche `audit/12-dpi`.
9. Fixes + tests : **test d'interop obligatoire** si binaire C
   disponible, sinon vecteurs golden extraits du C.
10. Gates fmt/clippy/test/doc ; `docs/audits/changelog/A12.md` ;
    PORTING.md.
11. Commit → rebase → merge `--no-ff` → cleanup → `## Résultat`.

## Pièges connus

- Le protocole est texte mais **délimiteurs subtils** : lire
  `a_Dpip_build_cmd`/`sh*` — ne pas deviner depuis un exemple.
- `dpi_socket_dir` : upstream crée un répertoire `/tmp/...` ou
  `~/.dillo/...` ? Permissions et cleanup au démarrage.
- `file.c` génère du HTML : la sortie doit être quasi identique
  (utilisateurs/scripts y sont habitués).
- `Content-Disposition` filename : règles `disposition.c` —
  sécurité (path traversal) portée fidèlement.
- Spawn plugin : quoting args, PATH search — différences
  Windows/POSIX à isoler dans `DpiTransport`.

---
name: audit-11-browser-glue
description: "[V7] Audit sémantique vs Dillo — Glue navigateur : cache, web, nav, history, bookmarks, prefs, actions, bw"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A11 — Glue navigateur : cache, fenêtres, prefs, navigation

**Périmètre** : `src/{cache,capi,bw,web,nav,history,bookmark,
prefs,prefsparser,paths,actions,bitvec,klist,misc,colors,version}`
— l'ossature applicative entre réseau, moteur et chrome.

## Fichiers

- **Upstream** (`$DILLO_ROOT`→`../dillo`) : fichiers ci-dessus
  + `dillorc` (defaults = spec), `devdoc/{Dillo,Cache,CCCwork,
  HtmlParser}.txt`
- **Upstream tests** : `test/unit/*` (bitvec…), usage réel
- **Rust** : `crates/dillo-browser/src/{cache,capi,bw,web,nav,
  history,bookmark,prefs,paths,actions}.rs`,
  `crates/dillo-common/src/{bitvec,klist,misc,colors}.rs`

## Checklist spécifique

- [ ] `cache.c` : state machine entry (CA_Init…), client queue,
      redirect handling, entry réutilisée pendant chargement,
      purge — CCCwork.txt est la spec des chaînes de callbacks
- [ ] `capi.c` : API client (open/abort/progress), primary vs
      secondary clients
- [ ] `bw.c` : BrowserWindow = onglet ? mapping nav stack par bw,
      images-on/off, multi fenêtres
- [ ] `nav.c`/`history.c` : pile avant/arrière exacte (redirect
      remplace ou empile ?), position scroll restaurée, `about:`
      exclus de l'historique ?
- [ ] `bookmark.c` : format fichier bm (`s:`/`--` sections) —
      byte-exact, ordre préservé, doublons
- [ ] `prefsparser` + `prefs.c` : **chaque clé `dillorc` avec son
      type et sa valeur par défaut** — table complète portée ;
      clés inconnues ignorées comme amont
- [ ] `paths.c` : `~/.dillo/` discovery, création, sous-dossiers
      (dpi sockets, tmp) — chemins Windows équivalents ?
- [ ] `actions.c` : actions UI (reload, save, zoom texte?) vers
      bonnes cibles ; zoom = re-style ou scale ?
- [ ] `colors.c` : table noms→RGB exhaustive + `a_Color_parse`
      lenient (#rgb, noms case-insensitive)
- [ ] `misc.c` : `a_Misc_*` (encode entities, content-type parse,
      hex…) — chaque helper recensé
- [ ] `dpiapi.c` : appels sortants vers plugins (cookies/bm via
      dpi ?) — frontière A12

## Protocole d'audit — deux modes

Défaut **`rapport`**, `apply` sur rapport relu.

### Analyse — 4 étapes + 5 lentilles

1. Cartographie Rust : qui possède quoi (bw→nav→cache→web), flux
   d'événements vers chrome.
2. Upstream : `cache.c` + `CCCwork.txt` (callback chains =
   architecture), `dillorc` defaults exhaustifs.
3. Lentilles : **L1** bornes (fichiers prefs/bm corrompus, URLs
   géantes, historique profond) · **L2** tolérance (pref invalide =
   default + warning, bm ligne cassée = skip) · **L3** cycle de vie
   (abort pendant chargement : chaîne cache démontée dans l'ordre,
   clients notifiés) · **L4** event loop (progress callbacks → UI,
   pas de starvation) · **L5** parité observable (fichiers
   byte-exact, comportement navigation, defaults dillorc).
4. Test gap analysis.

### Mode `rapport`

5. `docs/audits/reports/A11_browser-glue.md` — verdicts+preuves,
   déviations sketchées, frontière (net A01, dpi A12, UI A10),
   non-bugs.
6. Commit pathspec-limited. 7. STOP.

### Mode `apply`

8. Worktree `../wt/dillorust-a11` branche
   `audit/11-browser-glue`.
9. Fixes + tests : golden files (dillorc parsed, bm round-trip,
   cookies) par déviation.
10. Gates fmt/clippy/test/doc ; `docs/audits/changelog/A11.md` ;
    PORTING.md.
11. Commit → rebase → merge `--no-ff` → cleanup → `## Résultat`.

## Pièges connus

- `CCCwork.txt` décrit la chaîne producteur→filtre→consommateur :
  l'ordre des callbacks Link/Bck est structurel — le pipeline
  Rust typé doit le reproduire.
- `dillorc` : valeurs par défaut = spec comportementale ; une
  default différente change le rendu (fontes !).
- Cache : les clients secondaires attachés mid-load reçoivent le
  début depuis le début — mécanisme amont exact.
- `bw` + redirect : l'entrée nav est remplacée par l'URL finale —
  vérifier dans nav.c, ne pas supposer.
- `prefs` reload : hot-reload supporté amont ou seulement au
  démarrage ? Reproduire.

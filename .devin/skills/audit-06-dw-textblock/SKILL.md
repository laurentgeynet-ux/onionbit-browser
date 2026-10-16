---
name: audit-06-dw-textblock
description: "[V4] Audit sémantique vs Dillo — Textblock : word wrap, line breaking, hyphenation, iterators, find/selection"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A06 — Textblock : wrapping, césure, itérateurs

**Périmètre** : `dw/{textblock,textblock_linebreaking,
textblock_iterator,alignedtextblock,hyphenator,ruler,bullet,
listitem,findtext,selection}` + `lout/unicode` — **le widget le
plus complexe de dw ; parité au pixel sur les coupures de ligne**.

## Fichiers

- **Upstream** (`$DILLO_ROOT`→`D:/Projet/dillo`) : fichiers listés +
  `devdoc/dw-line-breaking.doc`, `dw-textblock-collapsing-spaces-*.png`,
  `not-so-simple-container.*`, `test/unit/unicode_test.cc`
- **Upstream tests** : `test/dw/*` texte, `test/html/render` textes
- **Rust** : `crates/dillo-dw/src/textblock/*.rs` (+`aligned`, `ruler`,
  `bullet`, `listitem`, `findtext`, `selection`, `hyphenator`),
  `crates/dillo-common/src/unicode.rs`

## Checklist spécifique

- [ ] Line breaking : pénalités/badness de `textblock_linebreaking.cc`
      — algorithme et constantes portés **verbatim** avant tout tuning
- [ ] `Word`/`Line` structs : mesure ascent/descent, espaces collapsés
      (règles exactes `collapsing-spaces` doc), trailing space width
- [ ] Re-wrap incrémental : ajout de mots en streaming → seules les
      lignes affectées re-wrap — frontière avec chunks réseau
- [ ] Hyphenator : `.pat` files (format TeX patterns) parsing,
      priorités impair/pair, insertion soft-hyphen — données
      `hyph-*.pat` copiées en test data
- [ ] `vertical-offset`, `text-align` (left/right/center/justify?),
      `text-indent`, `line-height` multiplicateur vs absolu
- [ ] `alignedtextblock` : colonnes alignement (tab stops ?)
- [ ] Itérateur : ordre des `Content` (TEXT/WORD/widget), positions,
      `highlight` → selection mapping
- [ ] `findtext` : incrémental, case-insensitive règles amont,
      wrap-around
- [ ] `selection` : granularité caractère, points d'ancrage après
      re-wrap (dérive des positions !)
- [ ] `ruler`/`hr`, `bullet`/`listitem` : retrait/marqueur placement,
      numérotation ol (types amont)
- [ ] Unicode : `a_Utf8_decode` lenient, break classes, zero-width /
      combining chars traités comment ?

## Protocole d'audit — deux modes

Défaut **`rapport`**, `apply` sur rapport relu.

### Analyse — 4 étapes + 5 lentilles

1. Cartographie Rust : structures Word/Line, point de re-wrap,
   lien avec `Layout` flags.
2. Upstream : `textblock.cc` est gros (~3k LOC) — lire les chemins
   `addText`/`wrap`/`calcWidgetSizes` complets + linebreaking doc.
3. Lentilles : **L1** bornes (mots plus larges que le bloc, blocs
   vides, texte 0-width, overflow de pénalité) · **L2** récupération
   (widget enfant retiré mid-word, hyphenator sans patterns) ·
   **L3** cycle de vie (positions sélection après re-wrap/resize,
   anchors internes) · **L4** incrementalité (re-wrap par chunk —
   le texte arrive mot par mot en streaming réseau) · **L5** parité
   pixel (coupures identiques = même nombre de lignes = même
   hauteur — diff automatique sur golden renders).
4. Test gap analysis.

### Mode `rapport`

5. `docs/audits/reports/A06_dw-textblock.md` — verdicts+preuves,
   déviations sketchées, frontière (font metrics platform — A10),
   non-bugs.
6. Commit pathspec-limited. 7. STOP.

### Mode `apply`

8. Worktree `C:/Projet/wt/dillorust-a06` branche
   `audit/06-dw-textblock`.
9. Fixes + tests : cas minimaux de wrap/hyphenation par déviation ;
   dump de lignes (x,y,width) vs amont.
10. Gates fmt/clippy/test/doc ; `docs/audits/changelog/A06.md` ;
    PORTING.md.
11. Commit → rebase → merge `--no-ff` → cleanup → `## Résultat`.

## Pièges connus

- **Métrique de fontes** : `Platform::TextLayout::width()` pilote
  chaque coupure — une dérive de fonte se propage partout. Isoler
  en mockant les métriques dans les tests.
- Trailing spaces : amont les mesure dans le word puis les
  exclut/inclut selon la ligne — détail facile à rater.
- `badness` overflow : pénalités entières amont, pas de float.
- `.pat` : le format a des conventions (digits, points) — parser
  lenient ou strict comme amont `hyphenator.cc`.
- Sélection après re-wrap : les indices se déplacent — amont
  stocke des positions (widget,index) pas des offsets absolus ?

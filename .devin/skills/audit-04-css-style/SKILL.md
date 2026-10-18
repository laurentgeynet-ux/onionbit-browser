---
name: audit-04-css-style
description: "[V3] Audit sémantique vs Dillo — CSS parser, cascade & style engine (cssparser.cc, css.cc, styleengine.cc)"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A04 — CSS parser, cascade & style engine

**Périmètre** : `cssparser.cc` (tokenizer tolérant), `css.cc`
(déclarations, sélecteurs, spécificité), `styleengine.cc` (cascade
UA/user/author, héritage, mapping vers `dw::core::style`) +
`prefsparser` pour les feuilles utilisateur.

## Fichiers

- **Upstream** (`$DILLO_ROOT`→`../dillo`) : `src/cssparser.cc`+
  `.hh`, `src/css.cc`+`.hh`, `src/styleengine.cc`+`.hh`,
  `src/prefsparser.cc` (style-related), `dillorc` (props css)
- **Upstream tests** : pas de suite dédiée → oracle = comportement
  sur `test/html/render` + diff de computed styles
- **Rust** : `crates/dillo-css/src/{parser,css,styleengine,selector}.rs`

## Checklist spécifique

- [ ] Tokenizer : récupération sur règle/déclaration malformée
      (skip jusqu'à `;`/`}` comme amont), commentaires non fermés,
      `@`-règles inconnues ignorées
- [ ] Propriétés supportées : liste exacte amont — une prop
      inconnue est ignorée, pas d'erreur bloquante
- [ ] Valeurs : `!important`, unités (`px`,`em`,`ex`,`%`,`pt`,
      unités relatives aux fontes), couleurs (`#rgb`/`#rrggbb`/
      noms via `colors.c`), `inherit` si amont le gère
- [ ] Sélecteurs : type/classe/id/descendant/child? — **ensemble
      exact amont**, ordre de matching, invalidité → skip règle
- [ ] Spécificité : calcul amont (a,b,c) et tri stable des
      déclarations — égalité → dernière gagne (ordre source)
- [ ] Cascade : UA sheet < user (`dillorc`) < author < author
      `!important` — vérifier l'ordre réel amont
- [ ] Héritage : props héritées vs non, valeurs initiales par
      défaut (`styleengine` nonCssHints, `style_attribute` HTML)
- [ ] `styleengine` : non-CSS hints HTML (`<font>`, `align`,
      `bgcolor`, `width`) — priorité vs CSS amont exacte
- [ ] Reparsing incrémental des `<style>` blocks + re-application
- [ ] `style.css` utilisateur chargé à l'identique des chemins amont

## Protocole d'audit — deux modes

Défaut **`rapport`**, `apply` sur rapport relu.

### Analyse — 4 étapes + 5 lentilles

1. Cartographie Rust : pipeline parse→match→cascade→`Style` dw.
2. Upstream : ordre réel des `DoApply`/`apply` dans styleengine,
   tables de props, valeurs initiales codées en dur.
3. Lentilles : **L1** fuzzing (feuilles adversariales, récursion
   sélecteurs, overflow de spécificité) · **L2** tolérance (règle
   cassée n'invalide pas la feuille) · **L3** cycle de vie (re-style
   après `<style>` différé, éléments ajoutés dynamiquement) ·
   **L4** incrementalité (style appliqué pendant le parsing, pas
   en batch final) · **L5** parité observable (computed style →
   `dw::core::style::Style` identique → même rendu ; dump
   comparatif recommandé).
4. Test gap analysis.

### Mode `rapport`

5. `docs/audits/reports/A04_css-style.md` — verdicts+preuves,
   déviations sketchées, frontière (quels champs `Style` sont
   touchés — lien A05/A06), non-bugs.
6. Commit pathspec-limited. 7. STOP.

### Mode `apply`

8. Worktree `../wt/dillorust-a04` branche `audit/04-css-style`.
9. Fixes + tests : pour chaque déviation, feuille minimale + dump
   de computed style attendu.
10. Gates fmt/clippy/test/doc ; `docs/audits/changelog/A04.md` ;
    PORTING.md.
11. Commit → rebase → merge `--no-ff` → cleanup → `## Résultat`.

## Pièges connus

- Dillo CSS = sous-ensemble CSS2.1 **avec des choix propres**
  (ex. `display` partiel, pas de position:fixed complet avant
  certaines versions) — vérifier le code, pas la doc CSS.
- Ordre source dans la cascade : le tri amont est-il stable ?
  Un tri instable Rust inverse des déclarations à spécificité égale.
- `!important` utilisateur vs author : ordre réel dans `css.cc`.
- Attributs HTML de présentation = « non-CSS hints » avec poids
  spécifique — souvent mal reproduit.
- Valeurs initiales : codées en dur dans `styleengine.cc`, pas dans
  une feuille UA — les recopier exactement.

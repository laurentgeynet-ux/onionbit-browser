---
name: audit-08-dw-oof
description: "[V5] Audit sémantique vs Dillo — Out-of-flow : floats, positioning abs/rel/fixed, stacking contexts"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A08 — Out-of-flow managers & stacking

**Périmètre** : `dw/{outofflowmgr,oofawarewidget(+iterator),
ooffloatsmgr,oofpositionedmgr,oofposabs{,like}mgr,oofposrelmgr,
oofposfixedmgr,stackingcontextmgr,regardingborder}` — managers
interdépendants, ordre d'allocation, z-order — **le sous-système
le plus délicat de dw**.

## Fichiers

- **Upstream** (`$DILLO_ROOT`→`D:/Projet/dillo`) : tous les fichiers
  `dw/oof*.cc` + `devdoc/dw-out-of-flow*.doc`,
  `dw-floats-01.png`, `dw-stacking-context.doc`, `dw-fixed-positions.doc`
- **Upstream tests** : `test/dw/dw_float_test.cc`, render tests
  floats/positions
- **Rust** : `crates/dillo-dw/src/oof/*.rs`, `stacking.rs`,
  `border.rs`

## Checklist spécifique

- [ ] Générateurs GBFF (`getBorder…`): enregistrement des widgets
      OOF et ordre d'appel des managers — dépendances croisées
      (float↔float, abs dans rel…)
- [ ] Floats : placement gauche/droite, collision float↔float,
      `clear`, texte qui s'écoule autour (extremes du textblock
      ajustés par le floats mgr — frontière A06)
- [ ] `position:absolute` : containing block = plus proche ancêtre
      positionné (règles amont), `left/top/right/bottom` auto
- [ ] `abs-like` vs `abs` : distinction amont exacte
- [ ] `position:relative` : décalage visuel vs allocation logique
- [ ] `position:fixed` : relatif viewport, repositionné au scroll
- [ ] Stacking : z-index, contextes imbriqués, ordre de peinture
      (arrière-plan→floats→inline→positionnés par z) — `dw-stacking
      -context.doc` est la spec
- [ ] `regardingborder` : effets sur les bordures adjacentes
- [ ] Redimensionnement viewport : re-layout des fixed/abs en
      cascade — déclencheurs identiques
- [ ] `oofawarewidget_iterator` : les OOF apparaissent-ils dans
      l'ordre de contenu ou de peinture ?

## Protocole d'audit — deux modes

Défaut **`rapport`**, `apply` sur rapport relu.

### Analyse — 4 étapes + 5 lentilles

1. Cartographie Rust : hiérarchie des managers, qui appelle qui,
   quand.
2. Upstream : chaque `oof*mrg.cc` lu + devdocs `dw-out-of-flow*`
   — l'ordre d'allocation est la spec.
3. Lentilles : **L1** bornes (floats plus larges que le conteneur,
   positions extrêmes/négatives, z-index extrêmes) · **L2**
   récupération (widget OOF sans containing block positionné —
   fallback amont) · **L3** cycle de vie (containing block détruit
   avant le widget positionné ; re-génération des OOF au resize) ·
   **L4** event loop (fixed repositionné au scroll sans full
   relayout — vérifier) · **L5** parité pixel (ordre de peinture =
   z-order visible ; positions finales à l'entier près).
4. Test gap analysis.

### Mode `rapport`

5. `docs/audits/reports/A08_dw-oof.md` — verdicts+preuves (code +
   devdoc), déviations sketchées, frontière (textblock floats A06,
   platform draw order A10), non-bugs.
6. Commit pathspec-limited. 7. STOP.

### Mode `apply`

8. Worktree `C:/Projet/wt/dillorust-a08` branche `audit/08-dw-oof`.
9. Fixes + tests : scènes minimales floats/abs/z-index vs amont.
10. Gates fmt/clippy/test/doc ; `docs/audits/changelog/A08.md` ;
    PORTING.md.
11. Commit → rebase → merge `--no-ff` → cleanup → `## Résultat`.

## Pièges connus

- Les managers se passent la main via callbacks du widget aware —
  porter la hiérarchie d'appels, pas refacto en un seul manager.
- Floats et line breaking interagissent : le textblock consulte le
  floats mgr pour chaque ligne — point d'intégration subtil.
- `stackingcontext` : un contexte est créé par quels critères
  exacts ? (position+z-index≠auto, opacity?…)
- Destruction d'un ancêtre → OOF déplacés vers parent suivant —
  ordre amont.

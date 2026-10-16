---
name: audit-05-dw-core
description: "[V4] Audit sémantique vs Dillo — dw core : widget tree, layout engine, size negotiation, draw/damage"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A05 — dw core : arbre de widgets & moteur de layout

**Périmètre** : `dw/{types,style,core,platform,view,widget,layout,
iterator,events,ui,tools}` — contrat de négociation de taille
`getExtremes`/`sizeRequest`/`setSize`/`draw`, allocation, damage
rects, anchors, ordre des appels — **parité pixel exigée**.

## Fichiers

- **Upstream** (`$DILLO_ROOT`→`D:/Projet/dillo`) : `dw/types.cc`,
  `style.cc`, `widget.cc`, `layout.cc`, `iterator.cc`, `ui.cc`,
  `tools.cc`, headers associés + `devdoc/dw-*.doc` (**spec interne,
  lecture obligatoire** : `dw-overview`, `dw-grows`,
  `dw-widget-sizes`, `dw-size-request-pos`, `rounding-errors`,
  `dw-layout-views`, `dw-interrupted-drawing`)
- **Upstream tests** : `test/dw/dw_*` (drivers FLTK → à convertir
  en tests headless)
- **Rust** : `crates/dillo-dw/src/{types,style,widget,layout,
  iterator,events,platform,view,ui,tools}.rs`

## Checklist spécifique

- [ ] `Extremes`/`Allocation`/`Rect` : types entiers, saturation,
      arrondis — cf. `devdoc/rounding-errors.doc` (**exact**)
- [ ] Protocole de layout : `sizeRequest`→`getExtremes`→
      `setSize`→`setAscent/Descent`→`draw` — ordre, appels redondants
      tolérés, caches d'extremes invalidés quand amont le fait
- [ ] `markSizeChange`/`markExtremesChange`/`queueDraw` : propagation
      parent/child identique, coalescence des flags
- [ ] Widget needsResize/needsAllocate/extremesChanged flags —
      machine d'état des flags reproduite bit pour bit
- [ ] Damage : `queueDrawArea` intersecte l'allocation, remontée au
      Layout, fusion/clip — même granularité (perf + redraw visible)
- [ ] Anchors : `layout->setAnchor` résolution différée si widget
      pas encore posé
- [ ] `iterator` : ordre de traversée (content order vs widget
      order), `highlight`/`getAllocation` semantics
- [ ] Interruptible layout : upstream peut interrompre/reprendre
      un layout (idle slices) — `Layout::queueResize` timing
- [ ] Style : `createStyle` héritage, attrs par défaut, Font/
      Color/Imgbuf attrs partagés (RefCount → Rc)
- [ ] Widget destruction : retrait de l'arbre, OOF cleanup, anchors
      pointant vers widget mort — ordre amont

## Protocole d'audit — deux modes

Défaut **`rapport`**, `apply` sur rapport relu.

### Analyse — 4 étapes + 5 lentilles

1. Cartographie Rust : trait `Widget`, variantes dispatch, flags,
   relation Layout↔platform view.
2. Upstream : `layout.cc`/`widget.cc` lus en entier + devdocs ;
   chaque invariant des .doc est un item de checklist implicite.
3. Lentilles : **L1** bornes (tailles extrêmes, widgets 0-size,
   overflow allocation, deep nesting) · **L2** erreurs (widget
   retiré mid-layout, draw hors allocation — amont clip ou
   ignore ?) · **L3** cycle de vie (destruction pendant queueDraw,
   anchors pendants, itérateurs invalidés) · **L4** event loop
   (layout interrompu par chunks réseau — requêtes obsolètes
   invalidées par génération) · **L5** parité pixel (**tout écart
   d'arrondi est une déviation** — golden renders headless).
4. Test gap analysis.

### Mode `rapport`

5. `docs/audits/reports/A05_dw-core.md` — verdicts+preuves
   (`fichier.rs:ligne` vs `dw/widget.cc:ligne` + devdoc ref),
   déviations sketchées, frontière (platform trait, textblock A06,
   table A07, OOF A08), non-bugs.
6. Commit pathspec-limited. 7. STOP.

### Mode `apply`

8. Worktree `C:/Projet/wt/dillorust-a05` branche `audit/05-dw-core`.
9. Fixes + tests : golden layouts minimaux (dump allocation tree
   vs amont) par déviation.
10. Gates fmt/clippy/test/doc ; `docs/audits/changelog/A05.md` ;
    PORTING.md.
11. Commit → rebase → merge `--no-ff` → cleanup → `## Résultat`.

## Pièges connus

- **Arrondis entiers** : la doc `rounding-errors` explique où amont
  accepte des erreurs ±1 volontairement — les reproduire, ne pas
  « corriger ».
- `getExtremes` appelé sur des sous-arbres non-aloués : caches OK ?
- Flags cumulatifs : `markSizeChange` ne propage pas si flag déjà
  posé (anti-boucle) — le supprimer = layouts infinis.
- Référence C++ : héritage multiple `Widget`/`Iterator` —
  en Rust attention aux accès simultanés pendant itération.
- `dw::core::ui::ResourceFactory` : widgets embarqués (form
  controls) — frontière platform.

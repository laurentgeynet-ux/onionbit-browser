---
name: audit-10-platform-ui
description: "[V6] Audit sémantique vs Dillo — Plateforme winit/tiny-skia vs FLTK : views, événements, fontes, chrome UI"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A10 — Plateforme & chrome UI (winit + tiny-skia vs FLTK)

**Périmètre** : `dw/fltk*.cc` (Platform/View/Viewport/UI/imgbuf) →
impl `dillo-platform` ; `src/{ui,uicmd,keys,menu,dialog,findbar,
tipwin,timeout}.cc` → `dillo-browser` chrome — **parité
fonctionnelle** (pas pixel sur le chrome) ; parité métrique fontes
(impact layout).

## Fichiers

- **Upstream** (`$DILLO_ROOT`→`../dillo`) : `dw/fltk*.cc`,
  `dw/platform.hh`, `dw/view.hh`, `src/{ui,uicmd,keys,menu,dialog,
  findbar,tipwin,timeout}.cc`+headers, `keysrc`
- **Upstream tests** : `test/dw/dw_ui_test.cc`, `dw_anchors_test`
- **Rust** : `crates/dillo-platform/src/*`,
  `crates/dillo-browser/src/{ui,uicmd,keys,menu,dialog,findbar,
  tipwin,timeout}.rs`

## Checklist spécifique

- [ ] `Platform` trait : drawLine/Rectangle/Text/Pixbuf/ViewPort —
      sémantique couleur/clip/coordonnées identiques (pixels
      inclusifs/exclusifs FLTK vs tiny-skia !)
- [ ] **Fontes** : `Font` attrs (family/size/weight/style/variant),
      `textWidth`/`ascent`/`descent` — métrique FLTK (fonte système)
      vs cosmic-text : **calibration documentée** ; les golden
      renders tournent sur une fonte embarquée figée (jamais la
      fonte système) et dw est testé avec un mock `Platform` à
      métriques déterministes (ADR-0002 §2)
- [ ] Damage/redraw : `Fl_Widget::damage` régions → softbuffer
      présente quoi ? clip tiny-skia par région — même visuel
- [ ] Scroll : `fltkviewport` scroll mécanisme (blit + redraw
      bande) — équivalent softbuffer (memmove vs redraw total)
- [ ] Événements : souris (click/dbl/drag/motion/wheel), clavier
      (mapping `keysrc` bindings identiques), focus, resize —
      ordre de dispatch vers Layout/widgets
- [ ] Curseurs (types amont), tooltips timing `tipwin`
- [ ] Chrome : tabs (CustTabs look/behavior), barre d'adresse,
      boutons nav, menus contextuels (`menu.cc` items/ordre),
      findbar (incrémental, highlight via `findtext`), dialogs
      (auth, prefs panel `dialog.cc` options complètes)
- [ ] `timeout.cc` : `Fl::add_timeout` one-shot/repeat → mapping
      winit/tokio timers, annulation
- [ ] Clipboard : selection (PRIMARY X11 vs Ctrl-C) — amont copie
      quoi exactement ?
- [ ] DnD/Fichiers, `xembed` (won't port — vérifier aucune dépendance)

## Protocole d'audit — deux modes

Défaut **`rapport`**, `apply` sur rapport relu.

### Analyse — 4 étapes + 5 lentilles

1. Cartographie Rust : trait Platform impl, event loop winit,
   chrome widgets.
2. Upstream : `fltkplatform.cc` primitives + `fltkviewport` scroll ;
   `ui.cc` structure chrome.
3. Lentilles : **L1** bornes (fenêtre 0-size, resize pendant
   layout, events pendant destruction) · **L2** erreurs (device
   lost, clipboard indisponible — graceful) · **L3** cycle de vie
   (timers pendant fermeture, widget détruit avec event en vol) ·
   **L4** event loop (intégration winit↔tokio — le net ne doit
   jamais bloquer le rendu ni inverse) · **L5** parité observable
   (raccourcis `keysrc` identiques, items de menus, comportement
   tabs/findbar — checklist fonctionnelle).
4. Test gap analysis.

### Mode `rapport`

5. `docs/audits/reports/A10_platform-ui.md` — verdicts+preuves,
   déviations sketchées, frontière (métrique fontes → A06, draw
   order → A08), non-bugs (différences visuelles chrome assumées).
6. Commit pathspec-limited. 7. STOP.

### Mode `apply`

8. Worktree `../wt/dillorust-a10` branche
   `audit/10-platform-ui`.
9. Fixes + tests : événements simulés headless (injecter events
   winit vers dw), métriques mock.
10. Gates fmt/clippy/test/doc ; `docs/audits/changelog/A10.md` ;
    PORTING.md.
11. Commit → rebase → merge `--no-ff` → cleanup → `## Résultat`.

## Pièges connus

- **Coordonnées** : FLTK y-down avec rectangles fermés ;
  tiny-skia paths/rect — off-by-one classique sur les bords.
- FLTK `fl_width()` = fonte système ; cosmic-text ≠ mêmes métriques
  → le layout divergera si on ne cale pas (même fonte ? scale ?).
- `Fl::event_*` globals vs modèle winit — le dispatch ciblé widget
  amont (focus chain) est à reproduire.
- Menus/dialogs modaux FLTK bloquent la boucle — reproduire la
  modalité sans nested loop = changement d'architecture (documenter).
- `Fl::add_timeout` persiste-t-il après destruction du widget ?
  Ordre d'annulation amont.

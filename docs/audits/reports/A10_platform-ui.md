# Audit A10 — Plateforme native winit/tiny-skia (rapport)

**Mode** : `rapport`.
**Périmètre audité** : `crates/dillo-platform/src/{platform,view,
ui,app,surface,golden}.rs` vs `dw/fltk{platform,viewbase,viewport,
flatview,ui,complexbutton,imgbuf,misc,preview}.{cc,hh}` +
`dw/fltkcore.hh`.
**Hors périmètre** : `src/ui.cc` (chrome navigateur — phase 6
suite), `xembed.cc` (won't port — X11), `fltkpreview.cc`
(print preview — deferred).

## Cartographie

```
winit WindowEvent ──► DwApp::window_event (≙ FltkViewport::handle)
        │                 │ button/motion/enter/leave/wheel
        │                 ▼
        │            Layout (Rc<RefCell>) ──view.*──► SharedView
        │                 │                          (Weak→Canvas)
        │                 ▼ draw(rect)
        │            SkiaView/Canvas ──► tiny-skia Pixmap
        │                                    │ PlatformSurface
        └─ RedrawRequested ◄── damage ── present() ──► softbuffer
```

## Verdict global

**Fonctionnel avec déviations structurelles documentées.** Toutes
les primitives `Platform`/`View`/`ResourceFactory` sont implémentées
et les métriques `sizeRequest` sont verbatim `fltkui.cc`. Les deux
déviations majeures (clipboard process-local, contrôles
auto-dessinés) sont des choix ADR-0002 tranchés, pas des oublis.

## Vérifié verbatim

- `getDwButtonState` (fltkviewbase.cc:201) : `FL_SHIFT`≙`SHIFT`,
  `FL_CTRL`≙`CONTROL`, `FL_ALT`≙`META`, boutons 1-3 →
  `BUTTON*_MASK` — tests `dw_button_maps_fltk_order` /
  `button_mask_matches_fltk`.
- `event_clicks` : compteur double-clic <500 ms même bouton
  (constante FLTK `FL_DBL_CLICK` implicite) — test
  `clicks_count_double_click`.
- Dispatch `FL_PUSH/RELEASE/MOVE/DRAG/ENTER/LEAVE` →
  `button_event`/`motion_notify`/`enter_notify`/`leave_notify`
  avec coords traduites `view→canvas` — verbatim ordre de
  `FltkViewport::handle` (scrollbars avant contenu).
- `scrollTo` clamp + `scroll(cmd)` (`linesize`≙`scroll_step`) —
  test `scroll_to_clamps_to_canvas`.
- Damage `drawRegion` accumulateur + `damage_all` ≙
  `FL_DAMAGE_USER1`/`EXPOSE` ; premier expose full (verbatim :
  widget né `FL_DAMAGE_EXPOSE`) — test `damage_accumulates_and_drains`.
- `setViewportSize` : barres cachées → origine remise à 0
  (fltkviewport.cc:496-509) verbatim.
- `sizeRequest` de chaque ressource : label `fl_width+2*RELIEF_X`
  × font+`RELIEF_Y` ; entry `fl_width("n")*size+label+reliefs` ;
  multiline colonnes×lignes ; toggles `font.height+relief` ;
  optionmenu max-item ; list max-item+4+scrollbar+row-height ;
  repli sans style `1×(1+0)` — tests `*_size_request_*`.
- `RELIEF_X/Y_THICKNESS` = 3 verbatim.
- Listes : préfixes `@N`/`\a`/depth ≙ `CustBrowser` — verbatim.
- `FltkImgbuf` RGB-only ≙ `ImgbufType::Rgb` ; `setCMap` vide
  verbatim (couvert en dw-1b).
- `Scrollbars` à gauche si `scrollbarOnLeft` — verbatim.
- Radio `groupedWith` ≙ `Fl_Group` partagé — test
  `radio_group_shared`.
- `screenWidth`/`screenHeight` ≙ `Fl::w/h` → moniteur winit
  (fallback 800×600 headless).

## Déviations

### D1 — Clipboard process-local
**Sévérité : majeure (visible) — décision assumée phase 6.**
`Fl::copy`/`Fl::paste` amont ≙ presse-papier OS (X11 PRIMARY +
CLIPBOARD). Le port : `SkiaPlatform::clipboard` in-process —
copier/coller fonctionne *dans la fenêtre* mais pas entre
applications. Correctif prévu : `arboard` (pur Rust, Win/X11/
Wayland/macOS) quand la glue clavier/sélection sera câblée
(phase 6b — `src/ui.cc`).

### D2 — Contrôles auto-dessinés (pas de widgets natifs)
**Sévérité : majeure (architecturale) — décision ADR-0002.**
Amont : `Fl_Input`/`Fl_Choice`/`Fl_Browser` natifs reçoivent
clavier/focus/sélection localement. Port : les ressources
dessinent leur état (relief, caret, item actif) dans le pixmap ;
le clic bouton-1/3 remonte à `Embed`→`emit_clicked` ≙ verbatim,
mais la saisie clavier d'un `Entry`/`TextArea`, la sélection de
texte dans un champ et le popup d'un `OptionMenu` ne sont **pas**
encore câblés — seam clavier via `DwApp::set_key_handler`
(≙ `FL_KEYBOARD → return 0` remonté au parent amont). Travail
restant : routeur de focus clavier dans `dillo-browser` + dessin
du caret/sélection déjà présents dans `ui.rs`.
Conséquence : `<input>`/`<textarea>` affichent et réagissent au
clic ; l'édition au clavier attend la phase chrome.

### D3 — Scrollbars dessinées sans flèches ni gadget natif
Amont : `Fl_Scrollbar` (flèches haut/bas, pouce, track). Port :
track+pouce dessinés dans le pixmap, hit-test par rectangles
(`ScrollbarHit`), page-up/down au clic track, drag du pouce
proportionnel. Pas de boutons fléchés (FLTK « `scrollbar2` »
style non utilisé par Dillo par défaut) — comportement de clic
identique, look minimal. **Cosmétique mineure.**

### D4 — `SharedView` proxie le `Canvas`, pas la `SkiaView`
**Non observable — note d'implémentation.**
Amont : `theLayout` tient `FltkViewport*` non-possédant. Port :
`Layout::attach_view` reçoit `SharedView` (`Weak<RefCell<Canvas>>`).
Le canvas est proxifié plutôt que la vue parce que le layout
rappelle `view.*` *pendant* `Layout::draw` qui tient déjà
`RefMut<SkiaView>` — un proxy sur la vue paniquerait (`RefCell`
double-emprunt). Toutes les méthodes `View` invoquées par le
layout (scroll/damage/canvas-size/cursor/clip) vivent sur le
canvas — équivalent sémantique, test `shared_view_proxies_canvas`
(+ `Weak` mort → no-op ≙ widget détruit).

### D5 — `Pixmap` en `Option` (OOM gracieux)
Amont `Fl_Image_Surface` échoue → `exit(1)` implicite. Port :
`Option<Pixmap>` + `new_pixmap` retourne `None` → draws no-op.
Conforme règle 3 (dégrader, jamais paniquer) ; seul l'écran
reste noir — **amélioration de robustesse**, pas de divergence
fonctionnelle.

### D6 — Calibration fontes `cosmic-text` vs `fl_*`
`fl_width`/`fl_height` FLTK (fontconfig+Xft) vs `rustybuzz`
métriques + `Buffer` cosmic : les polices par défaut et l'arrondi
peuvent différer d'±1 px — les formules `sizeRequest` sont
verbatim, seules les métriques sous-jacentes varient (même classe
de déviation que la calibration DPR — tests dorés à établir en
phase `test/html/render`).

## Reste à faire (phase 6b — chrome)

- `src/ui.cc`/`uicmd.cc`/`keys.cc`/`menu.cc`/`dialog.cc`/
  `findbar.cc`/`tipwin.cc` — non commencés (ROADMAP).
- Focus clavier + saisie `Entry`/`TextArea` (D2).
- Clipboard OS (D1) — `arboard`.
- Popup réel `OptionMenu`/`List` (actuellement dessin inline).

## Risques résiduels

- `ComplexButtonResource` (`<button>` HTML) : `FlatView` partagée
  testée en unitaire mais pas encore exercée end-to-end (attend
  `dillo-browser`).
- `winit` multi-fenêtres : `DwApp` gère une fenêtre ; le support
  multi-onglets/fenêtres est une extension `ApplicationHandler`
  (map `WindowId→(surface,view,layout)`) à faire avec les tabs
  amont (`CustTabs`).

# ADR-0002 — Stack GUI : winit + softbuffer + tiny-skia via `dw::core::Platform`

- **Statut** : accepté · **Date** : 2026-10-16

## Contexte

Dillo dépend de FLTK pour trois choses distinctes :
(1) implémentation de `dw::core::Platform`/`View` (surface de
dessin, événements, clipboard, mesure de fontes) — les `dw/fltk*.cc` ;
(2) widgets du chrome (`src/ui.cc`, `menu.cc`, `dialog.cc`,
`findbar.cc`, `tipwin.cc`) ; (3) la boucle d'événements
(`Fl::run`, timeouts, iowatch).

Contrainte utilisateur : **100 % Rust natif**, donc `fltk-rs`
(binding C++) exclu de la build par défaut.

## Options

| Option | Pour | Contre |
|---|---|---|
| fltk-rs | mapping 1:1 des `fltk*.cc`, chrome gratuit | lie FLTK C++ — viole le « 100 % natif » |
| egui/eframe | pur Rust, productif | modèle immédiat incompatible avec le widget tree retenu de dw ; chrome facile mais layout engine à réinventer par-dessus |
| **winit + softbuffer + tiny-skia** | pur Rust ; correspondance naturelle : `Platform`=trait de dessin, `View`=surface softbuffer, événements=winit | pas de toolkit : chrome (boutons, menus, dialogs) à écrire ; fontes/mesure à assembler (`cosmic-text`) |
| AccessKit/pixels/cairo | — | cairo = C ; pixels = wgpu GPU, surdimensionné |

## Décision

1. `dillo-platform` implémente `dw::core::Platform` (trait Rust
   miroir) sur **winit** (fenêtre, events, clipboard via
   `arboard`/`copypasta` pur-Rust si dispo, sinon shim),
   **softbuffer** (présentation) et **tiny-skia** (raster : lignes,
   rectangles, pixbufs, clipping par régions damage).
2. **Fontes** : `cosmic-text` (pur Rust, shaping+métriques) — la
   métrique de fonte pilote le layout ; exposée via le trait
   `Platform::Font`/`TextLayout` pour garder dw agnostique.
   **Parité de test** : les golden renders se font sur une fonte
   embarquée (ex. DejaVu) figée en test data — jamais sur la fonte
   système — et les tests unitaires dw utilisent un mock
   `Platform` à métriques déterministes. La calibration
   cosmic-text vs métriques FLTK est documentée dans
   `docs/audits/reports/A10` et marquée non-bug si écart assumé.
3. Les `dw/fltk*.cc` ne sont **pas portés 1:1** ; chacun est
   remplacé par l'impl équivalente du trait (lignes `won't port —
   replaced by dillo-platform` dans PORTING.md).
4. Chrome (`ui.cc`, menus, dialogs) : reconstruit en Rust natif dans
   `dillo-browser`/`dillo-platform` — le chrome n'est pas soumis à
   parité pixel, seulement à parité fonctionnelle.
5. Une impl `fltk-rs` optionnelle derrière feature **peut** être
   ajoutée plus tard comme référence de mise au point (feature-gated,
   jamais dans la build par défaut).

## Conséquences

- dw reste 100 % portable/testable headless (tiny-skia offscreen =
  golden renders).
- Travail net ajouté sur le chrome et les fontes — accepté.
- `unsafe` : aucun dans notre code ; `softbuffer`/`winit` gèrent le
  platform-unsafe en dépendance (autorisé, cf. AGENTS §4.1).

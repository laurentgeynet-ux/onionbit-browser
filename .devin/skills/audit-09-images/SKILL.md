---
name: audit-09-images
description: "[V7] Audit sémantique vs Dillo — Images : dicache, imgbuf, décodeurs gif/png/jpeg/webp/svg, décodage par scanline"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A09 — Pipeline images

**Périmètre** : `src/{dicache.c,image.cc,imgbuf.cc,gif.c,png.c,
jpeg.c,webp.c,svg.c}` + `dw/{image.cc,imgrenderer.cc}` — décodage
**incrémental par scanline**, callbacks au widget, scaling —
remplacement des libs C par des crates Rust pures.

## Fichiers

- **Upstream** (`$DILLO_ROOT`→`D:/Projet/dillo`) : fichiers ci-dessus,
  `devdoc/Images.txt`
- **Upstream tests** : `test/dw/dw_imgbuf_mem_test.cc`,
  `dw_images_*.cc`
- **Rust** : `crates/dillo-images/src/{dicache,imgbuf,image,
  decoders/{gif,png,jpeg,webp,svg}}.rs`, `crates/dillo-dw/src/
  {image,imgrenderer}.rs`

## Checklist spécifique

- [ ] `dicache` : state machine DECODE/DBEC/… par entrée, refs
      multiples, purge LRU (politique amont exacte), abort client
- [ ] `imgbuf` : types (RGB/RGBA/indexed), allocation par ligne,
      callback `ImgbufWrite` par scanline — **granularité conservée**
- [ ] GIF : animation (delays, disposal methods, loop count),
      transparence index, interlaced → ordre de présentation amont
- [ ] PNG : tous color types/bit depths que le C gère, interlacing,
      gamma appliqué ou non comme amont
- [ ] JPEG : progressif vs baseline — amont libjpeg affiche le
      progressif ; `jpeg-decoder` aussi ? (sinon documenter)
- [ ] WebP : lossy/lossless/alpha — couverture vs amont
- [ ] SVG : `resvg/usvg` vs nanosvg — **différences de rendu
      attendues → documenter en non-bug ou calibrer**
- [ ] `dw/image.cc` widget : placeholder avant données, alt text,
      scaling `imgrenderer` (filtre utilisé amont ? bilinear ?),
      image maps (`usemap`)
- [ ] Image cassée/mime inconnu : icône broken + alt, comme amont
- [ ] Background images CSS : tiling, position — chemin dw::style
- [ ] Multi-image même URL : un seul dicache entry, N clients

## Protocole d'audit — deux modes

Défaut **`rapport`**, `apply` sur rapport relu.

### Analyse — 4 étapes + 5 lentilles

1. Cartographie Rust : pipeline net→dicache→decoder→imgbuf→widget.
2. Upstream : `dicache.c` state machine + `imgbuf` callbacks ;
   quelles libs font quoi (libjpeg vs libpng features utilisées).
3. Lentilles : **L1** fuzzing (images corrompues — les décodeurs
   Rust doivent dégrader comme les libs C : erreur→broken icon,
   jamais panique ; dimensions 0/géantes) · **L2** tolérance
   (truncation mid-stream = affichage partiel amont) · **L3** cycle
   de vie (widget détruit pendant décodage, dicache entry orpheline,
   abort propre) · **L4** incrementalité (scanlines rendues au fur
   et à mesure — ne pas bufferiser l'image complète) · **L5**
   parité observable (pixels décodés — diff par image de test ;
   animation = même frames/timings).
4. Test gap analysis.

### Mode `rapport`

5. `docs/audits/reports/A09_images.md` — verdicts+preuves, déviations
   sketchées, frontière (cache browser, widget dw), non-bugs
   (ex. différences resvg vs nanosvg justifiées).
6. Commit pathspec-limited. 7. STOP.

### Mode `apply`

8. Worktree `C:/Projet/wt/dillorust-a09` branche `audit/09-images`.
9. Fixes + tests : corpus d'images (copié data only) décodées
   pixel-à-pixel vs sortie C quand possible.
10. Gates fmt/clippy/test/doc ; `docs/audits/changelog/A09.md` ;
    PORTING.md.
11. Commit → rebase → merge `--no-ff` → cleanup → `## Résultat`.

## Pièges connus

- Libjpeg/libpng sont très tolérants aux fichiers tordus ; les
  décodeurs Rust parfois plus stricts → déviations à cataloguer.
- GIF disposal method 2/3 mal implémenté dans beaucoup de libs —
  vérifier vs libgif amont.
- `imgbuf` partagé entre decodeur et renderer : en Rust `Rc`/copie
  par ligne — attention au modèle de propriété.
- SVG : nanosvg est *minimaliste* ; resvg rend « mieux » — c'est une
  déviation visible à documenter, pas un bug.
- Scaling : `imgrenderer` utilise quel filtre ? Si amont =
  nearest/box, ne pas mettre de bilinear « plus joli ».

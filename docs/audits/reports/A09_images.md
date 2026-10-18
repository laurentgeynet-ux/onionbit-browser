# Audit A09 — Pipeline images (rapport)

**Mode** : `rapport` (fixes différés — stratégie « ledger d'abord »,
cf. décision utilisateur du thread A01).
**Périmètre audité** : `crates/dillo-images/src/{dicache,imgbuf,
image,gif,png,jpeg,webp,svg}.rs` vs `src/{dicache.c,image.cc,
imgbuf.cc,gif.c,png.c,jpeg.c,webp.c,svg.c}` + `src/IO/mime.c`
(`[net]::mime`).
**Hors périmètre (non porté, non audité)** : `dw/image.cc`,
`dw/imgrenderer.cc` (widget image, scaling, usemap, broken-icon),
images de fond CSS — frontière dw/UI future.

## Cartographie

```
net::cache ──CA_Send/Close/Abort──> images::dicache_callback
        │                                  │
        │                          entry.decoder (trait)
        │                    gif │ png │ jpeg │ webp │ svg
        │                                  │
        └── set_parms/set_cmap/new_scan/write/close ──►
        imgbuf (Rc<Imgbuf>, rgb_line) ──► image (ImgRenderer trait)
MIME → net::mime::mime_get_viewer → images::register_image_viewers
       (DIC_GIF/PNG/JPEG/WEBP/SVG) → dicache_image
```

## Verdict global

Noyau dicache/imgbuf/image : **conforme** (audit antérieur +
16 tests). GIF : **conforme** (LZW maison porté verbatim, quirks
reproduits). PNG/JPEG/WebP/SVG : **fonctionnels avec déviations
structurelles documentées** — la classe commune « affichage
progressif perdu » est une décision de conception connue
(decode-at-completion vs libs C incrémentales), pas un oubli.

## Déviations

### D1 — Émission progressive des lignes perdue (PNG/JPEG/WebP)
**Sévérité : majeure (visible) — décision acceptée, à corriger si
un push-API pur Rust équivalent émerge.**

Amont : `png_process_data`/`WebPIUpdate`/libjpeg-source-suspendable
émettent chaque ligne dès qu'elle décode — une image s'affiche
ligne à ligne pendant le téléchargement. Rust : `png`/`jpeg-decoder`/
`image-webp` n'exposent pas de push-API avec transformations
(`StreamingDecoder` ne fait ni expand ni strip-16 ni palette — les
réimplémenter reviendrait à réécrire leurs pipelines de
transformations). Choix : décoder à la première tentative réussie
(`IoError(UnexpectedEof)` ≙ suspend), puis émettre toutes les
lignes. Preuves : `png.rs::try_decode`, `jpeg.rs::write`
(garde `buf.ends_with(EOI)`), `webp.rs::write` (garde RIFF-size).
Conséquence observable identique sur fichier complet : pixels
finaux identiques, ordre `dicache_write` 0..h identique.

### D2 — Multi-scan/interlaced : raffinement non reproduit
**Sévérité : mineure-majeure selon corpus.**

- JPEG progressif : amont `buffered_image` + `jpeg_start_output`
  par scan + `a_Dicache_new_scan` → l'image s'affiche d'abord floue
  puis se raffine. Rust : pixels finaux seulement. Le quirk
  `a_Capi_get_flags(url) & CAPI_Completed` gating `buffered_image`
  est du coup inapplicable (`jpeg.rs` doc).
- PNG Adam7 : amont émet chaque ligne plusieurs fois (passes
  libpng + `row_num < previous_row` → `new_scan`). Rust :
  `next_frame` déplié → une écriture/ligne, `new_scan` jamais
  appelé. État final identique.

### D3 — JPEG sans EOI : partiel perdu à la troncature
**Sévérité : mineure.**

Amont : libjpeg émet les lignes déjà reçues même si le flux meurt
sans EOI (`fill_input_buffer`→FALSE = suspend, `CA_Close` clôt
avec le partiel). Rust : sans `FF D9`, le décode n'est tenté qu'à
`CA_Close` (`final_attempt`) ; si `jpeg-decoder` échoue (il échoue
sur les troncatures), rien n'est émis → image absente vs image
partielle amont. `jpeg.rs::write(final_attempt)`.

### D4 — PNG gAMA : précision 8-bit vs 16-bit amont
**Sévérité : mineure (±1 LSB possible).**

Amont applique la correction gamma aux échantillons 16-bit avant
`png_set_strip_16`. Rust applique une LUT 8-bit post-strip
(`png.rs::gamma_lut`, exposant `1/(file_gamma*2.2)` — formule
`png_gamma_8bit_correct`). Pour les PNG 16-bit avec gAMA, des
pixels peuvent différer de ±1. Pour les 8-bit (cas majoritaire),
la LUT est l'algorithme exact de libpng → identique.
À noter : sans gAMA, ni amont ni nous n'appliquons de correction
(`png_get_gAMA` gate — reproduit).

### D5 — SVG : rasteriseur resvg ≠ nanosvg
**Sévérité : non-bug documenté (piège connu du skill).**

Pixels non bit-identiques par nature (antialiasing, interpolation
de Bézier, couverture). Sous-déviations cataloguées :
- `currentColor` : amont injecte `Image->fg_color` (NSVG_RGB) —
  usvg 0.45 ne le permet pas → `currentColor` = valeur usvg
  (défaut : noir/`color` non résolu). `svg.rs` conserve le champ
  `fgcolor` pour référence.
- Sortie premultiplied (tiny-skia) → dé-premultiplié puis formule
  amont `(c*a + bg*(255-a))/0xFF` ré-appliquée : arrondis doubles
  possibles de ±1 sur pixels semi-transparents.
- nanosvg tolère des SVG malformés que `roxmltree` rejette (et
  inversement). `Svg_write` tronqué : identique (attente `"</svg>"`
  verbatim, sous-chaîne octet-exacte).

### D6 — `disposal`/`delayTime` GIF non stockés
**Sévérité : nulle (non-bug).**

Amont les parse dans `Gif_do_gc_ext` (écritures mortes — jamais
lus : pas d'animation timing ni de disposal dans `gif.c`). Le
port les ignore → observable identique. Les GIF animés émettent
leurs frames en scans successifs, comme amont.

### D7 — `image/p` matche `image/png`
**Sévérité : nulle (quirk amont reproduit fidèlement).**

`a_Mime_get_viewer` compare `dStrnAsciiCasecmp(Key, Name, MinSize)`
— borne sur la clé : un type plus court que le nom enregistré
matche quand même. Reproduit dans `mime.rs` + test
`key_shorter_than_name_matches`.

## Lentilles

**L1 fuzzing** — mauvaise signature : GIF→`state=999`, PNG→
`IS_finished`, JPEG→`Error`, WebP→retry puis `IS_FINISHED` au
complet, SVG→silencieux ; dimensions 0/géantes → rejet
`IMAGE_MAX_AREA` partout (tests : `suspicious_dimensions_rejected`,
`bad_signature_*`, `garbage_never_decodes`,
`malformed_svg_emits_nothing`). Aucun panic sur ces chemins.
**L2 troncature** — GIF émet le partiel comme amont ; PNG/JPEG/
WebP/SVG → D3/D1 (partiel perdu — écart de comportement accepté).
**L3 cycle de vie** — take/restore du décodeur hors emprunts
`RefCell` (anti-réentrance, fix antérieur), `abort` ≙ `CA_Abort`
libre, entry orpheline ≙ refcount amont ; `Rc<Imgbuf>` partagé
décodeur/renderer sans copie (piège skill évité).
**L4 incrementalité** — GIF oui ; autres : D1/D2.
**L5 parité pixels** — GIF byte-exact (LZW verbatim) ;
PNG identique hors gamma-16bit (D4) ; JPEG/WebP : pixels produits
par `jpeg-decoder`/`image-webp` (pas de comparaison bit-à-bit vs
libjpeg/libwebp — voir gap) ; SVG D5.

## Test gap analysis

- **Corpus golden manquant** : aucun test pixel-à-pixel vs sortie
  C (pas de `.png/.gif/.jpg` de référence copiés dans
  `tests/data/`). Action suggérée : générer des goldens depuis le
  navigateur amont (`dw_imgbuf`/`test/images`) pour JPEG/WebP —
  les tolérances seront à calibrer (décodeurs ≠ libs C).
- GIF : pas de test interlaced, multi-frame, index transparent
  (cmap `bg_index`→bg), transitions de largeur de code LZW
  (>8 bits), séquence spanning plusieurs lignes (spill), local
  cmap. La structure est verbatim mais le corpus ne l'exerce que
  sur un GIF 1×1 trivial.
- PNG : pas de test 16-bit, palette+tRNS, Adam7, gAMA (LUT).
- JPEG : pas de grayscale/CMYK/progressif.
- WebP : pas de test chemin alpha (composite `/0xFF`).
- SVG : pas de test transparence/dé-premultiply.

## Frontière & recommandations

- `dw/image.cc`, `dw/imgrenderer.cc` : prochain chantier (widget,
  scaling, usemap, broken-icon) — l'audit final des pixels en page
  attend cette couche + `audit-05`.
- Les déviations D1–D3 partagent la même cause racine (absence de
  push-API transformante en pur Rust). Si la parité d'affichage
  progressif devient requise, la voie propre est un
  `TransformingStreamingDecoder` maison au-dessus de
  `png::StreamingDecoder` (+ réimplémentation expand/strip/palette
  — effort élevé) ; pour JPEG/WebP il n'existe pas de base
  incrémentale Rust équivalente.
- Appliquer quand : après `dw::image`, en même temps que les
  apply A11 (même couche d'interconnexion UI).

**Verdict** : prêt pour la couche `dw::image` — aucune correction
bloquante ; D1–D3 = dette de design acceptée et documentée.

---

# Passe 2 (post-port `dw/image` + golden suite)

## Nouveau périmètre audité (était hors-périmètre passé 1)

`crates/dillo-dw/src/{image,imgrenderer,imgbuf}.rs` (port `c5981c5`)
vs `dw/image.cc`, `dw/imgrenderer.cc`, `dw/fltkimgbuf.cc` :

- **widget `Image`** : verbatim — `altText`/`altTextWidth` lazy,
  `markEmpty`→bordure 1px+padding 2 alt-style, `useAltStyle`,
  `clicking`, `linkUnderPointer`/`imageMapLink`, `draw` avec
  clipping-view pour `drawMap`, `get_scaled_buf` requête +
  MAX_DIM verbatim, `drawRow`/`setBuffer` différés via
  `pending_draws`/`pending_resizes` (modèle D cohérent).
- **`ImageMapsList`** : recherche linéaire ≙ HashTable amont (non
  observable), `defaultLink` −1, premier shape contenant le point
  — verbatim (`image.cc:73-87`).
- **`Imgbuf` scaled-buffers** : arbre root→children `Weak` ≙
  `scaledBuffers`/`detachScaledBuf`, `scaled_y`/`backscaled_y`
  verbatim, `copied_rows` anti-multi-passes, `new_scan` reset des
  buffers scalés seulement (quirk verbatim).

## Mises à jour de statut passé 1

- **D5-SVG `currentColor`** : **résolu** en `f9aa4c8` — token
  `currentColor` substitué textuellement par `#rrggbb` (fg_color)
  avant parse usvg ≙ `nsvgParse(str,"px",dpi,NSVG_RGB(fg))`.
- **Golden suite** : corpus `img-aspect-ratio-*`/`float-img-*`/
  `table-td-width-percent-img` exerce le sizing image. **Attention** :
  le harnais compare deux documents rendus par *notre* pipeline —
  il ne valide pas les pixels décodés/scalés contre l'amont.

## Déviation nouvelle

### D8 — Buffers scalés : `SIMPLE` (nearest) vs `BEAUTIFUL_GAMMA` amont
**Sévérité : moyenne (L5 — pixels différents sur toute image
redimensionnée).**

Amont `fltkimgbuf.cc:37` : `scaleMode = BEAUTIFUL_GAMMA` en dur →
`scaleRowBeautiful`/`scaleBuffer` (moyenne de boîte du rectangle
source + LUT gamma `findGammaCorrectionTable(gamma)` en entrée,
`1/gamma` en sortie). Port `imgbuf.rs:158-218` : seul
`scale_row_simple` (nearest-neighbor + copie de ligne) est porté —
le commentaire l'annonce « déviation documentée » mais elle
n'était pas consignée au rapport passé 1.
Effets : downscale et upscale rendent moins lisses/différemment
gamma-corrigés qu'amont ; détectable sur `test/dw/dw_images_
scaled` ou toute page avec `width`/`height` forcés ou CSS.
Fix : porter `scaleBuffer`/`scaleRowBeautiful` + les deux LUT
gamma (données verbatim `findGammaCorrectionTable`), dispatchées
par un `scale_mode` ≙ `BEAUTIFUL_GAMMA`.
Test : image scalée pixel-à-pixel vs un golden généré amont.

## Verdict passe 2

Globalement conforme ; **D8** à ranger dans l'apply consolidé
(pixel-visible). D1–D3 (progressivité), D4 (gAMA 16-bit), D5
restant (rasterizer ≠) reconduits comme dette documentée.

## Résultat

Passe `apply` consolidée exécutée dans le worktree
`C:/Projet/wt/dillorust-p2` :

- **D8** corrigé — `SCALE_MODE = BEAUTIFUL_GAMMA` verbatim :
  `scaleRowBeautiful` (attente `allRootRows` en réduction,
  `scaleBuffer(srcH=1)` en agrandissement, court-circuit
  `copiedRows[sr1]`) et `scaleBuffer` (box-average footprint int,
  gamma `map1/table(γ)` + `map2/table(1/γ)`, cache
  `thread_local` ≙ `gammaCorrectionTables`) portés dans
  `imgbuf.rs`. 5 tests pixel ajoutés (moyenne, correction gamma
  chiffrée, attente progressive des lignes, réplication upscale,
  non-rescale).
- D1–D4 restent des déviations structurelles documentées (pas de
  push-API pure Rust équivalente aux libs C incrémentales) —
  réévaluer si un décodeur push apparaît.

# Audit A06 — Textblock (rapport partiel)

**Mode** : `rapport` (ledger post-port, même stratégie qu'A08).
**Périmètre audité** : `crates/dillo-dw/src/textblock/{mod,words,
breaking,sizing,draw,iter,listitem}.rs` (~9 000 lignes) +
`{hyphenator,selection,findtext,bullet,ruler}.rs` vs
`dw/textblock.{cc,hh}`, `dw/textblock_linebreaking.cc`,
`dw/textblock_iterator.cc`, `dw/alignedtextblock.{cc,hh}`,
`dw/listitem.{cc,hh}`, `dw/hyphenator.{cc,hh}`, `dw/ruler.{cc,hh}`,
`dw/bullet.{cc,hh}`, `dw/selection.{cc,hh}`, `dw/findtext.{cc,hh}`
+ `devdoc/dw-line-breaking.doc`.
**Hors périmètre** : tables (`dw/table*`, A07), `image.cc`/`Img`
widget (consommateur `addWidget` — A09/phase images), golden
renders pixel (harnais headless non écrit — A10), fichiers
`hyph-*.pat` amont non encore copiés en test-data.

## Cartographie

```
WidgetKind::Textblock { tb: Box<TextblockData> }
      │  oof: OofAwareData embarqué (≙ héritage RegardingBorder)
      │  words: Vec<Word> · lines: Vec<Line> · paragraphs
      │  anchors · badness_and_penalty · has_listitem_value
      ▼
Layout::with_tb ── mem::replace ──► (&mut Layout, &mut TextblockData)
      │
words.rs   addText0→splitWord(DIV_CHARS)→calcTextSize / fillWord /
           fillSpace / setBreakOption / addParbreak (propagation
           ancêtre breakSpace) / addWidget / addAnchor / handOverBreak
breaking.rs  wordWrap → processWord → addLine →
             wordWrapWidgetAndHyphenate → hyphenateWord (TeX pats)
sizing.rs  sizeRequestImpl (extrêmes × paragraphes, OOF bordures),
           getExtremesSimpl, sizeAllocateImpl, getAvail*OfChild,
           mark*/resizeDrawImpl, oofSizeChanged, getGenerator*
draw.rs    decorateText/drawWord*/drawSpace/drawLine/drawLevel +
           findWord/findLineIndex*/getWidgetAtPointLevel
iter.rs    TbInFlow ≙ TextblockIterator (OofAwareIterator porteur)
listitem.rs  AlignedTextblock (List Rc≙refcount) + ListItem
selection.rs   DeepIterator ancrage + CharIterator + clipboard
findtext.rs    KMP (case, whitespace-equiv, fwd/back, restart)
hyphenator.rs  Trie + .pat/.exc/.trie loaders
bullet.rs/ruler.rs  leaf widgets, EmptyIterator
```

## Verdict global

**Conforme** sur le périmètre porté : algorithmes de rupture de
ligne (pénalités entières, badness `ratio³`/`TOO_TIGHT`/
`QUITE_LOOSE`/`NOT_STRETCHABLE`), mesures ascent/descent et
espaces collapsés, césure TeX (priorités chiffres odd/even),
justification cumulative (doc rounding-errors), itérateur et
sélection/find-text sont verbatim amont sous le modèle arène.
87 tests dillo-dw verts (dont 4 findtext + 4 selection + 8
bullet/ruler/listitem itérateurs & values) ; workspace complet
vert ; clippy `-D warnings` propre (allows ciblés documentés).

## Déviations (modèle D & verbatim-mort)

### D1 — `Widget*`/`Iterator*` → `WidgetId`/`Box<dyn DwIterator>`
**Sévérité : nulle.** `DeepIterator::new` clone l'itérateur
entrant (verbatim ctor amont `it = it->cloneIterator()`).
`switch_link_to_selection` fait `link.take()` ≙ amont
`delete link; link = NULL` puis ré-assignation — même cycle de
vie observable.

### D2 — `hoverTooltip` : statique globale amont → `thread_local`
**Sévérité : nulle.** Amont `static Widget *hoverTooltip`
(textblock.cc:38) : un seul widget survolé à la fois sur toute
l'appli. `thread_local` préserve la sémantique UI-thread unique
(§10 contrat — jamais de layout cross-thread).

### D3 — `ContentData::Text` sans NUL terminal
**Sévérité : nulle.** Les `strlen` amont sont bornés par la
longueur du `Vec<u8>` — la fin de slice ≙ octet NUL virtuel
(`CharIterator`/`next_glyph` le traitent déjà ainsi).

### D4 — Sections OOF `SL_OOF_REF` mortes
**Sévérité : nulle (vérif amont).** `drawLevel`/`getWidgetAtPoint`
sur `SL_OOF_REF` lisent `word->content.widget` malgré le type
`WIDGET_OOF_REF` — union partagée jamais déréférencée sous
`IMPL_POS=false` (même analyse qu'A08-D1). Porté verbatim, mort.

### D5 — `WidgetKind::Textblock { tb: Box<..> }`
**Sévérité : nulle.** Boxé pour la taille de l'enum (nœud ~64 B
au lieu de ~520 B) ; `with_tb`/`node_tb*` inchangés (deref
transparents). `tb_drop` ≙ `~Textblock` + `~OOFAwareWidget`
(destruction enfants OOF incluse).

### D6 — API `searchHighlighting` du stub retirée
**Sévérité : nulle.** Inventée par le stub initial ; n'existe
pas dans `findtext.{cc,hh}` amont — retirée du code et de
`Layout`.

### D7 — `hyphenator` chemins : `DILLO_LIBDIR` runtime + fallback
**Sévérité : faible.** Amont = macro de compilation. Port :
`env::var("DILLO_LIBDIR")` fallback `libdir` — déjà documenté
(chemin resource, phase platform/binary à finaliser).

### D8 — `WidgetKind::Text` seam de test
**Sévérité : nulle.** Usage `TextIterator` commenté amont ;
seam pour traversées cross-widget de `DeepIterator`/`CharIterator`.

## Checklist de la skill (état)

- [x] Line breaking verbatim (penalties/badness + constantes).
- [x] `Word`/`Line` ascent/descent, espaces collapsés
      (`collapsing-spaces` doc : `spaceStyle`, `origSpace`,
      trailing space width verbatim dans `fillSpace`/`processWord`).
- [x] Re-wrap incrémental : `addText` par chunks → `wordWrap`
      re-queue limité aux lignes affectées (`handOverBreak`,
      `mustQueueResize`, `resizeQueued` flags amont).
- [x] Hyphenator `.pat`/`.exc`/`.trie` verbatim ; fichiers
      `hyph-*.pat` **à copier** en test-data (TODO tests).
- [x] `vertical-offset`, `text-align`, `text-indent`,
      `line-height` (absolu/pourcent) — chemins `words.rs`/
      `breaking.rs`/`mod.rs` verbatim.
- [x] `alignedtextblock` : List partagée, `updateValue`,
      `leftInnerPadding`/`line1Offset` (pas de tab-stops amont —
      c'est le mécanisme de puce numérotée, verbatim).
- [x] Itérateur : ordre Content, positions, highlight →
      sélection (`TbInFlow` + `CharIterator` verbatim).
- [x] `findtext` : incrémental CharIterator, case règles amont,
      wrap-around RESTART.
- [x] `selection` : granularité caractère, ancrage
      `(widget,index)` DeepIterator — positions **relatives**
      verbatim (survit au re-wrap comme amont, dérive identique).
- [x] `ruler`/`bullet`/`listitem` : verbatim (ordered-list =
      `initWithText("N.")` + numérotation côté appelant html).
- [x] Unicode lenient via `lout::unicode` (déjà porté dw-0/1).

## Test gaps (à couvrir phases suivantes)

- **Golden renders** pixel (L5) : harnais headless non écrit —
  report A10 (backend tiny-skia). Aujourd'hui : tests unitaires
  de machine d'état uniquement.
- **Re-wrap streaming** multi-chunks : tester frontières mot
  coupé au milieu (`addText` partiel) — non couvert.
- **Sélection après re-wrap/resize** : dérive positions — cas
  L3 non testés (besoin d'un resize réel après ancrage).
- **Boundary** : mot > largeur bloc, textblock vide + parbreak
  premier, `END_OF_WORD` hors texte, overflow penalty
  (`PENALTY_*` saturations) — partiellement couverts.
- **Hyphenation réelle** : nécessite les `.pat` data — loader
  testé sur fixtures inline seulement.
- **OOF × textblock** : `getBorder` lignes vs floats (seam A08)
  — à vérifier dès tables/floats exercés sur du vrai layout.

## Gates

`fmt` ✓ · `clippy -D warnings` ✓ (allows ciblés : casts `int`
verbatim, `unused_self`/`too_many_arguments`/`fn_params_
excessive_bools`/`struct_excessive_bools` signatures+champs
verbatim, `comparison_chain`, `match_same_arms` bras verbatim)
· 87 tests `dillo-dw` + workspace ✓ · `cargo doc` sans
warning ✓ · aucun `unwrap`/`panic`/`todo` hors tests.

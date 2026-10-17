# Audit A07 — Tables dw (rapport partiel)

**Mode** : `rapport` (ledger post-port, même stratégie qu'A08).
**Périmètre audité** : `crates/dillo-dw/src/table.rs` (~2 700
lignes) + dispatchs `WidgetKind::Table`/`cell` dans `widget.rs`,
`oof.rs`, `textblock/{breaking,sizing,listitem}.rs` vs
`dw/table.{cc,hh}`, `dw/tablecell.{cc,hh}`,
`dw/simpletablecell.{cc,hh}`, `dw/alignedtablecell.{cc,hh}`,
`dw/alignedtextblock.{cc,hh}`, `dw/table_iterator.cc`,
`dw/stackingcontextmgr.{cc,hh}`, `dw/regardingborder.{cc,hh}`
+ `devdoc/rounding-errors.doc`.
**Hors périmètre** : `src/table.cc` (glue HTML→dw — attend le
parseur `html.cc`, lot séparé audit-03), golden renders pixel
(A10), `IMPL_POS=true` (build amont non standard).

## Cartographie

```
WidgetKind::Table { tbl: Box<TableData> }
      │  oof: OofAwareData embarqué (Table ≺ OOFAwareWidget,
      │    PAS RegardingBorder — is_regarding_border_kind faux)
      │  children: Vec<Option<Child>>  (numRows × numCols)
      │    Cell{widget, colspanOrig/Eff, rowspan} | SpanSpace{start}
      │  colExtremes · colWidth{Specified,Percentage} (+compteurs)
      │  colWidths · cumHeight · rowSpanCells · baseline · rowStyle
      ▼
tbl_add_cell / tbl_add_row / tbl_realloc_children
tbl_force_calc_cell_sizes ─► tbl_actually_calc_cell_sizes
      │  totalWidth = max(availWidth, corr(adjustTableMinWidth))
      │   − spacings − boxDiff
      ▼
tbl_force_calc_column_extremes  (colspan==1 puis multi-colonnes)
tbl_calc_extremes_span_multi_cols / tbl_calc_adjustment_width…
tbl_apportion2  (3 cas : diffExtr / selon minima / parts égales —
                 cumulatif entier verbatim)
tbl_apportion_row_span  (redistribution hauteurs rowspan>1)
TableInFlow ≙ TableIterator (OofAwareIterator porteur)
TextblockData.cell: Option<TableCellKind> ≙ SimpleTableCell /
                  AlignedTableCell (sous-classes Textblock amont)
```

## Checklist spécifique

- [x] `colspan`/`rowspan` : `Cell{colspanOrig/colspanEff/rowspan}`
      + `SpanSpace{startCol,startRow}` verbatim (table.hh:330-346) ;
      `MAXSPAN=100` bornant `colspan<0|>100` et `rowspan<=0|>100`
      (table.cc:468-478) ; rangée implicite si `numRows==0` ou
      `rowClosed` (table.cc:484-494) ; `colspan 0` →
      `colspanEff = max(numCols-curCol, 1)` + `rowClosed`
      (table.cc:496-505) ; `reallocChildren` agrandit la grille en
      conservant l'existant (table.cc:724+).
- [x] Dimensionnement : `forceCalcColumnExtremes` d'abord
      `colspan==1` (Min/Max), puis `calcExtremesSpanMultiCols`
      (répartition de l'excès cellule > Σcolonnes) puis
      `calcAdjustmentWidthSpanMultiCols` — ordre verbatim ;
      `\bug` amont conservé : `numColWidthSpecified = 0` écrit
      **deux fois** et `numColWidthPercentage` jamais remis à
      zéro (table.cc:1417+) — dérive cumulative observable
      reproduite et commentée.
- [x] `apportion2` : 3 cas verbatim — cas normal
      (`cumDiffExtr·totalDiffWidth/totalDiffExtr − cumDiffWidth`),
      `totalMin≠0` (parts selon minima), `totalMin=0` (parts
      égales) — arithmétique **entière cumulative** fidèle à
      `rounding-errors.doc` (test : min=[0]³ → [33,33,34]).
- [x] Cas de largeur de table (table.cc:1090-1230) : cas 1
      `minWidth>total` → MinMin/MaxMin, ou branche pourcentages
      (`widthPartPer`/`minWidthIntrinsicPer` verbatim) ; cas 2
      `totalWidthSpecified && total>Σmax` → tous-max ou colonnes
      spécifiées exclues + `apportion2` sur `ExtrMod::Data` ;
      cas 3 → Min/Max sur `min(total, Σmax)`.
- [x] `border-spacing` : `(numCols+1)·hBorderSpacing` +
      `boxDiffWidth` dans requête/extrêmes/largeur disponible ;
      `vBorderSpacing` par rangée dans `cumHeight` (verbatim).
      `cellpadding`/`cellspacing`/`border` HTML → `src/table.cc`
      (hors périmètre — glue).
- [x] Table width : `width=` forçé détecté via `correctRequisition`
      sur requête factice (astuce verbatim table.cc:1100) ;
      `adjustTableMinWidth` statique globale portée (`true`
      défaut amont, réglable prefs).
- [x] Cellules/rangées vides : `child(NULL)` ≙ case vide, rangée
      sans `CELL` tolérée (test `grid_raster_order` — rangée
      courte). Contenu hors `<td>` → glue (hors périmètre).
- [x] Tables imbriquées : `notifySetParent` des tables OOF-aware
      trouve le containing block FLOATS chez le textblock ancêtre
      verbatim (`isPossibleOOFContainer` : `Table` →
      `oofmIndex != OOFM_FLOATS` — une table n'est jamais
      conteneur de floats amont, toplevel inclus ; testé via
      textblock top-level comme en usage réel).
- [x] `valign`/`height` : `correctAvailHeightOfChild` identité
      verbatim (`tablecell.cc` retourne `availHeight` tel quel) ;
      `cumHeight` = max requêtes rangée + `vBorderSpacing` ;
      `apportionRowSpan` redistribue aux rangées recouvertes
      (test `rowspan_height_redistributed`).
- [x] `table_iterator` : `numParts = children->size()`, section =
      case ; `CELL` → `WIDGET_IN_FLOW` index ; `SPAN_SPACE`/NULL →
      `InvalidIterator` sauté par next/prev → ordre raster sur
      racines seules (verbatim table_iterator.cc:60+) ;
      `highlight`/`unhighlight`/`getAllocation` in-flow = no-op
      `\bug` amont (commentaire `???` conservé).
- [x] Re-layout incrémental : `addCell`/`addRow` →
      `queueResize(0, true)` ; `forceCalcCellSizes` court-circuit
      `colWidthsUpToDateWidthColExtremes` ; `redrawX`/`redrawY`
      bornés dans `setCumHeight`/`calcCellSizes` ;
      `containerSizeChanged` notifié par colonne modifiée
      (TODO amont verbatim : spans multi-rangées via 1re col).
- [x] `src/table.cc` attributs : hors périmètre (glue HTML).

## Lentilles

- **L1 bornes** : spans > 100 / négatifs → clamp verbatim
  (`malformed_spans_clamped_to_maxspan`, grille 100×101
  effective) ; `numCols=0` → extrêmes = `boxDiff` seul
  (`empty_table_extremes_zero`) ; `colspan 0` en fin de rangée
  → `max(0,1)=1` verbatim.
- **L2 tolérance** : la glue produira des arbres tordus — dw
  tolère `child=NULL`, rangées courtes, `addCell` avant tout
  `addRow` (rangée implicite `MSG` amont), `rowClosed` → rangée
  supplémentaire. Aucun panic possible : accès `children` bornés
  par `n < len` (verbatim gardes amont) + `Option`.
- **L3 cycle de vie** : `tbl_drop` ≙ `~Table` (cellules `CELL`
  détruites via `widget_delete` — verbatim ordre : corps avant
  `~OOFAwareWidget`) ; `widget_children_for_detach` inclut les
  cellules ≙ masque `WIDGET_IN_FLOW|WIDGET_OOF_CONT` ;
  `~Table` ne détruit PAS les `SpanSpace` (verbatim — ce ne
  sont pas des widgets).
- **L4 incrementalité** : extrêmes colonnes recalculés à la
  demande (`forceCalcColumnExtremes` depuis `getExtremesSimpl`
  et `actuallyCalcCellSizes`) — même points de déclenchement
  qu'amont ; largeurs qui bougent au fil du parsing = nouveau
  `queueResize` → nouveau calcul.
- **L5 parité pixel** : `apportion2` cumulatif entier testé
  ([16,34,50] selon minima, [33,33,34] parts égales — reliquat
  sur dernière colonne verbatim) ; extrêmes multi-colonnes
  [50,50] pour cellule 100px sur 2 cols (test). Golden render
  pixel à venir (A10).

## Déviations

### D1 — `CellData` fusionné dans `Child::Cell`
**Sévérité : nulle.** Amont alloue `Child` + `CellData` séparés
(`child->cell`) ; le port fusionne (`Child::Cell(CellData)`) —
champs et accès identiques, une indirection de moins.

### D2 — `TableCellKind` ≙ sous-classes cellule
**Sévérité : nulle.** `SimpleTableCell`/`AlignedTableCell` sont
des `Textblock` amont → champ `cell: Option<TableCellKind>` sur
`TextblockData` + dispatchs (`isBlockLevel`→false,
`usesMaxGeneratorWidth`→true, clamps avail-width/requisition,
`getValue`/`setMaxValue`/`updateValue` virtuels). Même cycle de
vie (cellule détruite comme tout `Textblock` + `~Table`).

### D3 — `stackingcontextmgr` : seams, corps non porté
**Sévérité : nulle (vérif amont).** `IMPL_POS=false` →
`isEstablishingStackingContext`≡false → `stackingContextMgr`
jamais instancié (gardes `IMPL_POS` dans `Layout::addWidget` et
`Widget::setStyle`) → `SL_SC_BOTTOM`/`SL_SC_TOP` no-op et
`handledByStackingContextMgr`≡false amont **aussi**. Le port
reproduit les prédicats verbatim + champs/héritage
`setParent` ; les corps `draw`/`getWidgetAtPoint`/`findZIndex`
seront portés si `IMPL_POS` est activé un jour.

### D4 — `regardingborder` → `is_regarding_border_kind`
**Sévérité : nulle.** Tag interface vide amont (ctor/dtor
debug-only, `regardingborder.cc` 39 lignes) — `Table` n'hérite
**pas** `RegardingBorder` amont (OOFAwareWidget direct), le
prédicat reste limité à `Textblock`/`Ruler` (verbatim).

### D5 — `SL_OOF_REF` union morte dans `tbl_draw_level`
**Sévérité : nulle.** Amont lit `word->content.widget` pour
`SL_OOF_REF` malgré type `WIDGET_OOF_REF` — union jamais
déréférencée (même analyse qu'A06/D4).

## Non-bugs / points vérifiés

- `Table::removeChild` = `\bug Not implemented` amont → no-op
  verbatim (ne délègue même pas à `OOFAwareWidget::removeChild`).
- `affectsSizeChangeContainerChild` → `true` inconditionnel
  (table.cc:343-355 — tracking trop coûteux, verbatim).
- `Table::usesAvailWidth`/`isBlockLevel` → `true` ; cellules →
  `isBlockLevel` false (inline vis-à-vis du parent) — dispatch
  `w_is_block_level` ordonné cellule-avant-textblock.
- `applyPerWidth/Height` : `Table` et `tablecell` **sans**
  `boxDiff` (table.cc:320-325, tablecell.cc:122-128) — dispatch
  avant le `+boxDiff` générique.
- `correctAvailWidthOfChild` : clamp `availWidth−boxDiff` appliqué
  après `Textblock::getAvailWidthOfChild` (ordre verbatim).
- `AlignedTableCell::wordWrap` : détection char dans le mot TEXT
  → `charWordIndex/charWordPos = pos+1`, sinon `' '` trouvé →
  mot suivant + `charWordPos=0` ; `updateValue` seulement quand
  `wordIndex == charWordIndex` (verbatim
  alignedtablecell.cc:152-179) — dispatché dans `tb_word_wrap`.
- `getValue` : mesure `textWidth(font, text, charWordPos)`
  **brute** — pas de text-transform (verbatim).
- `numColWidthPercentage` non-reset (bug amont conservé —
  dérive cumulative sur re-layout, documentée dans le code).

## Test gaps

- [ ] `test/dw/dw_table*.cc` amont à porter en golden-layout
      (nécessite harnais A10) — les unit-tests actuels couvrent
      grille/spans/extrêmes/apportion/hauteurs/itérateur mais pas
      le rendu pixel.
- [ ] `valign` réel (alignement vertical dans la rangée) —
      non testé (dépend allocation complète).
- [ ] `rowClosed` via `addRow` intermédiaire + `colspan 0`
      combiné — partiellement couvert.
- [ ] Glue `src/table.cc` — audit différé (audit-03 parser).
- [ ] `numSizeRequestReferences`/`calcExtraSpaceImpl` des tables
      — seam présente, usage amont réel = `references` mot OOF ;
      à revérifier au lot positioned-layout.

## Verdict

**Conforme** sur le périmètre `dw` : algorithme de
dimensionnement maison porté verbatim (passe colonnes, cas de
largeur, `apportion2` cumulatif, hauteurs/rowspan), comptabilité
spans et tolérance aux arbres tordus fidèles, itérateur raster
verbatim, `\bug` amont conservés. 18 tests table verts ; 118
tests dillo-dw ; workspace/clippy/doc/fmt verts. La glue
`src/table.cc` reste un lot futur dépendant du parseur HTML.

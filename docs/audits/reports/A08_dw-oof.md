# Audit A08 — Couche OOF

**Mode** : `rapport` (audit du lot juste porté — stratégie
« ledger d'abord », cf. décision utilisateur du thread A01).
**Périmètre audité** : `crates/dillo-dw/src/oof.rs` (~3 200
lignes) vs `dw/outofflowmgr.hh`, `dw/oofawarewidget.{hh,cc}`,
`dw/oofawarewidget_iterator.cc`, `dw/ooffloatsmgr.{hh,cc}`,
`dw/regardingborder.{hh,cc}` + dispatchs `WidgetKind::OofAware`
dans `widget.rs`/`layout.rs`.
**Hors périmètre (non porté)** : `oofpositionedmgr` et dérivés
(morts amont, `IMPL_POS=false` — voir D1), `stackingcontextmgr`,
intégration `textblock` (consommateur principal : `initOutOfFlowMgrs`,
`addWidgetOOF`, `addWidgetInFlow`, bordures de ligne flottantes).

## Cartographie

```
WidgetKind::OofAware { data: OofAwareData }
      │  oof_container[4] · out_of_flow_mgr[4] (Rc<RefCell<dyn OofMgr>>)
      ▼
notify_set_parent → search_containers (isPossibleOOFContainer*)
init_out_of_flow_mgrs → FLOATS : OofFloatsMgr ; ABS/REL/FIXED : None
      │
OofFloatsMgr ─ floats: Vec<Float> + sorted_left/right (bsearch GB)
      │  tell_position → update_generators → widget_ref_size_changed
      │  get_border → lignes textblock · get_clear_position
      ▼
OofAwareIterator : IN_FLOW → FLOATS → ABS → REL → FIXED
      parent_ref = in-flow subRef | (oofmSubRef<<8 | oofmIndex+1)
```

## Verdict global

Couche **conforme** sur le périmètre porté : la hiérarchie de
managers, les callbacks et l'ordre d'itération sont verbatim ;
les floats gardent listes gauche/droite triées, collisions
verticales-puis-horizontales, clamping des débordements et
`clear` amont. 19 tests ciblés verts + 50 tests itérateur.

## Déviations

### D1 — Mgrs positionnés instanciés amont, `None` dans le port
**Sévérité : nulle (vérif amont) — documentée.**

Amont `initOutOfFlowMgrs` (`oofawarewidget.cc:205`) crée sans
garde `OOFPosAbsMgr`/`OOFPosRelMgr`/`OOFPosFixedMgr`. Ils sont
**inatteignables** : `testStyle*Positioned` ≡ `IMPL_POS && …`
≡ `false` (`core.hh:16`) → `getOOFMIndex` ne peut renvoyer que
`OOFM_FLOATS`, et `doesWidgetOOFInterruptDrawing` est
`if (IMPL_POS) … else false`. Tous les consommateurs amont
itèrent `outOfFlowMgr[i]` **avec test NULL**
(`oofawarewidget.cc:247,313,346,360,451`…). Le seul état des
mgrs vides (`containerAllocation` dans `sizeAllocateStart`)
n'est lu que par des enfants positionnés — inexistants.
Représenter par `None` est donc *observablement identique*,
et moins lourd. **Si `IMPL_POS` passe un jour à `true`** : porter
les 4 fichiers et basculer les slots à `Some` — les seams sont
déjà en place (constantes, `get_oofm_index`, sections
itérateur, `parent_ref` encodant `oofmIndex`).

### D2 — `stackingContextMgr` non porté → niveaux SC no-op
**Sévérité : nulle actuellement — seam.**

Amont : `drawLevel`/`getWidgetAtPointLevel` testent
`stackingContextMgr != NULL` pour `SC_BOTTOM`/`SC_TOP`. En
l'absence de `z-index` traité (le stacking mgr n'est instancié
que pour les contextes positionnés — morts via IMPL_POS), le
branchement est `None` → no-op, identique amont quand le mgr
est NULL. Seams `SC_BOTTOM`/`SC_TOP` déjà dispatchées dans
`oof_draw_level`/`oof_get_widget_at_point_level`.

### D3 — `assertNotReached`/`assert` amont → `debug_assert!`
**Sévérité : nulle (convention workspace).**

Amont `assertNotReached()` abort en release-checks ; Rust
`debug_assert!` n'agit qu'en debug — convention déjà adoptée
partout dans le port (documentée AGENTS/ADR). Les chemins
« impossibles » retournent des valeurs sûres (None/skip) en
release comme amont en `NDEBUG`.

### D4 — Itérateur OOF : `num_parts` optionnel + accès arène
**Sévérité : nulle — modèle D.**

`OofAwareIterator` prend `&Layout` (accès arène aux mgrs derrière
`Rc<RefCell>`) où amont déréférence des pointeurs. Signature du
ctor diffère, sémantique identique (section courante, index,
parentRef encode/décodage bit à bit verbatim, `compare`
cross-section verbatim).

### D5 — `markSizeChange`/`updateReference`/`widgetRefSizeChanged`
no-op sur kind nu
**Sévérité : nulle — verbatim.**

Amont : `notImplemented()` → no-op dans la base OOF-aware, les
vraies impls vivent dans `Textblock` (surcharges). Le kind
`OofAware` nu reproduit les défauts ; textblock consommera les
seams (`w_update_reference`, `w_widget_ref_size_changed`,
`w_mark_size_change`, `w_oof_size_changed`) — listées dans
PORTING.md.

## Quirks amont préservés (vérifiés)

- `notifySetAsTopLevel` met **tous** les slots `oofContainer[i]`
  au widget lui-même — pas seulement FLOATS (verbatim, testé).
- `OOFFloatsMgr` enregistre le CB lui-même comme premier générateur
  in-flow (`t1`), `updateGenerators` asserte `widgetReference`
  non-nulle pour les floats enregistrés.
- `tellPosition` : liste latérale propre triée par GB ; float trop
  large → clamp `x = 0` (gauche) / `x = max(0, effWidth - w)` droite.
- `collides` : recouvrement **vertical** via `accumulateHeight`,
  puis horizontal séparé (`getCombinedPosition` côté opposé).
- `getGB`/`getClearPosition` : `clear` consulte les deux listes et
  retourne le y de sortie du float le plus bas du côté concerné.
- `moveExternalIndices` décale les `externalIndex` des références
  ≥ seuil (verbatim `>=`).
- `parent_ref` : bit-31 OOF, bits 8+ `oofmSubRef`, bits 0-7
  `oofmIndex + 1` ; in-flow = subRef brut — encode/décode testés.
- Ordre itérateur verbatim : contenu in-flow **d'abord**, sections
  mgrs ensuite — distinct de l'ordre de peinture (seams SC).
- `~OOFAwareWidget` détruit les enfants OOF : ≙ `widget_delete`
  qui marque/detache via `OOF_CONT` + tombstone d'arène (les ids
  ne sont jamais réutilisés — équivalent `delete` amont).
- `mustAddBreaks` : float + relative-position ⇒ `false` amont
  (test corrigé en conséquence — le port était juste).

## Risques / à revérifier au lot textblock

- **Bordures de ligne** : `getBorder`/`getGB`/`getLeftOOFBorder`
  ne sont utiles que via textblock (`getAvailWidthOfChild` par
  ligne). Tests actuels = unitaires mgr ; la conformité
  pixel-flot texte↔float sera vérifiable quand textblock posera
  les lignes.
- `update_generators` dépend de `tell_position` des widgets
  enfants in-flow — appelés par les surchargeurs amont ; la
  boucle notify↔queueResize sera à observer (éviter cycles
  resize infinis ≙ amont `wasAllocated` guards — déjà portés).
- `RegardingBorder` : `is_regarding_border_kind` retourne
  `false` pour tous les kinds actuels — les floats amont
  « border regarding » (image dans paragraphe bordé) seront à
  couvrir quand `Textblock`/`Ruler` marqueront `true`.

## Gates

`fmt` ✓ · `clippy -D warnings` ✓ (allows ciblés : casts `int`
verbatim + `unused_self` signatures amont + `match_same_arms`
bras verbatim) · 19 tests oof + 50 itérateur + workspace ✓ ·
`cargo doc` sans warning ✓.

## Addendum — complétion : intégration textblock↔OOF + regard borders + draw/hit-test

**Périmètre ajouté** : le consommateur principal différé au moment du
rapport partiel — `dw/textblock.cc` (addWidget, calcBorders,
updateReference/widgetRefSizeChanged/oofSizeChanged,
getAvailWidth/HeightOfChild, containerSizeChangedForChildren,
affectsSizeChangeContainerChild, sizeAllocateImpl),
`dw/textblock_linebreaking.cc` (calcBorders, balanceBreakPosAndHeight,
wrapWordOofRef, drawLine→tellPosition2, minHeight lignes vides),
`dw/oofawarewidget.cc` (isOOFContainer, notifySetParent,
sizeAllocateStart/End, correctExtremesByOOF, draw/drawLevel/drawOOF,
getWidgetAtPoint*), `dw/ooffloatsmgr.cc` (tellPosition1/2,
tellIncomplete*, sizeAllocateFloats, calcFloatX, drawFloats,
getFloatWidgetAtPoint, affects*Border, dealingWithSizeOfChild),
`dw/tools.cc` (`SizeParams`), `dw/regardingborder.cc` (désormais
actif : `Textblock`/`Ruler` héritent `RegardingBorder`).

### Verdict : conforme — aucune déviation nouvelle

Relecture croisée exhaustive ; chaque point vérifié contre la ligne
amont citée :

- **`addWidget`** (`textblock.cc:2488-2540` ≙ `words.rs:825-890`) :
  ordre verbatim — `parentRef=-1` → `setStyle` → `initOutOfFlowMgrs` →
  `setParent(oofContainer[i])` → `setGenerator(this)` →
  `addWidgetOOF` → `parentRef=makeParentRefOOF` → `calcWidgetRefSize`
  → `addWord` + `WIDGET_OOF_REF`/`WidgetReference` +
  `setBreakOption` (césure permise après une réf OOF). Branche
  in-flow : enfant Textblock → `addWidgetInFlow` sur tous les mgrs ;
  `TOPLEFT_OF_LINE` via `calcSizeOfWidgetInFlow`.
- **`calcBorders`** (`linebreaking.cc:2075-2180` ≙
  `breaking.rs:1740-1810`) : `effOofRef = max(lastOofRef,
  firstWordOfLine - 1)` (cas du float né dans un `<p>` interne —
  commentaire verbatim), boucle mgrs avec `findSizeRequestReference`,
  `hasFloat*` puis `getLeftBorder − getGeneratorX` /
  `getRightBorder − getGeneratorRest`, hauteurs float par `max`
  (quirk « max pas tout à fait correct » conservé).
- **`tellPosition1`** (`ooffloatsmgr.cc:751-842` ≙ `oof.rs:2083-
  2133`) : collision même côté = float précédent seul ; côté opposé =
  `findLastBeforeSideSpanningIndex` puis boucle tant que collision
  verticale, arrêt à la 1re collision horizontale (exemple #f1/#f2/#f3
  amont reproduit) ; `updateGenerators` si `yReal` a changé.
  `tellPosition2` vide verbatim ; `tellIncomplete*` = `notImplemented`
  → no-op documenté.
- **Contrat `sizeAllocate`** (`widget.cc:1209` + `oofawarewidget.
  hh:25-35`) : `sizeAllocateStart`/`End` **dans** `sizeAllocateImpl` —
  wrapping présent dans `tb_size_allocate_impl` (`sizing.rs:252,357`)
  et `tbl_size_allocate_impl` (`table.rs:408,478`) ; les mgrs notifiés
  sont ceux des `oofContainer[i]` (pas les siens) ; `container
  Allocation` posé si `caller == container` ; `sizeAllocateFloats`
  LEFT puis RIGHT.
- **`calcFloatX`** (`ooffloatsmgr.cc:431-494` ≙ `oof.rs:1530-1575`) :
  left = `generatorX + boxOffsetX`, clamp `max(0, maxGenWidth − w)` ;
  right = branche `usesMaxGeneratorWidth` (verbatim cas inline-block),
  `x = genX + effWidth − w − boxRestWidth`, `max(0, …)`.
- **Ordre de dessin** : boucle `SL_START+1 .. SL_END` verbatim ;
  floats dessinés à `SL_OOF_CONT` **après** les lignes in-flow →
  floats au-dessus du texte ; `drawFloats` liste gauche puis droite
  (ordre croissant), skip `hasWidgetBeenProcessedAsInterruption` /
  `handledByStackingContextMgr` / `!intersects` (`oof.rs:1578-1615`).
- **Hit-test** (`oofawarewidget.cc:456` + `ooffloatsmgr.cc:721-749`)
  : niveaux en ordre inverse ; floats testés **droite puis gauche**,
  itération décroissante dans chaque liste — verbatim
  (`oof.rs:get_widget_at_point`).
- **`correctExtremesByOOF`** (`oofawarewidget.cc:300-338` ≙
  `oof.rs:762-778`) : max sur les 5 champs, **y compris le copier-
  coller amont** `maxWidthIntrinsic.max(oofMinWidth)` (pas oofMax).
- **`isOOFContainer`** (`oofawarewidget.cc:127-178` ≙ `oof.rs:614-
  650`) : FLOATS = `isPossibleOOFContainer` ∧ (toplevel ∨ parent non
  container-parent ∨ inline-block ∨ overflow≠visible ∨ OOF) ;
  REL/ABS = toplevel ∨ `testWidgetPositioned` ; FIXED = toplevel —
  verbatim. `notifySetParent` : remontée des parents, assert non-nul.
- **`RegardingBorder` actif** — la note du partiel
  (« `is_regarding_border_kind` retourne false pour tous les kinds »)
  est **obsolète** : `Textblock` et `Ruler` sont marqués
  (`oof.rs:265`), `mustBorderBeRegarded` + `widgetRegardingBorder
  ForLine` verbatim dont l'exclusion `!isOOFContainer(FLOATS)`
  (`breaking.rs:85-128`). Floats « border regarding » (image dans
  `<div>` bordé) couverts.
- **`updateReference`** : `queueResize` seulement si words non vides
  ou `clear ≠ None` (verbatim textblock.cc:3080-3090 ≙
  `sizing.rs:665-675`) ; **`widgetRefSizeChanged`** : `findLineOfWord`
  → `queueResize(makeParentRefInFlow(line), true)` ;
  **`oofSizeChanged`** : `queueResize(-1, extremesChanged)` +
  `containerSizeChanged` si extrêmes ∧ `!usesMaxGeneratorWidth` ;
  **`markSizeChange`/`markExtremesChange`** : délégation OOF vs
  `wrapRefLines`/`wrapRefParagraphs` (min tracking, ref=-1 ignoré —
  workaround amont verbatim).
- **`getAvailWidth/HeightOfChild`** : délégation OOF si
  `dealingWithSizeOfChild` — `false` verbatim pour OofFloatsMgr ;
  clamp auto-width `min/max-width` − `leftInnerPadding`, borne
  `maxWidth − boxDiff − leftInnerPadding` si `!usesMaxGeneratorWidth`.
- **`affectsSizeChangeContainerChild`** : `true` si `!usesMax` ∧
  extrêmes queued/changed — appel **base statique** sinon (note code :
  re-dispatch = récursion infinie, verbatim commentaire amont).
- **Ligne vide `minHeight`** (`linebreaking.cc:801-815`) : `max(min(
  leftFH, rightFH), 1)` si deux flottants, hauteur du seul sinon —
  verbatim.
- **`SizeParams`** (`tools.cc:88-200`) : `forChild` (référence parent
  → (xRel,yRel) sinon translation par params courants),
  `findReference`, `isEquivalent` — verbatim (`tools.rs:23-110`).
- **`dealingWithSizeOfChild`/`getAvailWidth/HeightOfChild`** du
  floats mgr : `false` / `notImplemented` → no-op documenté — les
  floats se dimensionnent par `ensureFloatSize`→`sizeRequest`, pas
  par l'avail-width du parent — verbatim.

### Non-bug complémentaire

- **NB-6** — `wrapRefLinesFCX/FCY` bornés à `(0,0)` quand la
  référence floats est indéfinie ; amont assigne `newFCX`/`newFCY`
  **non initialisés** (quirk `textblock.cc:339-343`). Un rewrap
  amont supplémentaire peut survenir à la transition
  indéfini→défini(0,0) — mais le rewrap est idempotent →
  **non observable** (cycles de resize seulement). Déjà documenté
  dans `sizing.rs:64-68`.

### Lentilles

- **L1 bornes** : float plus large que le conteneur → clamp
  `max(0, …)` verbatim (`calcFloatX`) ; `effOofRef` gère les refs
  hors bornes ; `findLastBeforeSideSpanningIndex` borné.
- **L2 récupération** : OOF sans container positionné impossible
  (assert `oofContainer != NULL` ≙ `debug_assert`) ; `tellIncomplete`
  = no-op comme amont.
- **L3 cycle de vie** : destruction ancêtre → enfants OOF détruits
  via `OOF_CONT` + tombstones (partiel, vérifié) ; re-génération au
  resize via `updateGenerators`→`updateReference(0)` verbatim.
- **L4 event loop** : `widgetRefSizeChanged`/`oofSizeChanged` →
  `queueResize` (idle) ≙ amont ; pas de re-layout synchrone caché.
- **L5 parité pixel** : ordre de peinture (in-flow → floats) et
  hit-test (floats dessus, droite d'abord) verbatim ; positions à
  l'entier près couvertes par les goldens floats.

### Test gap analysis (complément)

- **Goldens existants** : `multiple-floats`, `float-img-justify`
  + autres paires float/clear du corpus `render_pairs_pixel_identical`
  — couvre placement, wrap autour, `clear`.
- **Gap restant** : scène #f1/#f2/#f3 (collision côté opposé) et
  `clear:both` après float dans `<div>` imbriqué (cas `updateGenerators`
  explicite) ne sont pas des pages golden dédiées — les ajouter
  renforcerait la couverture des branches `collidesH`/`updateGenerators`.
- `dw_float_test.cc` amont = démo FLTK interactive — pas de port
  direct ; couvert par les goldens.

**Verdict complétion** : périmètre skill A08 désormais couvert en
entier. D1/D2/D5 du partiel restent des seams documentées (morts
amont `IMPL_POS=false` ; à porter si `IMPL_POS` devient actif).
Aucune déviation appliquable trouvée — le consommateur textblock est
verbatim, y compris les quirks (copier-coller `oofMinWidth`,
`max` incorrect sur hauteurs, GC des positions incomplètes).

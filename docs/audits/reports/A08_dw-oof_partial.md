# Audit A08 — Couche OOF (rapport partiel)

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

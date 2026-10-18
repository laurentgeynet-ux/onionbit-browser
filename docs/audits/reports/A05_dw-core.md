# Audit A05 — dw core : arbre de widgets & moteur de layout

**Mode** : `rapport` (analyse complète widget.cc/layout.cc relus
en entier + devdocs de spec).
**Périmètre audité** :
`crates/dillo-dw/src/{types,style,widget,layout,iterator,events,
platform,view,ui,tools}.rs` (~13k lignes) vs
`dw/{types,style,widget,layout,iterator,events,platform,view,ui,
tools}.{cc,hh}` (~10.5k lignes) + `devdoc/{rounding-errors,
dw-widget-sizes,dw-layout-views,dw-interrupted-drawing,
dw-overview,dw-grows,dw-size-request-pos}.doc`.
**Hors périmètre** : corps des kinds concrets — `Textblock`
(A06), `Table`/cellules (A07), managers OOF/SCM (A08, seams
`IMPL_POS=false` verbatims), `Image` widget (A09), impl
`Platform`/`View` concrete (A10), harnais golden headless.

## Cartographie

```
upstream C++                          Rust (ADR-0005, modèle D)
─────────────────────────────────────────────────────────────
Widget* (parent/quasiParent/         WidgetId → nœud Arena
  generator/container bruts)           {parent, quasi_parent,
                                        generator, container}
héritage Widget→OOFAware→             enum WidgetKind dispatch
  Textblock/Table/…                    (Textblock/Table/OofAware/
                                       Embed/Image/Text/…)
Layout{topLevel, queueResizeList,    Layout{top_level,
  anchorsTable, find/selection,       queue_resize_list: Vec,
  scroll/canvas, emitter}             anchors_table: HashMap,
                                      find/selection_state,
                                      emitter, …}
view: View* · platform: Platform*    view: Option<Box<dyn View>>
                                      platform: Rc<RefCell<dyn
                                       Platform>>
callbacks synchrones                 Arena::pending_draws /
  (img renderers, resources)          pending_resizes drainés
                                      par Layout::drain_idles
~Widget chaîné (dtor dérivés         widget_delete / tb_drop /
  → ~Widget → removeChild/             tbl_drop / section OOF →
  removeWidget)                        callback → renderer →
                                       w_remove_child |
                                       remove_widget
```

## Verdict global

**Très fidèle dans l'ensemble** — la machine à flags
`RESIZE_QUEUED`/`ALLOCATE_QUEUED`/`EXTREMES_QUEUED` →
`NEEDS_*`/`EXTREMES_CHANGED`, la propagation
`markSizeChange`/`markExtremesChange` avec `parentRef`, le cache
`SizeParams` (`is_equivalent` insensible à l'ordre, exact comme
amont), `resizeIdle` (limites 100 itérations / 1000 compteur,
conversion flags → `sizeRequest` → `sizeAllocate` → canvas +
scrollbar handling + `containerSizeChanged` sur
`canvasHeightGreater`), le protocole
`correctRequisition`/`correctExtremes` viewport↔effContainer
(quirk `container == NULL` vs `parent` préservé),
`calcWidth`/`calcHeight`/`calcFinalWidth` (clamps min>max,
postconditions), `scrollTo`/`scrollIdle`/`adjustScrollPos`/
`calcScrollInto`, anchors différés (`y == -1`, `updateAnchor`),
`draw`/`queueDraw`/`queueDrawExcept`/draw différé après scroll,
interrupted drawing (`DrawingContext`,
`drawInterruption`, hit-test interrompu), `types.rs` verbatim
(incl. quirks : borne asymétrique `y >= maxy` du Polygon,
fusion Region ≤16), `tools.rs` verbatim, `events.rs` verbatim.

**Mais 3 déviations sémantiques confirmées** (D1–D3), dont une
pixel-level (L5) et deux qui cassent la chaîne de vie du
top-level (L3) : `addWidget`/`setWidget` incomplets. Plus une
série d'écarts défensifs/de modèle sans impact observable ou
latents (D4+). 118 tests `#[cfg(test)]` verts ; aucun ne couvre
les trois chemins déviés.

## Déviations

### D1 — `border-style: double` : bande intérieure inversée/dégénérée
**Sévérité : haute (L5 — parité pixel).**
`draw_border_left` : amont `style.cc:1050` —
`points[2].x = points[3].x = points[0].x + w` (≙ `x1 + 2w + d`,
le bord intérieur `x1 + borderWidth.left` puisque `2w + d = bw`)
; port `style.rs:2141-2142` écrit `x1 + w`. Idem
`draw_border_right` : amont `style.cc:1150` —
`points[2/3].x = points[0].x - w` (≙ `x1 + 1 - 2w - d`) ;
port `style.rs:2258-2259` écrit `x1 + 1 - w`.
Conséquence : le 2ᵉ trapèze couvre `[x1+w+d, x1+w]` — polygone
**inversé** (dégénéré quand `d = 0`) au lieu de
`[x1+w+d, x1+bw]`. Toute bordure `double` de largeur ≥ 2 rend un
trait intérieur absent ou déformé. `top`/`bottom` sont
verbatims (`style.cc:845-852`, `947-954` ≙ `style.rs:1886-1894`,
`2015-2026` — seuls left/right sont fautifs).
Fix : `points[2/3].x = points[0].x ± w`. Test : golden/replay de
`drawPolygon` pour `border-width: 4px; border-style: double`.

### D2 — `Layout::add_widget` : queue manquante
(`canvasHeightGreater` + `topLevel->queueResize`)
**Sévérité : haute (L3/L4 + effet pixel indirect).**
Amont `layout.cc:371-404` termine `addWidget` par :
`canvasHeightGreater = false` (l.396) **et**
`topLevel->queueResize (-1, false)` (l.403 — « Do not directly
call Layout::queueResize(), but Widget::queueResize(), so that
all flags are set properly »). Le port `layout.rs:647-659`
s'arrête après `findtext_state.set_widget` — **les deux lignes
manquent**.
Effets :
- `canvas_height_greater` n'est pas reset au changement de
  top-level → après un `set_widget` sur une nouvelle page,
  `viewport_width - v_scrollbar_thickness` reste soustrait dans
  `widget_get_avail_width`/`correct_requisition_viewport`/
  `widget_correct_extremes` → le nouveau top-level est disposé
  ~une épaisseur de scrollbar trop étroit → **dérive de pixels
  sur toute la page** jusqu'au prochain `viewportSizeChanged`.
- Pas de `queueResize` → pas d'`ALLOCATE_QUEUED`, pas de
  `queue_resize`/`emitResizeQueued` pour le nouveau top-level.
  Fonctionnellement masqué quand le contenu se queue lui-même
  (addWord/setStyle postérieurs) **et** par les flags ctor
  `NEEDS_RESIZE|EXTREMES_CHANGED` + le test `needs` de
  `resizeIdle` + la diff d'allocation initiale — mais le
  *signal* `resizeQueued` n'est jamais émis (le glue UI s'y
  abonne) et un top-level statique (ex. `dw_simple_container`)
  ne déclenche aucun layout tant que rien d'autre ne queue.
- Cas aggravé : `ComplexButtonResource::init` (`ui.rs:598`
  `layout.set_widget(widget)` ≙ `ui.cc:351`) — le `queueResize`
  amont du layout interne émettait `resizeQueued` → relayé
  vers `embed->queueResize`. Avec D2 ce premier relais ne part
  jamais.

### D3 — `Layout::set_widget` : `removeWidget` court-circuité
**Sévérité : haute (L3).**
Amont `layout.cc:438-442` : `topLevel = NULL; delete w` →
`~Widget` (widget.cc:135-136) voit `parent == NULL` et appelle
`layout->removeWidget()` — ce qui réalise `layout.cc:406-431` :
reset `queueResizeList`/`widgetAtPoint`/`canvas*`/`scroll*`,
`view->setCanvasSize(0,0,0)` + `setViewportSize(w,h,0,0)` +
`queueDrawTotal`, `setAnchor(NULL)` + `updateAnchor`,
`emitCanvasSizeChanged(0,0,0)`, `findtextState.setWidget(NULL)`,
`selectionState.reset()`. Le port `layout.rs:665-674` met
`top_level = None` **avant** `widget_delete_subtree` ; dans
`widget_delete` (`widget.rs:3232-3236`) le test
`self.top_level == Some(w)` est alors faux → `remove_widget()`
jamais appelé → **tous les side-effects sautés** : position de
scroll, sélection, `anchors_table`/`requested_anchor`, canvas
et émission `canvasSizeChanged(0,0,0)` restent ceux de la page
précédente (jusqu'à ce que D2-corrigé refasse un layout). Les
receivers (`LayoutReceiver::canvas_size_changed`) ne voient pas
le reset → le chrome (viewport, titre de scroll) peut garder un
état incohérent entre deux pages.
Fix probable : appeler `remove_widget()` dans `widget_delete`
sur `top_level == Some(w)` **ou** dans `set_widget` après
`widget_delete_subtree` (≙ `delete` amont qui laisse
`removeWidget` se déclencher via `~Widget`).

### D4 — `Arena::detached` ≉ `widget->layout == NULL`
**Sévérité : moyenne (latente) / faible (observable aujourd'hui).**
Amont : un widget jamais rattaché a `layout == NULL` →
`queueResize` prend la branche « no-layout » (flags
`NEEDS_*`/`EXTREMES_CHANGED` directs, pas de liste, pas d'idle,
pas d'émission). Port : `detached` (HashSet) n'est rempli que
par `detach_widget` (destruction) ; un nœud frais de
`add_widget_node` a `detached == false` → `widget_queue_resize`
le traite comme attaché (`widget.rs:809-817`) : entrée en
`queue_resize_list`, idle schedulé via `queue_resize`, **et
`emitResizeQueued` émis** pour un widget sans layout. Chaque
`setStyle`/construction avant rattachement émet donc des
signaux et du churn d'idle qu'amont n'a pas. Masqué après
`add_widget` (flags ctor + `queue_resize_list.clear()`), mais :
- observable via `LayoutReceiver::resize_queued` et le nombre
  d'itérations idle ;
- `widget_set_style` (`widget.rs:2757`) utilise `node_detached`
  pour « `layout == NULL` » — un widget attaché est
  indistinguable d'un jamais-attaché. Latent : deviendra un
  vrai bug quand `style_establishes_stacking_context` cessera
  de retourner `false` (seam SCM, phase positioning).
Fix : insérer les nœuds dans `detached` à `add_widget_node` et
le retirer à `widget_set_parent`/`add_widget`.

### D5 — ordre `ALLOCATE_QUEUED`/`markSizeChange` sur les ancêtres
**Sévérité : nulle.**
Amont `widget.cc:382-384` : `setFlags(resizeFlag)` →
`markSizeChange(child->parentRef)` → `setFlags(ALLOCATE_QUEUED)`.
Port `widget.rs:850-854` : `resize_flag` + `ALLOCATE_QUEUED`
d'un coup, puis `mark_size_change` → `ALLOCATE_QUEUED` visible
pendant `markSizeChange`. Aucune impl `markSizeChange` ne lit
ce flag — non observable aujourd'hui ; à réaligner si un impl
futur l'inspecte.

### D6 — `has_bg_color` : `ptr::eq` sur copies, toujours faux
**Sévérité : faible (perf) / pixel seulement en `inverse`.**
Amont `style.cc:1237-1240` : identité de pointeur `Color*`
(`layout->getBgColor() != bgColor`). Port `style.rs:2321-2323` :
`Color` copié par valeur dans `Layout::set_bg_color` →
`std::ptr::eq(c, b)` entre deux copies → toujours faux → la
bande de fond du top-level est **toujours** redessinée (là où
amont saute le draw quand c'est le même objet `Color`). Pixels
identiques en `SHADING_NORMAL` (même couleur au même endroit) ;
différence possible uniquement en `SHADING_INVERSE` quand
`at_top && colors égaux` (amont saute, port dessine un rect
inversé — chemin sélection-toplevel marginal).
Fix : `Rc<Color>` ou comparaison par valeur pour rapprocher la
sémantique d'identité. Idem noter que `has_bg_image` utilise
`Rc::ptr_eq` — correct car `StyleImage` est partagé.

### D7 — `pending_resizes`/`pending_draws` différés (modèle D)
**Sévérité : nulle.**
Amont : `ExternalImgRenderer::draw` → `widget->queueDrawArea`
immédiat ; `LayoutReceiver::resizeQueued` (bouton complexe) →
`resource->queueResize` → `embed->queueResize` immédiat. Port :
files taguées `layout_id` dans l'arène, drainées par
`drain_idles` (`layout.rs:1145-1164`, `ui.rs:512-531`,
`image.rs:305`, `widget.rs:3155-3175`). Justifié par le modèle
(callbacks sans `&mut Layout`) ; converge au même point — le
seul écart est la fenêtre temporelle entre push et drain.
`Resource::queue_resize` (`ui.rs:274-278`) est synchrone quand
il a `&mut Layout` — verbatim.

### D8 — asserts amont → dégradation gracieuse
**Sévérité : nulle (politique du port).**
Inventaire : `widget_intersects` retourne `Some` quand
`ref_widget` n'est pas ancêtre (amont `assert`/UB —
`widget.rs:748-751`) ; `view` absent toléré dans
`remove_widget`/`queue_draw`/`set_cursor` (amont deref nu) ;
`widget_size_allocate` remplace `assert(resize_idle_entered)`
par `debug_assert!` ; `change_anchor` no-op si absent (amont
assert) ; `widget_nearest_common_ancestor` → `None` ≙ `NULL` +
`MSG_WARN` ; `get_extremes`/`size_request` sans asserts
`!queueResizeEntered`. Aucun changement de comportement sur les
chemins contractuels ; à conserver.

### D9 — `widget_set_style` : `update_cursor` inconditionnel
**Sévérité : nulle.**
Amont `widget.cc:1345` : `if (layout != NULL) layout->updateCursor()`.
Port `widget.rs:2748` : appel direct. Sur widget non rattaché,
lit `widget_at_point` (état du layout — stale) et ré-émet le
même curseur — idempotent, sans effet visible.

### D10 — `widget_delete` : `style = None` après `removeChild`
**Sévérité : nulle.**
Amont `widget.cc:130-136` : `style->unref()` *avant*
`parent->removeChild(this)` (le style peut mourir, le champ
reste pendant `removeChild`). Port `widget.rs:3237-3241` : garde
le `Rc<Style>` vivant pendant `w_remove_child`, drop après —
strictement plus sûr, même observable.

## Checklist de la skill (état)

- [x] `Extremes`/`Allocation`/`Rect` : types entiers verbatim,
      quirks de géométrie préservés (`types.rs` ≙ `types.cc`
      bit-à-bit, tests inclus).
- [x] Protocole de layout `sizeRequest`/`getExtremes`/
      `sizeAllocate` : caches `SizeParams`, conversion des
      flags QUEUED→NEEDS aux mêmes points, re-calc uniquement
      quand amont le fait. `requisitionParams` assigné
      inconditionnellement des deux côtés ; `extremesParams`
      seulement en branche non-équivalente des deux côtés
      (asymétrie amont reproduite).
- [x] `markSizeChange`/`markExtremesChange`/`queueDraw` :
      propagation ancêtres + `parentRef` verbatim ; anti-boucle
      `(parent->flags & totalFlags) == totalFlags` verbatim.
- [x] Machine à flags `needsResize/needsAllocate/
      extremesChanged` reproduite bit pour bit (hors D4 :
      branche « no-layout » atteinte trop tôt).
- [x] Damage : `queueDraw` (zone vide ignorée), `queueDrawArea`
      (coords widget→canvas), `queueDrawExcept` (4 rectangles)
      verbatim ; `draw` différé pendant scroll
      (`drawAfterScrollReq`) verbatim.
- [~] Anchors : `addAnchor`/`changeAnchor`/`removeAnchor`/
      `updateAnchor` différé verbatim — **mais** `setAnchor(NULL)`
      du `removeWidget` sauté par D3 (ancre de la page
      précédente survivant à `set_widget`).
- [x] `iterator` : `EmptyIterator`/`TextIterator`/
      `ChildIterator`/`DeepIterator`/`CharIterator` — ordre de
      traversée, masks, `highlight`/`allocation` verbatim ;
      comparaisons cross-arbres plus sûres (D8).
- [x] Layout interruptible : limites 100/1000 verbatims, idles
      plateforme drainés via `take_pending_idles` (≙ callback
      toolkit), `queue_resize` réentrant pendant `resizeIdle`
      géré (`resize_idle_id = -1` avant le corps, verbatim).
- [x] Style : `sizeDiffs` → `queueResize(0,true)`/`queueDraw`
      verbatim ; renderers externes ref/unref ≙ `Rc` ; **sauf**
      D1 (`double`) et D6 (`ptr::eq`).
- [~] Destruction : `widget_delete` reproduit l'ordre des
      destructeurs chaînés amont (tb→table→OOF→callback→
      renderer→`removeChild`/`removeWidget`) — **mais** le
      cas top-level via `set_widget` est cassé (D3) et OOF
      dépend des seams A08.

## Test gap analysis

118 `#[cfg(test)]` dans le crate (layout 10, style 21, oof 19,
table 18, ui 13, iterator 8, types 8, selection/findtext 8,
bullet/ruler/listitem/tools/imgrenderer 16, types/tools…).
**`widget.rs` : 0 test propre** — le cœur de la négociation
n'est couvert que via les tests `layout.rs`/`oof.rs`.

L'inventaire amont `test/dw/` (19 drivers FLTK : anchors,
border, find, float, image_background, images_simple/scaled,
links ×2, lists, resource, simple_container ×2, table ×2, ui)
n'a pas d'équivalent headless — c'est le rôle du harnais
golden d'A10/phase 9.

Trous concrets à combler (apply) :

1. **D1** — replay `View::draw_polygon` : `border-style:double`,
     `border-width: 4px` et `6px` (d = 0 et 2) — le 2ᵉ trapèze
     doit couvrir `[x1+w+d, x1+bw]`.
2. **D2** — `set_widget` puis `drain_idles` : vérifier
     `canvas_height_greater == false`, `resize_queued` émis,
     top-level alloué (`was_allocated`), canvas = requisition.
3. **D3** — `set_widget` A→B : scroll/selection/anchors reset,
     `canvasSizeChanged(0,0,0)` émis, `findtext_state` vidé.
4. **D4** — `queueResize` sur nœud non rattaché : aucune
     émission `resize_queued`, aucun idle schedulé.
5. Couverture `SizeParams` (cache hit/miss), `markSizeChange`
     propagation `parentRef`, flags après `add_widget` sur
     widgets pré-queués, `calcScrollInto`/`adjustScrollPos`
     bornes, `intersects` hit-test — drivers `dw_anchors`,
     `dw_border`, `dw_simple_container` en replay de calls
     `View`.

## Frontière / non-bugs

- `StyleImage`/`ImgRenderer`/`LayoutImgRenderer` : adaptations
  `Rc`/`Weak` documentées, verbatim fonctionnel (A09 pour le
  fond imgbuf).
- `Embed`/`Resource`/`ComplexButtonResource` : factory +
  relief + `correctRequisitionOfChild` verbatim, y compris la
  typo amont `getAvailHeight` pour le pourcentage de largeur
  (`ui.cc:438` FIXME → `ui.rs:749,792`
  `l.widget_get_avail_height(e, false)` — reproduite).
- Dispatch `WidgetKind` vs vtables C++ : ordre de destruction
  respecté (tb→table→OOF→~Widget) — modèle D justifié par
  ADR-0005.
- `widget_children_for_detach`/`_container_change` : seams
  remplaçant les itérateurs `WIDGET_IN_FLOW|WIDGET_OOF_CONT` —
  équivalents fonctionnels.
- `remove_widget`/`queue_resize`/`draw`/`scroll_*`/`viewport_
  size_changed`/`attach_view`/`detach_view`/`process_mouse_*`/
  `get_widget_at_point` : relus — verbatim.
- `Style` refcount → `Rc<Style>` + `size_diffs` : verbatim.
- `num_to_str`/`multiply_with_per_length`/background tiling :
  verbatim (incl. overflow `****.`, division `rint(…/3.0)`).

## Résultat

**3 déviations à corriger en apply** (D1 pixel, D2+D3 cycle de
vie top-level — le trio forme le chemin « nouvelle page ») ;
D4 à décider (corriger le modèle `detached` ou documenter
l'écart de signal) ; D6 mineur (identité `Color`). Le reste du
périmètre est sémantiquement conforme — la machine de
négociation de tailles, de damage et de scroll est la couche
la plus fidèle auditée à ce jour, les écarts se concentrent
aux frontières lifecycle `addWidget`/`setWidget`/destruction.

## Apply — résolution

Branche `audit/05-dw-core` (worktree `C:/Projet/wt/dillorust-a05`) :

- **D1** corrigé — `style.rs` : seconds trapèzes gauche/droit en
  `x1 + 2*w + d` / `x1 + 1 - 2*w - d` (verbatim `points[0].x ± w`) ;
  coords exactes épinglées (`border_double_draws_two_bands`).
- **D2** corrigé — `add_widget` : `canvas_height_greater = false`
  + `widget_queue_resize(widget, -1, false)` verbatim.
- **D3** corrigé — `widget_delete` : `remove_widget` si
  `!detached` et sans parent (≙ `layout != NULL`), tombe marquée
  `detached`.
- **D4** corrigé — `detached` ≙ `layout == NULL` complet :
  nœud naît detached, levé à l'attache, reposé à la destruction ;
  `queueResize` non rattaché = branche NEEDS_* verbatim, gate SCM
  de `set_style` revivifié.
- **D5** corrigé — ordre `resizeFlag → markSizeChange →
  ALLOCATE_QUEUED` ancêtres verbatim.
- **D6** différé — identité `Color` (copie valeur) : perf-only,
  nécessite `Rc<Color>` partagé.

Tests : +5 (`border_double` coords, lifecycle add/set_widget,
queueResize unattached, émission `resizeQueued`) ; fixture
`anchors_scroll_to_named_position` réordonnée. Gates :
fmt / clippy `-D warnings` / `cargo test --workspace` (tous verts,
dont 122 `dillo-dw`) / `cargo doc` clean.
Changelog : `docs/audits/changelog/A05.md` ; `PORTING.md` annoté.

---

# Passe 2 (post-apply `a1751e2`, commits dw `f8e1fe7`…`0b9756d`)

## Delta code — relecture des 4 commits post-apply

| Commit | Changement | Verdict |
|---|---|---|
| `f8e1fe7` | `Layout::draw` pendant `scrollIdle` : re-queue `view.queue_draw(area)` — adaptation documentée du modèle de damage FLTK (`damage()` persistant ≙ `take_damage` consommateur) ; clamp `textWidth len<0→0` verbatim. | **Conforme** — équivalent observable, test de régression dédié. |
| `a20dc55` | Overrides textblock rappellent les impls de **base** `Widget::*` (`w_*_base`) ≙ liaison statique C++ amont — corrige la récursion infinie sur `Resized`. | **Conforme** — restaure la sémantique de dispatch amont. |
| `f9aa4c8` | Idles tagués par `layout_id` ≙ file privée `FltkPlatform` par onglet ; `BadnessAndPenalty` en `wrapping_*` ≙ wrap `int` C (incl. `ratio³` wrap amont) ; `tb_dump` diagnostic `#[doc(hidden)]`. | **Conforme** — `idle_callbacks_route_to_owning_layout` épingle le routage. |
| `0b9756d` | Gardes de progression `next_glyph` dans les boucles hit-test (amont s'en remet au NUL terminal ; nos slices n'ont pas de sentinelle → la garde est l'équivalent défensif fidèle). | **Conforme** — guards non observables sur chemins contractuels. |

## Oracle visuel activé

La golden suite `render_ref` (38 paires amont, pipeline complète)
est un oracle **pixel** direct pour ce périmètre : allocations,
damage, borders (y compris le fix `double` D1), `space_width`,
weights. Verdict L5 passé de « parité par lecture » à **parité
mesurée** sur le corpus amont.

## Statut des déviations passé 1

- D1–D5 : corrigées en `a1751e2`, reconduites (aucune régression
  dans les diffs relus).
- **D6** (`Color` identité → `ptr::eq` toujours faux) : **toujours
  ouverte**, toujours perf-only / pixel marginal en
  `SHADING_INVERSE` — reporter à l'apply consolidé.
- D7 (files différées modèle D) : design documenté, inchangé.

## Verdict passe 2

**Conforme.** Aucune déviation nouvelle dans le périmètre dw-core.
Le seul reste est D6 (perf-only, différé par décision passé 1) et
les gaps déjà documentés (drivers `test/dw/` non convertis — la
golden suite couvre partiellement ce rôle).

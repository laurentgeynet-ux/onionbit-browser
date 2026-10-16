# ADR-0005 — Modèle de propriété de l'arbre de widgets dw

- **Statut** : accepté (prototype phase 0 livré) · **Date** : 2026-10-16, amendé phase 0

## Contexte

`dw/widget.hh` repose sur des pointeurs croisés bruts, sans
refcounting :

```cpp
Widget *parent;       // propriétaire (delete récursif)
Widget *quasiParent;  // conteneur logique (floats/OOF)
Widget *generator;    // créateur de contenu — getGenerator()
                      // retombe sur parent si null (widget.hh:600)
Widget *container;    // ancêtre de containing block
```

Le layout incrémental traverse ces liens dans les deux sens
(`queueResize` enfant→parent, `sizeRequest` parent→enfant), y
compris **pendant** une itération de contenu. Les OOF managers
s'injectent dans la hiérarchie : le parent de *contenu* d'un widget
peut différer de son parent de *layout*. `lout::object::Signal`
ajoute des callbacks widget→récepteurs externes pendant ces
traversées.

Avec `#![forbid(unsafe_code)]` et l'interdiction de panique :

- **`Rc<RefCell<dyn Widget>>`** : cycles parent↔enfant (fuites),
  `BorrowMutError` garantis dès qu'un `markSizeChange` remonte
  pendant qu'un ancêtre itère — à écarter sauf preuve contraire.
- **`Box<dyn Widget>` en arbre propriétaire** : impossible
  d'exprimer `generator`/`container` (liens latéraux) ni les
  remontées enfant→parent.

## Options sur la table

| Option | Principe | Coût principal |
|---|---|---|
| **A — Arène + `WidgetId`** | `Vec<WidgetNode>` plat, tous les liens = ids ; méthodes prennent `&mut LayoutCtx` | réécriture des signatures ; callbacks Platform compliqués (double emprunt) ; fidélité au code amont moindre |
| **B — `Rc`/`Weak` mixte** | parent=`Rc` fort, enfants=`Rc`, remontées=`Weak` | casse `borrow_mut` aux mêmes endroits que C++ fait des appels croisés ; patterns `try_borrow` + requeue partout |
| **C — Layout propriétaire** | arbre dominé par `Layout` ; widgets mutés uniquement via méthodes `Layout` (contexte central comme amont en pratique) | sémantique plus proche du C (`Layout` orchestre tout) ; API à inventer |
| **D — Hybride** | arène pour les nœuds+liens, `enum WidgetKind` (pas de `dyn`) pour les implémentations ; `LayoutCtx` passe partout | combinaison A+C — probable candidat |

## Décision

**Option D retenue** — arène + `enum WidgetKind` + `LayoutCtx`,
mesurée sur `crates/dillo-dw-proto` (modules `a.rs`, `b.rs`, `d.rs`,
scénario de stress identique : `queue_resize` frère déclenché
pendant le `size_request` d'un `Textblock`, remontée
`mark_size_change` vers un ancêtre en cours d'itération).

Résultats mesurés :

- **A** (arène + `dyn`) : zéro panique, mais take/replace `Null` sur
  *chaque* appel récursif + downcast `Any` pour la config — le `dyn`
  coûte sans gain sur l'enum.
- **B** (`Rc`/`Weak`) : **panique réelle `BorrowMutError` au premier
  essai** — même `id_of` (`borrow()` immuable) échoue sur un nœud
  mut-emprunté. Version corrigée (`try_borrow_mut` + file de `Weak`
  déférés + drain post-itération) passe, mais le coût est *réparti*
  : chaque `parent->foo()` amont devient upgrade+try+branche, et
  l'ordre des événements de remontée se ré-ordonne.
- **D** : zéro panique nativement — le `&mut self` C++ devient
  `&mut LayoutCtx` + id ; les liens croisés `parent`/`quasiParent`/
  `generator`/`container` restent des `WidgetId` fidèles à
  widget.hh:127-146. Friction *localisée* : extraire les données du
  nœud (`mem::take`/copy) avant toute récursion, réécrire après —
  pattern mécanique à appliquer dans le port de `dw/`.

Contraintes rappelées : jamais de `Mutex` dans l'arbre ; les
`Signal` lout deviennent des files d'événements drainées *après* la
mutation (implémenté dans le proto via `drain_events`).

Règles de port dérivées pour `dw/` :

1. Tous les liens (`parent`, `quasiParent`, `generator`,
   `container`, `children`) sont des `WidgetId` dans l'arène
   `Layout` ; `getGenerator()`/`getQuasiParent()` gardent le
   fallback sur `parent`.
2. Toute méthode de widget prend `ctx: &mut LayoutCtx` ; avant un
   appel récursif, extraire les champs nécessaires (take/copy),
   rappeler, réécrire.
3. Les émissions (`extremesChanged`, `layout*`, destructions)
   passent par `ctx.events`, drainées après mutation.
4. Si les ids doivent survivre à des suppressions, passer
   `WidgetId` en (slot, génération) — à évaluer en phase 4.

## Conséquences

- Bloque la phase 4 (`dw/`) ; phase 0 doit livrer le prototype.
- Impacte la signature de *tous* les modules dw — à fixer avant tout
  port de `textblock`/`table`/`oof`.
- Si le prototype montre que B (`Rc`/`Weak`) tient sans panique, il
  minimise l'écart structurel avec le C++ et accélère le port.

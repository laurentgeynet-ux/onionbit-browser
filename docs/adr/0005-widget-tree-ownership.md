# ADR-0005 — Modèle de propriété de l'arbre de widgets dw

- **Statut** : ouvert (décision bloquante avant phase 4) · **Date** : 2026-10-16

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

**Prototype obligatoire en phase 0** avant engagement : crate
`dillo-dw-proto` avec 3 widgets (`Container`, `Textblock`, `Image`)
implémentant `queue_resize` qui traverse `parent` + `generator`
pendant une itération, sous les options A, B et D. La décision est
prise sur mesure : (1) zéro panique en scénario de stress,
(2) fidélité structurelle au C, (3) ergonomie pour les ~36 k LOC
suivants. Cet ADR sera amendé `accepté` avec le choix retenu.

Contraintes quel que soit le choix : jamais de `Mutex` dans
l'arbre ; les `Signal` lout deviennent des files d'événements
drainées *après* la mutation, pas des appels synchrones.

## Conséquences

- Bloque la phase 4 (`dw/`) ; phase 0 doit livrer le prototype.
- Impacte la signature de *tous* les modules dw — à fixer avant tout
  port de `textblock`/`table`/`oof`.
- Si le prototype montre que B (`Rc`/`Weak`) tient sans panique, il
  minimise l'écart structurel avec le C++ et accélère le port.

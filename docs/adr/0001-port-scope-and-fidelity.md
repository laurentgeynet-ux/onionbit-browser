# ADR-0001 — Port complet et fidélité sémantique

- **Statut** : accepté · **Date** : 2026-10-16

## Contexte

Port de Dillo (C/C++, `../dillo`, ~130 k LOC) vers Rust.
Question : que signifie « porter » — traduction, réécriture
fonctionnelle, ou compatibilité ?

## Décision

1. **Port complet** : chaque fichier source de `dlib/`, `lout/`,
   `dw/`, `src/` (incl. `IO/`), `dpi/`, `dpid/`, `dpip/` obtient un
   module Rust ou une ligne `won't port` justifiée dans
   `docs/PORTING.md`. Le port est terminé quand PORTING.md est à
   100 % de lignes résolues.
2. **Fidélité sémantique, pas littérale** : référence = comportement
   observable (tolérance de parsing, résultats de layout, formats de
   fichiers, wire HTTP/dpi). On écrit du Rust idiomatique ; on ne
   translittère pas le C.
3. **Checkout amont en lecture seule** : `../dillo` n'est
   jamais modifié ni copié en gros ; les données de test seules sont
   copiées dans `crates/<crate>/tests/data/`.
4. **Oracle = tests amont + golden renders** : `test/unit`,
   `test/dw`, `test/html/render` portés/remplacés par un harness
   headless ; les audits `.devin/skills/audit-*` contrôlent la
   déviation par vague.

## Conséquences

- Les choix « pratiques » divergents (parseur HTML5 standard,
  toolkit moderne, layout engine existant type Servo/leaflet) sont
  **hors scope** : ce serait un autre navigateur, pas un port.
- `won't port` exige une raison explicite (plateforme, build,
  doublon crate) — jamais « trop compliqué ».

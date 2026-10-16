---
name: audit-07-dw-table
description: "[V5] Audit sémantique vs Dillo — Tables dw : algorithme de dimensionnement, cellules, colspan/rowspan"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A07 — Tables dw

**Périmètre** : `dw/{table,tablecell,simpletablecell,alignedtablecell,
table_iterator}` + glue `src/table.cc` — algorithme de
dimensionnement de colonnes **maison** de Dillo ; parité pixel sur
les largeurs de colonnes.

## Fichiers

- **Upstream** (`$DILLO_ROOT`→`D:/Projet/dillo`) : `dw/table.cc`
  (~2k LOC, cœur de l'algo), `tablecell.*`, `simpletablecell.*`,
  `alignedtablecell.*`, `table_iterator.cc`, `src/table.cc` (parsing
  HTML→widget)
- **Upstream tests** : `test/dw/dw_table*.cc`, `test/html/render`
  (tables imbriquées, col/rowspan)
- **Rust** : `crates/dillo-dw/src/table/*.rs`,
  `crates/dillo-html/src/table.rs`

## Checklist spécifique

- [ ] `colspan`/`rowspan` : comptabilité cellMax/col tracking amont
      exacte (cellules fantômes, rangées courtes)
- [ ] Dimensionnement : `calcExtremes`/`apportion` — poids des
      min/max content vs `width=` forcés, redistribution du surplus,
      ordre de passage des colonnes
- [ ] `border-spacing`, `cellpadding`, `cellspacing`, `border`
      attribut HTML vs CSS — cumul identique
- [ ] Table width : `width="50%"`, absolue, auto — base du %
      (containing block) comme amont
- [ ] Cellules vides, rangées vides, contenu hors `<td>` (foster
      parenting côté html glue)
- [ ] Tables imbriquées : négociation extremes parent↔enfant
- [ ] Vertical align des cellules (`valign`), `height` forcée
- [ ] `table_iterator` : ordre de traversée (raster order ?)
- [ ] Re-layout incrémental : lignes ajoutées au fil du parsing
      changent les largeurs — même re-dimensionnement qu'amont
- [ ] `src/table.cc` : attributs HTML hérités (`align`, `bgcolor`),
      `<col>`/`<colgroup>` supporté ?

## Protocole d'audit — deux modes

Défaut **`rapport`**, `apply` sur rapport relu.

### Analyse — 4 étapes + 5 lentilles

1. Cartographie Rust : structures col/row, passage de l'algo.
2. Upstream : `dw/table.cc` lu en entier — les passes
   (`calcExtremes` → répartition) sont la spec ; devdocs éventuels.
3. Lentilles : **L1** bornes (colspan>nombre de cols, table 0 col,
   cellule 0-size, width absurdes) · **L2** tolérance (balisage
   table cassé — le HTML glue produit des arbres tordus, dw doit
   encaisser) · **L3** cycle de vie (cellules retirées, re-layout
   pendant streaming) · **L4** incrementalité (largeurs qui bougent
   au fil du chargement = comportement visible) · **L5** parité
   pixel (largeurs de colonnes à l'entier près — golden renders
   `test/dw` + `test/html/render`).
4. Test gap analysis.

### Mode `rapport`

5. `docs/audits/reports/A07_dw-table.md` — verdicts+preuves,
   déviations sketchées, frontière (textblock interne aux cellules,
   OOF dans cellules ?), non-bugs.
6. Commit pathspec-limited. 7. STOP.

### Mode `apply`

8. Worktree `C:/Projet/wt/dillorust-a07` branche `audit/07-dw-table`.
9. Fixes + tests de régression : tables minimales (largeurs de
   colonnes attendues en dur) par déviation.
10. Gates fmt/clippy/test/doc ; `docs/audits/changelog/A07.md` ;
    PORTING.md.
11. Commit → rebase → merge `--no-ff` → cleanup → `## Résultat`.

## Pièges connus

- L'algo de répartition amont n'est **pas** le standard CSS — ne
  pas implémenter le modèle W3C, porter le code.
- Arrondis cumulés : le surplus distribué colonne par colonne en
  entier — dernier pixel à la dernière colonne ? Vérifier l'ordre.
- `rowspan` + hauteurs variables : cellMax tracking par rangée.
- Attributs `border` HTML ont une sémantique présentationnelle
  (bordures cellules aussi) — non-CSS hint à poids spécifique.

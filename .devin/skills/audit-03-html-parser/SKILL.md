---
name: audit-03-html-parser
description: "[V2] Audit sémantique vs Dillo — Parseur HTML tag-soup (html.cc), doctree, entités, plain/form glue"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A03 — Parseur HTML & doctree

**Périmètre** : `src/html.cc` machine d'état tolérante, entités
`html_charrefs.h`, `doctree.hh`, `plain.cc`, glue `table.cc`/`form.cc`
côté parsing — **le risque n°1 de déviation : un parseur « correct »
diverge du parseur Dillo**.

## Fichiers

- **Upstream** (`$DILLO_ROOT`→`D:/Projet/dillo`) : `src/html.cc` (~5k
  LOC), `html.hh`, `html_common.hh`, `html_charrefs.h`, `doctree.hh`,
  `plain.cc`, parties parsing de `table.cc`/`form.cc`
- **Upstream tests** : `test/html/render/*` (oracle visuel),
  `test/html/manual`
- **Rust** : `crates/dillo-html/src/{parser,tokenizer,entities,
  doctree,plain}.rs`

## Checklist spécifique

- [ ] Fermetures implicites : règles exactes de `Html_tag_close_*`/
      table de compatibilité (p dans p, li dans li, td/tr…)
- [ ] Éléments inconnus/malformés : `<foo`, `<>`, `</ >`, attributs
      sans quote/valeur, `=` manquant — récupération amont identique
- [ ] Entités : `&nbsp;` `&amp` sans `;`, numériques `&#x…;`/`&#…;`
      invalides/dépassement → même fallback qu'amont
- [ ] Espaces : collapsing rules (cf. `devdoc/dw-textblock-collapsing
      -spaces`) — pré/nowrap, espaces en début/fin de bloc
- [ ] `<table>` : construction implicite tbody, cellules hors tr,
      contenu avant table (foster parenting amont = son propre algo)
- [ ] Commentaires `<!--` non fermés, CDATA, `<?php`, DOCTYPE ignoré
- [ ] Feed incrémental : parser reprend exactement où le chunk
      précédent s'arrête (tag coupé en deux chunks)
- [ ] Encodage : tout passe par `a_Utf8_decode` lenient — séquences
      invalides → mêmes caractères de remplacement
- [ ] `plain.cc` : wrapping <pre>, pas d'entités interprétées
- [ ] Forms : parsing `<input>` types inconnus → fallback text
- [ ] Limite profondeur nesting (upstream en a-t-il une ? reproduire
      ou documenter)

## Protocole d'audit — deux modes

Défaut **`rapport`**, `apply` sur rapport relu.

### Analyse — 4 étapes + 5 lentilles

1. Cartographie Rust : états tokenizer, arbre doctree, où la
   récupération diffère du C.
2. Upstream en profondeur : la table d'imbrication de `html.cc`,
   ordre des tests de fermeture, `Html_process_tag` chemins
   complets — **les états de récupération sont la spec**.
3. Lentilles : **L1** fuzzing/bornes (tout byte stream doit
   produire un arbre, jamais de panique — fuzz différentiel vs
   binaire C) · **L2** tolérance (upstream *toujours* récupère ;
   un `Err` Rust qui avorte le doc = déviation critique) ·
   **L3** cycle de vie (widget tree partiel quand le parsing
   s'arrête mid-stream, anchors `#id` résolus tardivement) ·
   **L4** incrementalité (push par chunk, re-render progressif
   identique) · **L5** parité observable (arbre produit = arbre
   amont sur la suite `test/html` — comparaison structurelle
   automatisée si possible).
4. Test gap analysis : pages `test/html` non couvertes.

### Mode `rapport`

5. `docs/audits/reports/A03_html-parser.md` — verdicts+preuves
   (`fichier.rs:ligne` vs `src/html.cc:ligne`), déviations sketchées,
   test gap, frontière (dw hooks : quels appels widget le parser
   fait-il et dans quel ordre ?), non-bugs.
6. Commit pathspec-limited. 7. STOP.

### Mode `apply`

8. Worktree `C:/Projet/wt/dillorust-a03` branche
   `audit/03-html-parser`.
9. Fixes + tests de régression : pour chaque déviation, un cas
   tag-soup minimal + assertion d'arbre.
10. Gates fmt/clippy/test/doc ; fragment `docs/audits/changelog/
    A03.md` ; rows PORTING.md enrichies.
11. Commit branche → rebase → merge `--no-ff` → cleanup →
    `## Résultat`.

## Pièges connus

- La table de compatibilité de tags de `html.cc` est **la spec** —
  ne pas « améliorer » vers HTML5.
- `&amp` sans `;` suivi de `=` ou alphanum : règle de lookahead
  amont spécifique.
- Espaces insécables `&nbsp;` ≠ espace : impact direct sur le
  line-breaking dw (frontière A06).
- `Html_write_raw`/push model : le parser peut être suspendu
  mid-tag — état de continuation à reproduire fidèlement.
- Attributs dupliqués : amont garde le premier ou le dernier
  (vérifier dans le C, pas supposer).
- `dReturn_if_fail` ≠ contrat strict : en release amont continue
  sur état incohérent → Rust doit dégrader pareil.

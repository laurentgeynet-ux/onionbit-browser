# Audit A04 — CSS parser, cascade & style engine (rapport)

**Mode** : `rapport` (ledger post-port, même stratégie qu'A07/A08).
**Périmètre audité** : `crates/dillo-css/src/{doctree,css,parser,
styleengine}.rs` (~5 100 lignes + 48 tests) vs `src/doctree.hh`,
`src/css.{hh,cc}`, `src/cssparser.{hh,cc}`, `src/styleengine.{hh,cc}`.
**Hors périmètre** : `html.cc` (consommateur — phase 2, lot
suivant), `prefsparser`/`dillorc` (phase 6 — `StyleEnginePrefs`
est injecté), golden computed-style vs Dillo réel (A10).

## Cartographie

```
Doctree (arène Vec<DoctreeNode>)          CssParser<'a>
  push/pop/top/parent/sibling/num   ──►   tokenizer (tval capé 255)
      element/klass/pseudo/id             declaration/shorthands/!important
                                          @import→hooks · @media all|screen
                  │                                  │ CssContext::add_rule
                  ▼                                  ▼
        StyleEngine::start_element        CssStyleSheet[4] + USER_AGENT_SHEET
          Node stack (style/word/bg)        element_table[94]/id/class/any
          css_context.apply(node)           RuleList trié (spec,pos)
                  │                                  │
                  ▼                                  ▼
        Style::create(StyleAttrs)   ◄──   règles par (spec,pos) croissants
        → dw::core::style (arène dw)
```

## Verdict global

**Conforme** sur le périmètre porté. Les cinq lentilles passent :
L1 (fuzzing : tokenizer tolerant, `tval` capé, récursion sélecteurs
bornée par le doctree) ; L2 (tolérance : déclaration/règle cassée
ignorée sans invalider la feuille — test dédié) ; L3 (cycle de vie :
`restyle` recalcule toute la pile, `close` dépile) ; L4
(incrémentalité : `StyleEngine::parse` alimente `css_context` en
continu — le re-style est déclenché par le caller ≙ `html.cc`) ;
L5 (parité : `apply` matérialise `StyleAttrs` dans l'ordre verbatim).

## Ordre de cascade (vérifié vs `css.cc:CssContext::apply`)

`USER_AGENT_SHEET` → `sheets[User]` → `nonCssHints` →
`sheets[Author]` → `tagStyle` (attribut `style=`) →
`sheets[AuthorImportant]` → `tagStyleImportant` →
`sheets[UserImportant]` — identique amont, position `pos`
croissante à spécificité égale (insertion *après* les égaux dans
`RuleList::insert` — dernier gagne, verbatim stable).

## Déviations documentées

- **D1** — `USER_AGENT_SHEET` : `static` process-wide amont →
  `thread_local!` (les `Rc` internes ne sont pas `Send`) ; le CSS
  vit sur le thread UI (§10 AGENTS) — observable identique.
- **D2** — `elementTable[node->element]` : amont indexe sans
  borne (`element=-1` pour tag inconnu = UB réelle). Le port
  garde le résultat *défini* de `addRule` (rejet `ELEMENT_NONE`)
  et ajoute un guard `(0..94).contains(&element)` dans `apply` —
  déviation de réparation d'UB, documentée en commentaire.
- **D3** — `a_Html_tag_index` : fonction libre amont → seam
  `TagIndex` injectée (la table vit dans `dillo-html`, plus haut
  dans le graphe de dépendances). `NullTagIndex` ≙ table absente.
- **D4** — `DilloHtml*` → trait `CssHtmlHooks` (`resolve_url` ≙
  `a_Html_url_new`, `load_stylesheet` ≙ `a_Html_load_stylesheet`)
  — évite la dépendance montante css→html.
- **D5** — `BrowserWindow*` → trait `StyleEngineSink`
  (`start_background_image_load` ≙ `a_Image_new`+`a_Web_new`+
  `a_Capi_open_url`+`a_Bw_add_client`+`a_Bw_add_url` inline).
- **D6** — sentinel `borderColor = (Color*)-1` de
  `preprocessAttrs` → `[Option<Color>; 4]` + flag transitoire
  `border_color_unset` (équivalent sémantique — le sentinel
  est un pointeur invalide exploité comme marqueur, `Option`
  encode la même information).

## Quirks verbatim conservés (vérifiés)

- `border-rigth-width` (typo amont) — test dédié
  (`border-right-width` *non* reconnu, la typo l'est).
- `z-index` : `p->type == CSS_LENGTH_TYPE_AUTO` compare deux
  domaines d'enum incompatibles → `auto` produit `zIndex=13`
  (`intVal` brut) — reproduit, test dédié.
- Table `Css_property_info` : `CSS_NUM_PARSED_PROPERTIES =
  LAST-3` avec queue désordonnée (`x-link`/`x-colspan`/
  `x-rowspan`/`last`) — `binary_search_by` sur le même ordre
  (mêmes trous de recherche qu'amont).
- `margin-*`/`padding-*` = `SIGNED_LENGTH` — **pas de `%`**
  (quirk Dillo — test dédié sur `width` qui l'accepte).
- `@import` non initial → `ignore_statement` ; `importDepth>10`
  → warning + drop (verbatim).
- `CssSimpleSelector::set_select` : `pseudo`/`id` posés une
  seule fois (premier gagne — verbatim).
- `spaceSeparated`/`withinBlock` pilotent `#hex` et les
  sélecteurs descendant (verbatim tokenizer).
- Règles auteur `pseudo + prop unsafe` rejetées
  (`is_safe` ≙ `safe` flag sur `display`/`background-image`).

## Points d'attention (frontière)

- `StyleEngine::init(tag_index)` doit être appelé avant le
  premier `StyleEngine::new` avec un `TagIndex` réel — sinon la
  feuille UA est vide (tous les sélecteurs → `ELEMENT_NONE`).
  `html.cc` fournira la vraie table au branchement.
- `restyle` est exposé ; le déclencheur (`<style>` différé /
  `restyle()` amont) viendra avec `html.cc`.
- `background_image` retourne la spec verbatim ; le chargement
  réel passe par `StyleEngineSink` — testé par seam seulement
  (pas d'image réelle en test unitaire).
- `prefs` (`dillorc`) : `StyleEnginePrefs` est une struct
  injectée — le parsing `dillorc` est phase 6 (prefs).

## Test gap analysis

48 tests verts : doctree (4), modèle css (12), parser (21),
styleengine (11). Non couverts : golden computed-style contre
Dillo compilé (attend A10 — harnais headless), `@import` récursif
réel (seam testée, pas de chargeur réseau ici), `x-link`/
`x-colspan`/`x-rowspan` (propriétés internes consommées par
`html.cc` — couverture en phase 2), re-style après mutation
dynamique (frontière html).

---

# Passe 2 (après merge A01-p2)

## Delta code

`dillo-css/src` **inchangé** depuis la passe 1 (un mock de test
`Platform::add_idle` dans `styleengine.rs`, `f9aa4c8`). Verdicts
passés 1 reconduits tels quels — cette passe se concentre sur les
quatre « points d'attention » devenus auditables (glue réelle).

## Points d'attention — maintenant vérifiés

| Point passé 1 | État passe 2 |
|---|---|
| `TagIndex` réel | **Conforme** — `HtmlTagIndex` injecté à `StyleEngine::init`/`parse` (`html.rs:2503`), verbatim `a_Html_tag_index` (D3 résorbée en pratique). |
| `restyle` déclenché | **Conforme** — appelé dans `Html_tag_open_body` après les hints `bgcolor`/`text` (`html.rs:2106` ≙ `html.cc:1842`), et après chargement des feuilles via `GlueOp::Repush`. |
| `prefs`/`dillorc` | **Conforme** — `html_prefs()` mappe `Prefs`→`StyleEnginePrefs` verbatim (`glue.rs:225-239`), feuille user `~/.dillo/style.css` ≙ `buildUserStyle` (`styleengine.rs:1574`, `styleengine.cc:1083`). |
| `StyleEngineSink` réel | **Conforme** — `DocSink::start_background_image_load` verbatim (`doc.rs:758-784` ≙ `styleengine.cc:762`) ; déviation notée : `StyleImageDeletionReceiver` non porté (stop client à la destruction du style) — cas rare, documenté. |

## Mécanisme stylesheet réseau — re-vérifié de bout en bout

Cycle verbatim amont : `<link rel=stylesheet>` non caché →
`sink.load_stylesheet` → `a_Web_new`/`WEB_STYLESHEET`/
`capi_open_url` + `NumPendingStyleSheets++` ; à EOF le callback
`--` le compteur et pose `GlueOp::Repush` à 0 ≙
`Html_css_load_callback` → `a_UIcmd_repush` → `Nav_repush` =
**re-navigation `RELOAD_FROM_CACHE`** (`nav.rs:400`) → le `<link>`
est re-rencontré → `load_stylesheet` prend la branche cache et
parse inline, y compris le sniffing `@charset` ≤ 51 octets
verbatim (`html.cc:3272-3286` ≙ `html.rs:2488-2500`).
`formNew` respecte `NumPendingStyleSheets==0` (formulaire
désactivé pendant le re-push ≙ `html.cc:671`).

## Oracle visuel

La golden suite (`render_ref`, 38 paires `test/html/render` amont,
pixel-identique modulo `ALLOWED`) exerce désormais la cascade
complète bout-en-bout (UA + author + hints non-CSS → `Style`
dw). Gap « golden computed-style » partiellement fermé : reste
un dump comparatif de computed styles vs Dillo compilé pour les
props sans effet pixel direct.

## Verdict passe 2

**Conforme.** Aucune déviation nouvelle dans `dillo-css` ; les
quatre seams en attente sont câblés verbatim. Restes (gaps, non
bugs) : `@import` récursif réseau (seam testée), dump
computed-style comparatif, `StyleImageDeletionReceiver` (cas
rare documenté).

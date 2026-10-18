# Audit A03 — Parseur HTML & doctree

**Mode** : `rapport` (ledger post-port, même stratégie qu'A04/A07).
**Périmètre audité** : `crates/dillo-html/src/` (~10 500 lignes) —
`html.rs`, `common.rs`, `entities.rs`, `tags.rs`, `sink.rs`,
`plain.rs`, `table.rs`, `form.rs` vs `src/html.cc` (~5 300 LOC),
`html.hh`, `html_common.hh`, `html_charrefs.h` (2 125 entrées),
`doctree.hh`, `plain.cc`, `table.cc`, `form.cc` amont.
**Hors périmètre** : golden renders `test/html/render` (harnais
A10), `seam BrowserWindow` réelle (phase 7 — `HtmlSink` injecté),
`form_send` réseau réel (phase 6 — sink), `minidoc.rs` (pilote
phase 0.5, hors spec).

## Cartographie

```
DilloHtml ≙ struct DilloHtml (monolithique amont)
  ├── core : Rc<RefCell<HtmlCore>>    ≙ champs parser (stack,
  │     in_flags, stash, line_no, DocType, page_bugs …)
  ├── shared : Rc<RefCell<HtmlShared>> ≙ BW/page/style-engine
  │     layout : Rc<RefCell<Layout>>  ≙ Layout* + Widget* dw
  │     se : StyleEngine              ≙ styleEngine (dillo-css)
  │     bw : Box<dyn HtmlSink>        ≙ BrowserWindow* + a_UIcmd_*
  │     prefs : HtmlPrefs             ≙ prefs (dillorc)
  └── maps : Rc<RefCell<Vec<ImageMap>>> ≙ html->maps (ImageMapsList)

write(buf_cumulatif, eof)  ≙ cache→Html_write_raw(Client->Buf)
  └── write_raw : scan tag / espace / verbatim par chunk,
      état `ParseMode` + `space_start` persistants entre chunks
```

`DilloPlain` ≙ `plain.cc` : `expand_tabs` + `add_line` + anchors
`line-%d`, offsets sur buffer cumulatif.
`HtmlForm`/`HtmlInput` ≙ `form.cc` : resources `Embed` via
`ResourceFactory`, `FormReceiver` `Weak` (cycle brisé ≙ `form` ptr).

## Checklist — verdicts

- [x] **Fermetures implicites** : `triggers_optional_close`
      verbatim (P/DT par block `(flags&1)==0` ; LI par LI ; TD/TH
      par TD/TH/TR/TBODY ; TR par TR/TBODY ; DD par DD/DT ;
      OPTGROUP par non-OPTION ; OPTION toujours) —
      `tags.rs:937` vs `html.cc:3651`.
- [x] **Cross-nesting** A/BUTTON/SELECT/TEXTAREA :
      `forbids_cross_nesting` verbatim `tags.rs:963` vs
      `html.cc:3690`.
- [x] **Sections magiques** html/head/body :
      `Html_test_section` portée (`html.rs` — ouvertures
      implicites, `Num_HEAD`, `IN_HEAD`→`</head>` auto, warning
      doctype verbatim `"The required DOCTYPE declaration is
      missing. Handling as HTML4."`).
- [x] **Bug amont `Tags[stack_idx]`** : reproduit avec borne
      documentée (`html.rs` `tag_cleanup_at_close` — `None` → pas
      inline → `expected`), vs `html.cc:3834`.
- [x] **Attributs dupliqués** : premier gardé — `Found`→`FINISHED`
      dès le premier match (`html.rs` `get_attr2`, verbatim
      `html.cc:4189`).
- [x] **Entités** : `;` optionnel HTML≤4.01 hors attr ; `&ampy` →
      nom entier `ampy` inconnu → littéral (vérifié `html.cc:986` —
      test `entities_no_semicolon_legacy` corrigé en conséquence) ;
      numériques hors plage + `ms_stupid_quotes` 145-151 ; `&apos;`
      XHTML only ; dichotomie `html4_str`/`html5_str` verbatim.
- [x] **Incrémental** : `write_raw` reprend à `start_ofs` du buffer
      cumulatif ; tag coupé en deux chunks → `ParseMode` conservé
      (test `soup_incremental_token_split` vert).
- [x] **`plain.cc`** : buffer cumulatif (fix `start_ofs` vs slice
      courante — panic éliminé, verbatim `Client->Buf`), ligne
      finale à EOF, anchors `line-%d`.
- [x] **DOCTYPE** : parsing HTML4/5/XHTML + warnings verbatim.
- [x] **Limite de profondeur** : aucune amont (grep `html.cc`) —
      aucune ajoutée, fidèle.
- [x] **`<form>`/`<input>`** : 15 types verbatim, inconnu → text ;
      encodage urlencode/multipart verbatim (`generate_boundary`,
      `name.x`/`name.y` image vs `x`/`y` anonyme — `form.cc`
      relu et réparé).
- [x] **`tag_index`** : table `Tags[]` 94 entrées verbatim —
      `bdo`/`caption`/`col`/`colgroup`/`noframes` absents amont
      (commentaires seulement) → absents du port (test corrigé,
      pas la table).

## Lentilles

**L1 fuzzing/bornes** — `s_top` invariant `stack[0]` (dummy
fallback ≙ `dReturn_if_fail` tolérant amont) ; `TAGS.get(idx)`
borné partout (index pile négatif → `-1` ≙ sentinelle amont) ;
`content_type_cmp` inversion bool/int corrigée dans `cache.rs`
(découverte d'audit). Aucun panic hors `#[cfg(test)]`.

**L2 tolérance** — tous les chemins dégradent (verbatim) :
`get_attr2` → `Option`, tags inconnus → warning + ignorés,
entités invalides → littéral + `bug_message` compté `page_bugs`.

**L3 cycle de vie** — anchors `#name` différées (`anchors` map
consultée tardivement ≙ `Html_check_name_id`), widget tree
partiel mid-stream (`finish_parsing` EOF cleanup verbatim).

**L4 incrementalité** — `write` par chunk, `process_space`/
`process_word` par token, `image_new` lazy + `Html_load_image`
via sink à la fermeture du tag (verbatim `Html_tag_open_img`).

**L5 parité observable** — warnings textuellement verbatim
(`"Unexpected closing tag: </%s> -- expected </%s>."` etc.) ;
table `Tags[]` octet-par-octet vs amont ; `EndTag` `'O'`/`'R'`/
`'F'` → enum ; bugs trouvés : 2 (voir D1/D2).

## Déviations / bugs trouvés

### D1 — `DilloPlain::write` `start_ofs` hors borne
**Sévérité : corrigée.** `&buf[self.start_ofs..]` panic quand
un nouveau chunk était plus court que l'offset accumulé — le
port traite désormais `buf` comme cumulatif verbatim
(`Client->Buf` amont), `start_ofs`/`last_line_start` indexés
dedans. Test `plain_write_lines` vert.

### D2 — `add_textblock` `RefCell` double-borrow
**Sévérité : corrigée.** `word_style()`/`style()` empruntent
`layout.borrow_mut()` — calculés **avant** le borrow de
`add_textblock` (verbatim : amont appelle `wordStyle()` puis
`HT2TB` sans verrou ; ordre des effets de bord préservé).

### D3 — `content_type_cmp` inversion sémantique
**Sévérité : corrigée (hors crate).** Le port retournait `bool`
(équivalent) où `cache.rs` attendait l'`int` amont (≠0=différent)
— condition inversée au site d'appel corrigée.

### D4 — `Rc<dyn SubResource>` → `Rc<dyn Resource>` upcasting
**Sévérité : nulle (toolchain).** `form.cc` caste
`Resource*`→sous-traits partout ; rust-version bumpée 1.86
(trait upcasting stable) + downcast accessors object-safe
ajoutés à `Resource` (defaults `None`).

### D5 — `@import` hook réentrant
**Sévérité : nulle (design).** `load_stylesheet` amont est
réentrant (`se->parse` inline) ; le hook Rust retourne les octets
en cache (`Option<Vec<u8>>`) et le parser récurse inline —
ordre de cascade strictement identique, `None` = fetch async →
repush. Documenté dans `parser.rs`.

### D6 — `HtmlSink` seams non encore câblées au vrai shell
**Sévérité : attendue (phase 7).** `load_image`, `goto_url`,
dialogs, `select_file`, `form_send` (`a_Capi_open_url`) passent
par le trait `HtmlSink` ≙ `BrowserWindow*` — mock injecté en
tests, impl réelle = phase browser-glue (A11 partiel).

## Non-bugs / points vérifiés

- `Tags[stack_idx]` (html.cc:3834) : index pile utilisé comme
  index `Tags` — bug amont reproduit verbatim avec borne.
- `triggers_optional_close` inclut `i_TBODY` pour TD/TH/TR —
  verbatim (pas une extension).
- Attributs dupliqués → **premier** gardé (`Found`→`FINISHED`),
  pas le dernier (vérifié, pas supposé).
- `&amp` sans `;` : heuristique verbatim — attr → requis, XHTML/
  HTML5 → requis, HTML≤4.01 → toléré *si le nom entier match*
  (`ampy` ≠ `amp` → littéral, test corrigé).
- `isindex` verbatim (prompt+field+submit hidden).
- `MAX_BORDER` 10, `IMAGE_MAX_AREA` verbatim.
- Warning doctype sur **tout** doc sans `<!DOCTYPE>` — verbatim
  (`Html_test_section` IN_HTML==false+DT_NONE) ; les 28 tests
  préfixent un doctype HTML4.

## Test gaps

- [ ] `test/html/render/*` oracle visuel — harnais golden-layout
      (dépend A10 platform).
- [ ] `<table>` foster-parenting amont (contenu avant `<tr>`) —
      glue `table.rs` portée mais non exercée end-to-end.
- [ ] Frameset/frame/iframes — handlers portés, pas de sink réel.
- [ ] `<map>`/`<area>` hit-testing — structures portées
      (`Shape` R/C/P verbatim), coords non testées.
- [ ] `form_send` POST/multipart réel — encodage testé, envoi
      réseau = phase 6.
- [ ] Fuzz différentiel vs binaire C (L1) — non automatisé.

## Verdict

**Conforme** sur le périmètre porté : machine d'état tolérante
verbatim (sections magiques, fermetures implicites/croisées,
stash, entités legacy, incrémental chunk-par-chunk), glue
table/form/plain fidèles, quirks amont reproduits et documentés.
3 bugs de port corrigés pendant l'audit (D1 plain offset, D2
borrow ordering, D3 bool inversion). 28 tests `dillo-html` verts ;
workspace/clippy/doc/fmt verts. Reste : golden renders (A10) et
câblage `HtmlSink` réel (phase 7).

---

# Passe 2 (après merge A01-p2, commit `5ebb518`+)

## Ce qui a changé depuis la passe 1

- Fuzz phase 9 (`99ca279`) : `DilloHtml::write` clamp `start_ofs`
  (fix verbatim — le contrat `Client->Buf` cumulatif reste) ;
  campagne 30k×6 + 10k×7 clean sur la pipeline html.
- Golden suite (`f9aa4c8`) : **38/38 paires amont** `test/html/
  render` rejouées pixel-identiques via la pipeline réelle
  (parser→style→dw→tiny-skia, dicache réel) — `ALLOWED` = les 4
  XFAIL amont + `svg-current-color` (AA rasterizer resvg). Le gap
  « oracle visuel » de la passe 1 est fermé côté harnais.
- `arm_widget_delete` (`0b9756d`) : cycle de vie corrigé — le
  widget toplevel possède le `Rc<DilloHtml>` ≙ `setDeleteCallback
  (Html_free, this)` ; fini les `Weak` morts sur clics liens/forms.
- `HtmlSink` réel câblé : `dillo-browser::doc::DocSink` —
  `load_image`/`load_stylesheet` verbatim (`WEB_IMAGE`/`WEB_
  STYLESHEET` → `capi_open_url` → `add_client`/`add_url`,
  repush à compteur 0), navigation/dialogs via `GlueOp` (queue
  ≙ synchronicité amont, évite les réentrance RefCell). D6 passe-1
  → **conforme**.

## Déviations (passe 2)

| # | Déviation | Lentille | Sévérité | Preuves |
|---|-----------|----------|----------|---------|
| D7 | `HtmlInput::get_resource` emprunte `layout.borrow()` — invoqué depuis les receivers `clicked`/`activate` qui tournent **sous** `layout.borrow_mut()` (dispatch `Layout::button_event` → `process_mouse_event_button` → `event_handler`) → **panic `RefCell`** sur tout événement de contrôle de formulaire. Signalé « connu » dans `0b9756d`, toujours ouvert. Fix sketch : même traitement que `GlueOp::LinkVisited` — extraire le `Rc<dyn Resource>` hors emprunt (l'id `embed` suffit à la récupération différée) ou résoudre le resource avant dispatch. | L3/L1 | **fonctionnelle** | `form.rs:318-328,576-578` vs `form.cc` (`getResource` = pointeur direct) ; `layout.rs:1356-1376` |

## Re-vérification passe 1

Code parser quasi inchangé depuis `12e0ccb` (fuzz clamp +
`arm_widget_delete` seulement) — verdicts passés 1 reconduits ;
tests `dillo-html` 29 verts, golden suite verte.

## Test gaps résiduels

- Foster-parenting/iframe/frameset et hit-testing `<map>`/`<area>`
  : toujours sans fixture end-to-end (ni amont ni ici).
- `form_send` réseau réel : encodage couvert, l'émission via
  `capi_open_url` (sink réel) reste non exercée en test.
- Fuzz différentiel vs binaire C : smoke runner interne seul.

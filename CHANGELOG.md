# Changelog

All notable changes to DilloRust are documented here.
Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **phase 9 — golden render harness** : `dillo/tests/render_ref.rs`
  rejoue les 39 paires `test/html/render/*.html`/`.ref.html` via la
  vraie pipeline (`DilloHtml`→`StyleEngine`→`Layout`→`SkiaView`→
  tiny-skia), images réelles via dicache (`pic.png`), canvas 780×580
  + scrollbar 15 px verbatim `PREFS_GEOMETRY_DEFAULT_*`/
  `SCROLLBAR_THICKNESS`. Comparaison pixel `golden::diff_pixmaps`,
  dumps PNG/textblock via `DILLO_GOLDEN_DUMP`. `ALLOWED` documenté :
  les 4 XFAIL amont (`margin-auto`, `max-width-html`, `span-padding`,
  `table-td-width-percent` — non-supports assumés par Dillo) +
  `svg-current-color` (AA de bords resvg ≠ nanosvg seulement).
- **phase 9 — packaging + perf pass** : `packaging/package.{sh,ps1}`
  assemblent l'archive autonome (layout dpid verbatim —
  `dpi/<service>/<service>.dpi`, `dillo-dpid` renommé `dpid` pour
  `Dpi_start_dpid`), `packaging/dillorc` = gabarit amont. Bench
  `render_bench` (10× par fixture, médianes) + mesure RSS idle dans
  `docs/perf.md` : ~60 ms démarrage, ~40 Mo WS, ~34 ms/page médiane.
- **TLS** : provider crypto `ring` (ADR-0003) au lieu de `aws-lc-rs`
  (défaut rustls) — plus de compilation C BoringSSL dans l'arbre.

- `dillo-browser` : test `icon_probe` (décodage XPM `#RGB`/
  `#RRGGBB`/12&16 bits/canal, `#None`→transparent, ressources
  embarquées réelles).

- **phase 9 — fuzzing** : crate `fuzz/` hors workspace, 6 cibles
  `fuzz_target!` (html pipeline réelle, css, decode chunked/
  gzip/deflate/br/charset, dpip wire, fichiers cookiesrc/hsts/
  domainrc, url) + runner `smoke` stable mutateur seedé
  (libFuzzer non linkable sous MSVC — cibles prêtes pour CI/Linux).
  Documentation `docs/fuzzing.md`.

### Fixed

- **phase 9 — test live, 2ᵉ passe (clics liens, popups, saisie)** :
  - **clic sur lien figeait le navigateur** : `SkiaPlatform::
    next_glyph` renvoyait `end_of_char(i)` (dernier octet du
    caractère *courant* = `i` pour l'ASCII) au lieu du début du
    caractère suivant (≙ `fl_utf8fwd`) → la boucle de hit-test de
    `tb_send_selection_event` n'avançait jamais → boucle infinie sur
    tout clic texte. Même avance stagnante corrigée dans
    l'hyphenator et la mesure smallcaps ; gardes de progression et
    de bornes ajoutées (malformed shaping ne doit jamais pendre ni
    slicer hors texte — amont s'en remet au NUL final).
  - **clic sur lien sans effet** : `DilloHtml` n'était retenu que par
    la closure du cache client — droppé en fin de fetch → le `Weak`
    de `HtmlLinkReceiver`/`DilloHtmlReceiver` (forms) cassait →
    `enter`/`clicked`/`pressed` en no-op silencieux. Le port
    reproduit désormais la propriété amont
    `setDeleteCallback(Html_free, this)` : `arm_widget_delete` fait
    de la delete callback du widget toplevel le possesseur du
    document (`Rc`), libéré pile à la destruction (≙ `Html_free`).
  - **panic latent RefCell** : le feedback « visited » dans
    `clicked` empruntait le `Layout` déjà sous `borrow_mut` pendant
    `button_event` → différé via `GlueOp::LinkVisited` (drain post-
    event, comme `open_url`).
  - **popups jamais rendus** : `take_popup` n'était drainé qu'en
    tests — Tools/File/clic droit construits mais invisibles.
    Overlay natif possédé par `Chrome` : dessin au-dessus du layout,
    hit-test modal, sous-menus, fermeture Échap/clic hors-zone
    (ADR-0002 — trou fonctionnel A10 clos).
  - **barre d'adresse** : caret/sélection/hit-test mesurés via le
    shaping réel cosmic-text (`Buffer::hit`, positions glyphes) au
    lieu de l'avance fixe `pos*size*2/3` ; double-clic = mot,
    triple-clic = tout, drag-sélection (≙ `event_clicks`/`FL_DRAG`
    amont) ; `set_scale_factor` câblé (init + `ScaleFactorChanged`
    ≙ `Fl::screen_dpi`).
  - **clipboard OS** : `arboard` (texte, `default-features=false`)
    dans `dillo-platform` + fallback in-process ; copier/coller
    câblés dans la barre d'adresse et la findbar.

- **phase 9 — bugs réels trouvés en test live (fenêtre Windows,
  navigation HTTP(S) réelle)** : `dillo-browser` — un clic sous la
  barre d'onglets (`y >= ctab_h`) retournait `Some(false)` de
  `strip_hit` et court-circuitait `press` avant le hit-test chrome :
  tout le chrome sous le strip (toolbar, Location, status) et le
  contenu étaient inertes (verbatim `UI::handle` — `Tabs->handle`
  qui retourne 0 poursuit le dispatch). `chrome_widget` gère
  désormais `WId::Location` : `take_focus` + caret sous le clic
  (≙ `Fl_Input::handle_mouse`), paste middle-click. `dillo-net` —
  **stall HTTPS** : `TlsConn::try_write` n'appelait `write_tls`
  qu'une fois ; rustls n'émet qu'un chunk par appel → le ciphertext
  de la requête restait bufferisé (`wants_write=true`) et jamais
  re-flushé, le serveur attendait la fin du record indéfiniment.
  `flush_conn` boucle désormais jusqu'à `!wants_write` (handshake,
  write, close_notify, drain post-`process_new_packets` ≙ écritures
  internes `mbedtls_ssl_read`), et `write_task` draine le résidu sur
  readiness `WRITABLE` (`drain_tls_pending`). `dillo` — les logs
  `tracing` vont sur stderr (verbatim `MSG`/`_MSG` amont), pas
  stdout.

- **phase 9 — bugs réels trouvés par le golden harness** :
  `dillo-platform` : fontes — quand la requête `fontdb` par famille
  échoue (familles génériques non taguées sous Windows), les
  métriques proviennent désormais de la face effectivement choisie
  par le fallback shaping cosmic-text (probe `glyph.font_id`) —
  restaure `space_width` ≠ 0 (espaces inter-mots réelles, fixe
  `white-space`) ; poids quantifiés verbatim FLTK
  (`weight ≥ 500 → bold`, `fltkfont.cc` — fixe `table-thead-tfoot*`
  où 700 vs 900 doivent donner la même face). `dillo-images::svg` :
  `currentColor` résolu par injection de la `fg_color` (≙
  `p->image->current_color` amont) — diff 12 194 → 1 614 px, reste =
  AA rasterizer. `dillo-dw` : les callbacks idle `Platform` sont
  tagués par `Layout` propriétaire (`add_idle`/`take_pending_idles`
  prennent l'id) — verbatim la file privée par `FltkPlatform`/onglet
  ; sans routage, une plateforme partagée exécutait l'idle d'un
  onglet sur un autre (canvas blanc). `words.rs` : `i16` négatif
  borné à 0 avant cast `usize` (verbatim `textWidth` `len>0`).
  `dillo-browser::ui` : `xpm_rgb` ajouté (couleurs XPM `#RGB`,
  12/16 bits par canal — `ui.cc`).

- **phase 9 — bugs réels trouvés par le fuzzing** : `dillo-html` —
  `DilloHtml::write` clampe `start_ofs` à `buf.len()` (un buffer
  plus court que l'offset cumulatif paniquait en slice).
  `dillo-platform` — `sanitize_shape_text` avant tout
  `Buffer::set_text` : les séparateurs bidi classe B (FS/GS/RS, NEL,
  LS/PS…) que le splitter de lignes cosmic-text ignore créaient des
  paragraphes multi-directions dans une ligne → assert `shape.rs`
  (`fl_width` amont n'a pas cette notion). `dillo-net::decode` —
  `charset_process` ne saute plus un octet de plus après `Malformed`
  (`read` encoding_rs l'inclut déjà) : `drain(..pos)` dépassait la
  longueur du leftover → panic. Régression
  `charset_malformed_tail_no_panic`.

- **phases 7+8 — DPI complet** : `dillo-net::dpi` (verbatim
  `src/IO/dpi.c` — `Dpi_blocking_start_dpid`, auth, `dpi_ccc`
  4 branches), `dillo-dpi::dpip` serveur (`DpIo`/`DpIoTcp`/
  `DpIoSock`, `Dsh` tokens/modes/`check_auth`, framing
  byte-exact interop C↔Rust), `dillo-dpi::dpiutil`
  (`dpi/dpiutil.c`), `dillo-dpi::dpid*` + bins `dillo-dpid`/
  `dillo-dpidc` (démon verbatim : registry `*:` wildcards,
  spawn-on-demand, socket dir, boucle `select`→tokio),
  plugins : `dpi-hello`, `dpi-datauri` (base64 custom verbatim,
  borne `p+5<256`), `dpi-vsource` (view-source numéroté),
  `dpi-file` (listings+MIME+sniffing, 1 thread/client ≙
  `select`), `dpi-ftp` (wget pipé, retry `/`), `dpi-cookies`
  (store complet : `cookiesrc`, `Max-Age`, dates 2-4 digits,
  `cookies.txt` byte-compatible), `dpi-bookmarks` (`BmStore`
  format `:sN:` verbatim + HTML généré), `dpi-downloads`
  (`dlitem` verbatim + fenêtre native winit/tiny-skia
  remplaçant FLTK — géométrie verbatim 420×85/70 px par item).

- chrome-4 (phase 6, fin du shell) — **le binaire `dillo` est réel** :
  `dillo.rs` main verbatim `dillo.cc` (table `Options`/`getCmdOption`,
  `makeStartUrl` F_OK→`file:/`, ordre verbatim des `a_*_init` :
  paths→prefs(dillorc)→keysrc→domainrc→capi→dicache→cookies→hsts→
  auth→control→StyleEngine UA→`gen_about_keys`→réglages dw→overrides
  CLI→factory moteurs tokio→fenêtre initiale→boucle→`*_freeall`),
  `dilloc.rs` verbatim `dilloc.c` (socket `~/.dillo/ctl/<pid>`,
  forward stdin, code retour 1er octet), `dillo-net::control` verbatim
  `control.c` (machine d'état, seams `CtlIo`/`CtlOps`, `wait` différé),
  `dillo-browser::doc` (dispatch MIME→viewers ≙ `a_Web_dispatch_by_type`
  + `Html_callback`/`Plain_callback`, `DocImageFactory` ≙
  `a_Image_new_with_dw`, file `GlueOp` ≙ appels UI synchrones amont
  reportés au tick), `dillo-browser::glue` (`ShellGlue` complet :
  événements→chrome verbatim `UI::handle`/`CustTabs`, pompe réseau
  `capi_poll`+`poll_timeouts`+`Control::poll` par tick,
  `wake_after`≙`Fl::wait(0.01)`, `CtlOps`, `WinOp` fenêtres natives).
  `DwApp` multi-fenêtres (`DwWin` par fenêtre ≙ `Fl_Double_Window`,
  tombstones ≙ ids stables, `new_empty`≙main amont, `set_shell`,
  `ControlFlow::WaitUntil` ≙ `Fl::wait(timeout)`). `BwRegistry`
  mutabilité intérieure + tombstones ≙ `BrowserWindow*` stables amont.
  Déviations documentées : `-x` xembed rejeté (X11), signaux
  SIGUSR1/2 non câblés (reload via `dilloc`), `-geometry` verbatim.

- `dillo-browser` (phase 6, lot chrome-1) : glue navigateur —
  `bw.rs` (`src/bw.c` verbatim : `BrowserWindow` à mutabilité
  intérieure ≙ pointeur nu amont, `BwRegistry` ≙ `bws`/`num_bws`,
  clients root/image, compteurs progress, `page_urls` dédup,
  `stop_clients`/`cleanup`/`expect`/`doc` courant), `nav.rs`
  (`src/nav.c` : pile verbatim `stack_ptr=-1`, `stack_move_ptr`
  bornes muettes, `open_url`/`expect_done` (ops pile si
  `expecting` uniquement — quirk amont), flags `MustLoad`,
  reload POST→confirmation sauf `dpi:/vsource/`,
  redirection0/meta-refresh différés via `NavSched` ≙
  `a_Timeout_add(0)`, `Nav_save_cb`, seams `NavIo` =
  `NavBw`+`NavUi` fusionnés en un seul objet ≙ globaux amont —
  deux `&mut` sur le même registre seraient du UB), `keys.rs`
  (`src/keys.cc`+`keysrc` : 33 raccourcis par défaut, parser
  `<Ctrl|Shift|Alt|Meta>`+nom/char/`0x`hexa, `nop`→remove,
  tri binaire verbatim, decode UTF-8 avec quirk amont
  char-shifté→mods droppés, `gen_about_keys` → `about:keys`
  injecté en cache), `uicmd.rs` (`src/uicmd.cc` : dispatch
  `a_UIcmd_*` complet — cycle de vie fenêtre/onglets, `open_url*`
  + préfixes de recherche case-insensitifs + quirk
  `search_url_idx: bool`, `file:` suffixe vide→ancien cwd,
  zoom borné [0.10,10.0], save flow + `save_filename`
  ` `/`%20`→`_`, dialogues choice 1-based, popups
  link/image/page/history, view-source `dpi:/vsource/`+POST
  id cyclique, progress/status/title `Dillo: `+UTF-8
  `end_of_char` 80, `set_buttons_sens`, `findtext`/`findbar`,
  `by_name`, `CustTabs` wheel/shortcut/close). Seams
  `UiShell`/`NavSeams` injectées ≙ globaux FLTK amont
  (déviation ADR-0002 : pas de widgets natifs). `dillo-dw` :
  `Layout::copy_selection` (≙ `a_UIcmd_copy_urlstr`) +
  `ComplexButtonOps::size_allocate`. 37 tests `dillo-browser`.
- `dillo-browser` (phase 6, lot chrome-2) : `ui.rs` — chrome
  `src/ui.cc` verbatim en widgets logiques auto-dessinés
  (`Ui`/`Group` ≙ `CustGroupHorizontal`/`CustGroupVertical` :
  `rearrange` resizable+drop verbatim ; `Tabs` ≙ `CustTabs` +
  `Fl_Wizard`/`Fl_Pack`/`Fl_Scroll` : `ctab_h`, `focus_num`
  wrap+renumérotation, scroll offset, fermeture → focus au
  `focus_num` max, label ≤15 octets UTF-8+`...`, strip caché à
  1 onglet ; `LineInput` ≙ `CustInput`/`TipWinInput` + filtres
  clavier verbatim ; métriques panel `P_tiny`/`P_small`/`P_medium`
  verbatim, `customize` `prefs.show_*`, `panels_toggle`,
  `PanelTemporary` ; `ui_shortcut` ≙ `UI::handle` FL_SHORTCUT ;
  dessin `tiny-skia`/`cosmic-text` `pm_box`/`pm_text`/`pm_icon`,
  blit vue→`Main`) ; `Chrome` ≙ globaux `tabgroups`+FLTK :
  `UiShell` complet (fenêtres/onglets/labels/focus/location/
  status/progress/bug-meter/sensibilité/`defer_nav`), `NavSched`.
  `dillo-platform` : `pixmaps.rs` — 27 icônes XPM générées
  verbatim depuis `src/pixmaps.h` (`_gen_pixmaps.py`), décodées
  par `xpm_decode` (`*In` = `desaturate`+`color_average`
  verbatim `iconset`) ; `View::set_scroll_prefs` +
  `middle_click_drag`. `prefs.search_url_idx` → `Cell<bool>`
  (mutation verbatim pendant `open_urlstr` sans `&mut` —
  évite le double-emprunt RefCell prefs/partagé avec le shell).
  67 tests `dillo-browser`.
- `dillo-browser` (phase 6, lot chrome-3) : `menu.rs` (`src/menu.cc`
  verbatim — 8 popups : page/link/image/form/file/bugmeter/
  history/tools ; actions dynamiques `a_Actions_link_get`/
  `page_get`, sous-menu stylesheet avec urls CSS en `user_data`
  verbatim, ellipsis labels 30/40, middle-click ≙
  `middle_click_opens_new_tab`/`focus_new_tab` ^ Shift, history
  sentinel `-1`, toggles prefs tools, `MenuCtx` ≙ globaux
  `popup_*`, seams `DocOps` ≙ `a_Html_*` + `UiShell::*_popup`),
  `dialog.rs` (`src/dialog.cc` — `DialogState`+seam
  `DialogRunner` ≙ fenêtres FLTK, `choice` 1-based/cancel→0,
  `user_password`, `search` labels `search_urls`, `LineEditor` ≙
  `Fl_Input` Ctrl-AEKD+cut+clamp UTF-8), `findbar.rs`
  (`src/findbar.cc` — filtre clavier verbatim Pass/Consume,
  Enter/Shift+Enter, métriques 70/2), `tipwin.rs`
  (`src/tipwin.cc` — `TipWin` partagé snprintf-255+8, position
  sous/au-dessus pointeur, délais 0.8s/0.1s, `CustButtonState`
  highlight Enter/Leave/Release/Hide/Deactivate). Câblage
  `ui.rs`/`Chrome` : dispatch verbatim `b1_cb`/`clear_cb`/
  `search_cb`/`help_cb`/`filemenu_cb`/`bugmeter_cb`,
  `Hit::Findbar`, coordonnées popup ≙ `Fl::event_x/y`. Fix :
  `NavUi::set_buttons_sens` reçoit `(ptr,size)` en snapshot
  (verbatim `bw->nav_*` — `b.nav` déjà emprunté par `Nav`),
  `NavOp::CloseBw` + `UiCmd::expect_done` exposés (entrées
  capi/menu, chrome-4). 101 tests `dillo-browser`.
- `dillo-html` (phase 2, lot html-1) : crate complète —
  machine d'état tag-soup verbatim `src/html.cc` (`html.rs` :
  `DilloHtml` + `Html_write_raw` buffer cumulatif ≙ `Client->Buf`,
  ouvertures/fermetures implicites, cross-nesting, stash modes,
  warnings+numéros de ligne verbatim), `tags.rs` (table `Tags[]`
  94 entrées + `B8` flags verbatim + ~90 `Html_tag_open_*`),
  `entities.rs` (table 2125 entrées `html_charrefs.h` + parsing
  `&name;`/numeric verbatim — `;` optionnel HTML≤4.01, `&ampy`
  → littéral), `common.rs` (`HtmlState`/`Core`/`Shared`/`Prefs`,
  `EndTag::{Required,Optional,Forbidden}` ≙ `'O'`/`'R'`/`'F'`),
  `sink.rs` (trait `HtmlSink` ≙ `BrowserWindow*` + `a_UIcmd_*` +
  dialogs), `plain.rs` (`DilloPlain` verbatim — `expand_tabs`,
  anchors `line-%d`, incrémental), `table.rs` (glue `src/table.cc`
  — border/cellspacing par défaut, td/th → `Textblock`/`Simple`),
  `form.rs` (`src/form.cc` complet : `HtmlForm`/`HtmlInput`
  15 types, encodage urlencode/multipart verbatim, `FormReceiver`
  `Weak` ≙ cycle brisé, radio groups, select/optgroup/option,
  textarea/button/file/isindex, `form_send` via sink).
  Quirks verbatim : bug amont `Tags[stack_idx]` dans
  `tag_cleanup_at_close` (reproduit avec borne), attributs
  dupliqués → premier gardé, `IMAGE_MAX_AREA`, `MAX_BORDER` 10.
  Seam `@import` : hook → octets en cache + récursion inline
  (ordre de cascade verbatim ; `None` → fetch async + repush).
  Préparatifs `dillo-dw` : `Arena` publique (`tb_*`/`w_*`/`embed_*`
  sur `Layout`), `get_resource_factory` typé `ResourceFactory`,
  downcast accessors `Resource` (≙ casts `form.cc`),
  `ListStyleType` ordonné, `widget_parent` accesseur,
  `Layout::platform/arena` publics, `rust-version` 1.86
  (trait upcasting `Rc<dyn SubResource>`→`Rc<dyn Resource>`),
  `dlib::strtol`/`strtod`/`dGethomedir`. Fix connexe :
  `content_type_cmp` retournait un bool inversé par rapport à
  l'`int` amont dans `cache.rs` (≠0 = différent). 28 tests
  `dillo-html` (entités, soup, forms, plain).
- `dillo-platform` (phase 6, lot plat-1) : pile native
  `winit`+`softbuffer`+`tiny-skia`+`cosmic-text` remplaçant
  `dw/fltk*.cc` (ADR-0002). `platform.rs` — `SkiaPlatform`
  (≙ `fltkplatform.cc`) : fontes cosmic-text (pool, interning
  familles, métriques `rustybuzz` ≙ `fl_height`/`fl_width`),
  couleurs/tooltips, adaptateur imgbuf `dillo-images`,
  file idle ≙ `Fl::add_idle`, clipboard in-process
  (déviation ≙ `Fl::copy`). `view.rs` — `Canvas`/`SkiaView`/
  `ClippingView`/`FlatView`/`SharedView` (≙ `fltkviewbase.cc`+
  `fltkviewport.cc`+`fltkflatview.cc`) : draws tiny-skia avec
  traduction vue↔canvas verbatim, pile de clips ≙ `fl_push_clip`,
  scroll/damage/blit ≙ `fl_scroll`+`FL_DAMAGE_*`, `Pixmap` en
  `Option` (OOM gracieux). `ui.rs` — `SkiaResourceFactory` +
  8 ressources auto-dessinées (≙ `fltkui.cc`+`fltkcomplexbutton.cc`)
  : `sizeRequest` verbatim (`fl_width("n")*size+label+2*RELIEF_X`,
  RELIEF=3), listes `@N`/`\a`/depth, groupes radio partagés.
  `app.rs` — `DwApp` `ApplicationHandler` winit (≙
  `FltkViewport::handle`+`Fl::run`) : dispatch bouton/motion/
  enter/leave/wheel→`Layout`, `getDwButtonState` verbatim,
  double-clic <500 ms ≙ `event_clicks`, scrollbars dessinées et
  hit-testées ≙ `Fl_Scrollbar`, drain idles dans `about_to_wait`,
  présentation `softbuffer`. `surface.rs` — `PlatformSurface`
  (Pixmap→blit 0RGB→present). Seam : `SharedView` proxie le
  `Canvas` (et non la vue — le layout rappelle `view.*` pendant
  `draw` ; un proxy `Rc<RefCell<SkiaView>>` paniquerait —
  pointeur widget amont reproduit au niveau du canvas).
  21 tests (scroll/clamp, damage, clip, pixels headless,
  hit-test scrollbars, métriques `sizeRequest` verbatim).
- `dillo-css` (phase 3, lot css-1) : crate complète —
  `doctree` (arène `Vec` ≙ pointeurs chaînés amont),
  `css` (types/`CssLength` union, `CssPropertyList`,
  sélecteurs/combinateurs/spécificité, `CssStyleSheet`
  indexée elem/id/classe/universel, `CssContext` cascade
  UA<user<nonCssHints<author<author!important<user!important,
  rejet verbatim des règles auteur pseudo+unsafe),
  `parser` (tokenizer tolérant : commentaires non fermés,
  `<!--`/`-->`, strings échappées, `#hex` contextuel,
  nombres négatifs, `tval` capé 255 ; déclarations +
  14 shorthands + `!important` + `@import`/`@media` +
  récupération verbatim) et `styleengine` (pile doctree,
  héritage `resetValues`, `preprocess`/`postprocessAttrs`,
  deux passes de résolution `em/ex/ch/rem/vw`, bordures
  4-directions, feuille UA `thread_local`, `~/.dillo/style.css`).
  Seams : `TagIndex` (≙ `a_Html_tag_index` fonction libre
  amont), `CssHtmlHooks` (≙ `DilloHtml*` — `@import`/URL),
  `StyleEngineSink` (≙ `BrowserWindow*` — bg images),
  `StyleEnginePrefs` injecté. Quirks verbatim conservés :
  `border-rigth-width` (typo), `elementTable[-1]` UB→guard,
  `z-index:auto`→`zIndex=13`, table de props désordonnée
  (`x-*`/`last`), `margin-*` sans `%`.
- `dillo-dw::table` (phase 5, lot dw-5) : `WidgetKind::Table` +
  `TableData` verbatim `dw/table.cc` — grille `Cell`/`SpanSpace`
  (`colspan 0`→`eff=max(numCols-curCol,1)`+`rowClosed`, `MAXSPAN`
  100, rangées implicites/manquantes, `reallocChildren`), extrêmes
  colonnes en deux passes (`colspan==1` puis apportionnement
  multi-colonnes `apportion2` cumulatif entier — rounding-errors),
  cas de largeur verbatim (min>dispo → MinMin/MaxMin ou branche
  pourcentages `widthPartPer`, `width` forcé > Σmax → maxima /
  colonnes spécifiées exclues via `ExtrMod::Data`, sinon Min/Max),
  `cumHeight`+`apportionRowSpan`, `redrawX/Y`,
  `containerSizeChanged` par colonne modifiée ; `\bug` amont
  conservés (`numColWidthSpecified` reset 2× / jamais
  `numColWidthPercentage`, `baseline` jamais rempli,
  `removeChild` no-op). Cellules ≙ `TextblockData.cell :
  Option<TableCellKind>` (`Simple`/`Aligned` ≙ sous-classes
  amont) : `isBlockLevel`→false, `usesMaxGeneratorWidth`→true,
  `correctAvailWidthOfChild`/`correctCorrectedRequisitionOfChild`
  clamps, `applyPer*` sans boxDiff, `AlignedTableCell`
  (`getValue`/`setMaxValue`, détection `textAlignChar` dans
  `wordWrap`, `updateValue` dispatché). `TableInFlow` ≙
  `TableIterator` (raster order, `SPAN_SPACE`→InvalidIterator).
  `WidgetKind::Table` câblé dans tous les dispatchs (sizing,
  OOF, iterator, detach/delete, draw, hit-test). 18 tests.

- `dillo-dw::stacking` : seams verbatim `dw/stackingcontextmgr`
  — `IMPL_POS=false` amont ⇒ `stackingContextMgr` jamais
  instancié ; prédicats `is_establishing`/`handled_by` +
  héritage `stackingContextWidget` fidèles, corps différé.

- `dillo-dw::textblock` (phase 4, lot dw-3) : port complet du
  sous-système texte dw — modèle `Word`/`Line`/`Paragraph`/
  `BadnessAndPenalty` verbatim, construction incrémentale
  (`addText*`/`addSpace`/`addParbreak` avec propagation
  ancêtre/`addLinebreak`/`addWidget`/`addAnchor`), wrapping
  + breaking `textblock_linebreaking.cc` (penalties TeX,
  `DIV_CHARS`, justification cumulative anti-arrondi,
  hyphenation via `hyphenator` Trie+`.pat`/`.exc`/`.trie`),
  sizing/extremes/allocation + OOF, draw complet
  (`drawText`/`drawWord*`/`drawSpace`/`drawLine`/`drawLevel`),
  itérateur `TbInFlow` verbatim (highlight/unhighlight,
  `getAllocation` lignes non allouées), `sendSelectionEvent`
  avec hit-test mot/caractère réel. `WidgetKind::Textblock`
  boxed, `with_tb` extraction `mem::replace` (modèle D).

- `dillo-dw::selection` + `findtext` : `SelectionState`
  verbatim `selection.cc` (press/motion/release, lien→drag,
  `END_OF_WORD`, highlight incrémental, copie clipboard) et
  `FindtextState` verbatim `findtext.cc` (KMP, case-sens/
  insens + équivalence whitespace, forward/backward,
  wrap-around RESTART). `DeepIterator::new` clone l'itérateur
  entrant (verbatim ctor amont).

- `dillo-dw::image` (phase 4, lot dw-4a) : `WidgetKind::Image`
  + `ImageData` verbatim `dw/image.cc` — états `altText`/`buffer`/
  `ImageMapsList` (imagemaps client/serveur, hit-test shapes),
  `ImageImgRenderer` ≙ `Image:ImgRenderer` (`Weak<Arena>`+
  `WidgetId`, `setBuffer` scale/alt-transition, draws/resizes
  différés tagués `Layout::id` — files `pending_*` désormais
  taguées par layout car l'arène est partageable), sizing/
  extremes/allocation/draw-par-lignes/alt-text verbatim,
  events `x_img`/`x_link`.

- `dillo-dw::ui` (phase 4, lot dw-4b) : `WidgetKind::Embed` +
  trait `Resource` verbatim `dw/ui.cc` — `&self` + mutabilité
  intérieure (ré-entrance `w_*_of_child`), `Activate`/`Clicked`
  receivers/emitters, sous-traits verbatim (`LabelButton`,
  `Selection`, `List`/`SelectionMode`, `OptionMenu`, `Text`,
  `Entry` (`UNLIMITED_SIZE=-1`), `MultiLineText`, `Toggle`,
  `Check`, `Radio`+`RadioGroupIterator`) et `ResourceFactory`
  (impl phase platform ≙ `fltkui.cc`) ; `ComplexButtonResource`
  complet : layout interne `Layout::with_arena` (arène partagée
  ≙ pointeurs cross-layout amont), relief `2*reliefX/Y` sur
  sizeRequest/getExtremes, corrections `*OfChild` verbatim
  (FIXME typo `getAvailHeight` amont conservé, `NoRec`≙base
  extraite), `EmbedResizeRelay` ≙ `LayoutReceiver::resizeQueued`
  → `pending_resizes` tagué. `Layout::id` unique ajouté pour
  taguer les files différées en arène partagée. 13 tests.

- `dillo-dw::textblock::listitem` + `bullet` + `ruler` :
  `AlignedTextblock`/`ListItem` (liste partagée Rc,
  `updateValue`→`setMaxValue`, `leftInnerPadding`/
  `line1Offset`), `Bullet` (sizeRequest xHeight·4/5, draw
  Disc/Circle/Square), `Ruler` (usesAvailWidth, draw→
  `drawWidgetBox`, `getWidgetAtPoint`=inAllocation) ; arms
  dispatch `WidgetKind` pour les trois kinds.

- `dillo-dw::hyphenator` : `hyphenator.cc` verbatim — Trie,
  loaders `.pat`/`.exc`/`.trie`, priorités chiffres odd/even,
  `hyphenateWord` consommé par le wrapping.

- `dillo-dw::iterator` (phase 4, lot dw-2b) : `DwIterator`
  trait object ≙ `Iterator` (widget `WidgetId`, mask,
  `next`/`prev`/`highlight`/`unhighlight`/`get_allocation`),
  `EmptyIterator`/`TextIterator` verbatim + `ChildIterator`
  seam enfants (`WidgetKind::Text` feuille ≙ usage
  `TextIterator` anticipé amont), `iterator_scroll_to`
  verbatim (clamp viewport sur saut de ligne/colonne),
  `DeepIterator` pile niveau-indexée (`searchDownward`/
  `searchSideward`/`respective_parent` OOF_REF→generator
  verbatim), `CharIterator` (BREAK→`'\n'`, `space`→`' '`) ;
  `Layout::w_iterator` dispatch `WidgetKind`. 8 tests
  (traversée cross-widget, space flag, compareTo, vide).

- `dillo-dw` style+interfaces image (phase 4, lot dw-1a) :
  `imgbuf` trait (`dw/imgbuf.hh` — verbatim, `Rc<dyn Imgbuf>`),
  `imgrenderer` (`ImgRenderer` + `ImgRendererDist`, snapshot Rc),
  `style` complet modulo draw_* : tous les enums, `Length`
  packing verbatim + `roundInt` ≙ `(d>0?d+0.5:d-0.5)`, `Box`,
  `StyleAttrs` (`initValues`/`resetValues`/`sizeDiffs`≙false,
  `equals` ptr-identité `Rc::ptr_eq`, `hashValue` somme
  `(intptr_t)`→adresses), `Style` + `styleTable` thread_local
  (déduplication ≙ pool statique), `Font`/`FontAttrs`,
  `Tooltip` (+trait `TooltipHooks` ≙ virtuals), `StyleImage`
  (partiel), `numtostr` verbatim (romain/alpha/`"****."`,
  `num=0`→`"."` quirk préservé).

- `dillo-dw` fondations (phase 4, lot dw-0) : `types`
  (`dw/types.cc/.hh` — `Rectangle`/`Circle`/`Polygon`/`Region`
  avec quirks verbatim : borne `y>=maxy` asymétrique, fusion
  forcée ≤16 rect, ray-casting sans garde sommet ; `Allocation`/
  `Requisition`/`Extremes`, `Content` union→enum, stacking
  contexts `HashSet<WidgetId>`), `events` (`dw/events.hh`),
  `tools` (`SizeParams`, refs `Widget*`→`WidgetId`),
  `style::Color`/`Shading`/`shadeColor` verbatim (+`view` seam :
  3 primitives `draw_*` pour `Shape::draw` — interface
  `dw/view.hh` complétée avec le lot layout).


- `dillo-images` décodeurs : `gif` (LZW maison amont porté verbatim
  — spill lines, `orig_code==last_code`, entrelacé 4 passes,
  `state=999`), `png` (crate `png` — sig check, `png_composite` +
  LUT gamma `png_set_gamma(2.2,·)` reproduits, gris→RGB manuel),
  `jpeg` (`jpeg-decoder` — decode-at-EOI, CMYK_INV ≙ Adobe),
  `webp` (`image-webp` — RIFF-complete check, composite
  `(c*a+bg*(255-a))/0xFF` verbatim), `svg` (`resvg`/`usvg` — attente
  `"</svg>"`, `dpi` ≙ `nsvgParse`, premultiplied→straight→composite
  amont). Décodeurs câblés dans la table dicache (≙ table statique
  amont) + `register_image_viewers` ≙ les `Mime_add_minor_type` de
  `a_Mime_init`. Déviations « decode-at-completion » (pas
  d'affichage progressif des lignes, multi-scan/interlace
  raffiné non reproduit) documentées par module pour
  `audit-09-images`.
- `dillo-net::mime` : port de `src/IO/mime.c` — registre
  `MimeMinItems`/`MimeMajItems`, `a_Mime_get_viewer` verbatim
  (quirk : clé plus courte matche via `MinSize`), `MimeViewer`/
  `ViewerOutcome` injectés (viewers texte → `dillo-html` plus tard).
- Repository scaffold: mission contract (`AGENTS.md`), feasibility
  study (`docs/feasibility.md`), phase plan (`ROADMAP.md`), full
  upstream→Rust file mapping (`docs/PORTING.md`).
- ADRs 0001–0005: port scope & fidelity, GUI stack
  (winit+softbuffer+tiny-skia), I/O model (tokio+rustls), DPI
  subsystem, widget-tree ownership (open, prototype-gated).
- 12 parity-audit skills in `.devin/skills/` (report/apply two-mode
  protocol, `docs/audits/` outputs).
- Cargo workspace : 11 crates squelettes (`dillo-common` → `dillo`,
  + `dillo-dw-proto` pour l'ADR-0005), `#![forbid(unsafe_code)]`,
  lints workspace (unsafe forbidden, clippy pedantic warn), CI
  GitHub Actions (fmt/clippy/test/doc + MSRV 1.85 sur ubuntu+windows).
- `dillo-common::dlib` : port de `dlib/dlib.c` — `Dstr`, `Dlist<T>`,
  helpers `dStr*`/`dIs*` ASCII, `parse_rc_line`, `getline` (14 tests).
- `dillo-common::lout` : port de `lout/` —
  `misc` (`SimpleVector`, `NotSoSimpleVector` fidèle avec extra-
  arrays, `StringBuffer`, `BitSet` avec sa sur-allocation amont,
  `round_int`), `unicode` (décodeur UTF-8 lenient + table alpha
  verbatim), `object` (traits + enum `Value`), `signal` (`Emitter<R>`
  sur `Weak<RefCell<R>>`, `emit_bool` sans court-circuit),
  `identity` (trait `Identifiable`). `container`/`debug`/`msg` →
  won't port (collections std / `tracing`). 15 tests ajoutés.
- `dillo-dw-proto` : prototype ADR-0005 (3 options × scénario
  `queue_resize` en pleine itération) → **option D retenue** :
  arène `Layout` + `WidgetId` + `enum WidgetKind` + `LayoutCtx`.
  B (`Rc`/`Weak`) a paniqué `BorrowMutError` au premier essai puis
  exigé try_borrow+defer partout ; A (`dyn`) paie take/replace +
  downcast sans gain. ADR-0005 amendé `accepté` + règles de port
  `dw/` (liens en ids, take/copy avant récursion, events drainés).
- `dillo-platform::golden` : harness golden-render headless —
  `Pixmap` offscreen tiny-skia, diff pixel-à-pixel (compte, bbox,
  delta canal), PNG load/save, `assert_golden` avec régénération
  via `DILLO_UPDATE_GOLDEN=1`. Deps : `tiny-skia` 0.11 (ADR-0002),
  `png` 0.17 (pur Rust — stockage des images de référence).
- **Phase 0.5 — tranche verticale** : `dillo` bin ouvre une fenêtre
  winit, rend blocs+liens via tiny-skia/cosmic-text, navigue par
  clic (`file://`). Modules : `dillo-html::minidoc` (parseur
  tolérant h1-6/p/a/entités), `dillo-net::fetch` (`file://` +
  `resolve_href` URL-semantics), `dillo-platform::surface`
  (winit+softbuffer+pixmap blit). `ControlFlow::Wait` sans spin.
  Deps : `winit` 0.30, `softbuffer` 0.4, `cosmic-text` 0.12
  (ADR-0002). Pilote — remplacé par dw phase 4.
- `dillo-common::utf8` : port de `src/utf8.cc`/`utf8.hh` —
  `end_of_char`, `decode` lenient (fallback octet brut type
  ISO-8859-1), `encode` aux formes historiques 1–6 octets de
  `fl_utf8encode`, `test` (catégories 0–4), `ideographic`,
  `combining_char`, `char_count`, constantes U+FFFD/U+200B (8 tests).
- `dillo-common::misc` : port de `src/misc.c`/`misc.h` —
  `escape_chars`, `expand_tabs` (taquets 8 + queue de diacritiques
  combinants), sniffing `get_content_type_from_data`,
  `parse_content_type`/`cmp`/`check`, `parse_geometry`,
  `parse_search_url`, `encode_base64`, `parse_content_disposition`
  (filename sanitizé), `file_to_vec`. Quirks amont reproduits et
  documentés : `content_type_check` `-1` inatteignable ; le sniffing
  classe `text/plain` tout buffer UTF-8-valide (même octets de
  contrôle) (12 tests).
- `dillo-common::colors` : port de `src/colors.c` — table
  `EXTENDED_COLOR` verbatim (147 entrées triées), `color_parse`
  (`#`/`0x`/nom/hex nu + statut err 0/1/2), `color_vc` avec scores
  `distance2`/`distance3` verbatim (4 tests).
- `dillo-net::url` : port de `src/url.c`/`url.h`/`src/IO/Url.h` —
  `DilloUrl` (champs owned au lieu de vues NUL), parse lenient,
  `resolve_relative` RFC-3986 avec les mêmes hacks amont (dot-
  segments, hack `/` pré-authority, truncate `query_sep`), encodage
  `%XX` des chars illégaux, heuristique `http:`/`//` par défaut,
  `hostname`/`port` lazy (ipv6 `[]`, défauts 80/443), `url_cmp`
  (ordre amont), `decode`/`encode_hex_str`, `strip_delimiters`,
  `host_type`, `host_public_suffix` + liste TLD, `set_ismap_coords`.
  Switch HSTS isolé : `switch_to_https` reproduit la transform
  (`:80`→`:443`, port, scheme) — le déclencheur `prefs`/`hsts` se
  branchera avec leurs ports. 18 tests dont la batterie RFC-3986
  §5.4 complète (normal + abnormal).
- `dillo-browser::prefs` : port de `src/prefs.c` + `prefsparser.cc` —
  `Prefs` owned (pas de global), `Default` = `a_Prefs_init` verbatim
  (~110 champs, UA `Dillo/3.3.0-rc1`), `parse_dillorc` ligne à ligne
  via `dlib::parse_rc_line`, `parse_option` = symbol table → match
  avec les 12 types (BOOL/COLOR/STRING/STRINGS/URL/INT32/DOUBLE/
  FRACTION_100/GEOMETRY/PANEL_SIZE), sémantique « 1re occurrence
  vide le défaut » pour les listes, `strtod`/`strtol` lenients.
- `dillo-net::decode` : port de `src/decode.c` — `DecodeTransfer`
  (chunked : `strtol` hex sur buffer non-borné → whitespace C/`\n`,
  signe, préfixe `0x`, troncature `long`→`int` ; fin sur longueur 0
  ou garbage), `Decode` (gzip : en-tête RFC1952 + FHCRC + trailer
  CRC32/ISIZE vérifiés à la main — `miniz_oxide` ne gère pas le
  wbits=31 ; deflate : fallback raw sur `Z_DATA_ERROR` avec rejeu
  de l'input ; brotli ; charset `encoding_rs` avec skip 1 octet
  par `EILSEQ`). Deps : `flate2` (rust_backend), `brotli` 8.x,
  `encoding_rs`, `crc32fast`.
- `dillo-net::chain` : port de `src/chain.c`/`chain.h` — CCC
  (`Concomitant Control Chain`) verbatim : arène `LinkId` + free-list
  (ADR-0005), `new_link`/`link_new`/`unlink`/`free`, callbacks FWD/BCK
  + `bfcb` (flag end/abort effacé entre les deux sens comme amont),
  flags `CCC_Stopped`/`Ended`/`Aborted`, `LocalKey`→`Box<dyn Any>`,
  `void* Data1/Data2`→`ChainData` (`None`/`Buf`/`Int`/`Str`/`Handle`/
  `Owned` partagé par `Rc`), `check` + log d'avertissement verbatim.
  Callbacks `Rc<dyn Fn>` clonés avant appel → réentrance/unlink-safe.
- `dillo-net::dns` : port de `src/dns.c` — cache d'adresses, file FIFO
  avec déduplication insensible à la casse, `NUM_DNS_SERVERS` = 4
  canaux de résolution, serve/channel `-1`/`-2` verbatim ; threads
  amont → `tokio::task::spawn_blocking`, notification → mpsc +
  `Dns::poll` (callbacks non-`Send`, tout reste sur le thread
  principal). 5 tests (cache, dédup, FIFO, échec, sync).
- `dillo-net::io` : port de `src/IO/IO.c` + `iowatch.cc`/`IO.h` —
  `a_IO_ccc` verbatim (branches 1B/1F/2B/2F : write par `OpSend`
  buffer/`"FD"`, read par events), `IoEngine` = `ValidIOs` +
  `ValidSocks` (table fd entiers ≙ numéros amont). `Fl::add_fd` →
  tâches tokio (`ready().await` + `try_read`/`try_write`), callbacks
  `IO_fd_*_cb` → `IoEvent` mpsc dispatchés par `poll_io` (garde
  `IO_get` reproduite contre la réentrance — un `OpSend` peut avoir
  aborté la chaîne). Fidélité fd partagé : `"FD"` → `clone_socket`
  (≙ `dup` via `socket2`), `IO_StopRd`/`IO_StopWr` retirent la watch
  sans fermer, seul `IO_StopRdWr` fait `dClose` (`shutdown(Both)`).
  Déviation corrigée vs BUG amont : le double-close de la race Abort
  disparaît par ownership. Dep : `socket2` 0.6.5. Test de régression
  `shared_fd_survives_writer_close` sur vraie socket loopback.
- `dillo-net::mime` : port de `src/IO/mime.c` — `MimeRegistry`
  (listes minor/major ordonnées), `get_viewer` verbatim : scan unique
  MinSize/MajSize avec le quirk du `'/'` initial (`MajSize` = 2e
  slash), `dStrnAsciiCasecmp` préfixe dans les deux sens ; `Viewer_t`
  → enum `ViewerId` en attendant les vrais modules (4 tests dont
  quirks). `src/IO/proto.c` = won't port (stub vide amont).
- `dillo-net::about` : `ABOUT_SPLASH` verbatim (VERSION 3.3.0-rc1).
- `dillo-net::tls` : port de `src/IO/tls.c`/`tls.h` + backend
  `tls_mbedtls.c` — `TlsEngine` = `Tls_servers` par (host-ci, port)
  avec state machine `connect_ready` (NONE/RECEIVING/CLEAN/BAD/
  USER_ACCEPTED → `CONNECT_NEVER`/`NOT_YET`/`READY`), `DilloVerifier`
  = `MBEDTLS_SSL_VERIFY_OPTIONAL` (webpki enregistre `CertWarning`s
  au lieu d'échouer — la décision reste post-handshake via
  `after_handshake` = `Tls_check_certificate` + `a_Dialog_choice`
  injecté), `TlsConn` partagé par `Arc`+`Mutex` ≙ `io->FD`/`conn`
  partagé lecteur+écrivain (handshake conduit en tâche tokio ≙
  `Tls_handshake_cb` sous iowatch ; `Mutex` jamais tenu à travers
  `.await`), `IoStream::Tls` dispatché dans `try_read`/`try_write`/
  `ready`/`try_clone`/`shutdown_both` (`send_close_notify` ≙
  `mbedtls_ssl_close_notify`), `read_task` réordonné drain-puis-wait
  = ordre `read()` amont + couvre le plaintext bufferisé sans
  readiness. CA : `rustls-native-certs` ≙ `Tls_load_certificates` +
  `add_ca_file` PEM/DER ≙ `Tls_load_certificates_from_file`. Deps :
  `rustls` 0.23.46 (provider `aws-lc-rs` — tension « 100 % natif »
  documentée ADR-0003, `rustls-rustcrypto` alpha-only), `rustls-
  native-certs` 0.8. `tls_openssl.c` → won't port (backend
  alternatif). 4 tests (b64, state machine, cert clean, dialog).
- `dillo-net::http` : port de `src/IO/http.c` — transport pur (la
  lecture/parse de réponse reste chez `cache.c`, pas porté).
  `HttpEngine` = `ValidSocks` (zombies `TO_BE_FREED` en file ≙
  `dFree` différé) + `servers` par (host-ci, port, https) + `fd_map`.
  `Http_socket_enqueue` verbatim (non-images devant le 1er
  `WEB_Image` valide — la socket entrante testée sans `valid`,
  zombies de file visibles comme amont). `connect_queued_sockets` :
  balayage zombies, `TLS_CONNECT_*` par socket TLS, `active_conns`
  ≤ `http_max_conns`, `running_the_queue` anti-réentrance, retrait
  du serveur vide. Proxy : env `http_proxy` > prefs, `no_proxy`
  suffix-match ascii-ci avec quirk `dStrsep` (token vide = match
  tout), `CONNECT host[:443]` + `Proxy-Authorization` base64,
  réponse accumulée jusqu'à `\r\n\r\n` avec statut à l'index 9.
  `make_query_str` verbatim : URI absolue proxy ( '#' tronqué),
  Accept par type (`WEB_*`), referer `host`/`path`/masqué, E2E →
  `Pragma`+`Cache-Control: no-cache`, boundary multipart extrait
  de `data[2..'\r')`, POST `Content-Length` = `data.len`.
  `reply_complete` → keep-alive reuse (fd transféré au socket en
  file, `active_conns` déplacé, `fd_map` retiré puis re-rempli,
  `connect_done(TRUE)`) ; `reuse_compatible` : TLS+proxy exige même
  host:port. `a_Http_ccc` 4 branches verbatim + `"FD"` ; connect
  non-bloquant + retry `addr_list` → `connect_task` tokio ≙
  `Http_connect_socket_cb` + `SO_ERROR` ; upgrade TLS = swap du
  stream `IoStream` dans `IoEngine` + `begin_handshake` ≙
  `a_Tls_connection` + `Tls_handshake_cb` ; `Tls_check_certificate`
  (USER_ACCEPTED court-circuite) puis `connect_done`. `DilloWeb` →
  `HttpWeb` (url/requester/flags/valid/MSG_BW) ; `cookies`/`auth`/
  `prefs` injectés (deps descendantes — défauts build `!HAVE_COOKIES`
  amont). Infra associée : `IoMsg::Swap` + `take`/`swap`/`remove`/
  `close_notify` dans `IoEngine`, `close_by_fd → Option<connecting>`
  (emprunts modules séquentiels), `DnsCallback` chain-aware +
  `dns_resolve`/`poll_dns`. 11 tests : goldens GET/POST/proxy/
  CONNECT, quirks `no_proxy`, priorité de file, zombie+sweep,
  reuse_compatible, wire loopback end-to-end (requête octet-pour-
  octet), limite `max_conns` en vrai.
- `dillo-net::{domain,hsts,history}` : pré-requis de `cache.c`/
  `capi.c`. `domain.c` : `Domain` (règles `origine→destination` +
  `default_deny`) — parsing `domainrc` verbatim (strtok, texte
  surnuméraire = warn mais règle ajoutée), `Domain_match` (`*`,
  exact ci, `.suffixe` frontière `diff==-1` sans le point),
  `permit` : short-circuit, `data:`/hôtes vides, match qui inverse
  `default_deny`. `hsts.c` : liste triée ci de politiques,
  `hsts_set` (attr/value/eat_value verbatim, `max-age`
  `strtol`+ERANGE→INT_MAX, `=0` retire, `preload` ignoré, garde
  `cert_clean` injectée ≙ `a_Tls_certificate_is_clean`), marche des
  domaines parents `strchr(host+1,'.')` + `subdomains`,
  `future_time` saturé ≙ hack `tm_sec=INT_MAX`, `preload` fichier
  `domain true|false` `ONE_YEAR`, `enabled` injecté ≙ pref.
  `history.c` : `History` thread_local ≙ globales, `add_url` dédup
  `url_cmp`+fragment → idx, titres avec fallback `URL_STR`,
  `set_title_by_url` scan inverse. 18 tests. Registres
  `thread_local` = globales amont (single-writer main).
- `dillo-net::{auth,digest,dpip,cookies}` : lot B des pré-requis
  `cache.c`/`capi.c`. `auth.c`/`auth.h` : parse de challenges
  `WWW-Authenticate` verbatim (cb-0 n'interrompt que le token en
  cours, `stale`/`algorithm` non quotés, `realm` ci), arbre
  `AuthHost`→`AuthRealm` (chemins `URL_PATH` complets,
  `path_is_inside` = préfixe + frontière `/`, longest-path),
  `stale` → refresh nonce sans dialog, `AuthDialog` injecté ≙
  `a_Dialog_user_password`, Digest avant Basic, `dStrshred` du mot
  de passe. `digest.c` : MD5 via `md-5`, cnonce `fastrand` ≙
  `rand()%16`, A1/A2 verbatim, ordre de champs IE + `nc=%08x`,
  `opaque` NULL-vs-"" distingué (`Option`). `dpip/dpip.c` (helpers
  protocole) : `build_cmd` (`%s`→`'val'` + stuffing `'`→`''`, EOT
  ` '>'`) et `get_attr_l` (machine SEEK_NAME→FOUND + destuff). 
  `cookies.c` : client `cookies.dpi` — parseur `cookiesrc`
  (`DEFAULT` + longest-domain-first, `.dom` suffixe sans apex),
  DENY→drop, cross-org non-racine→`""`, canal `DpiSend` injecté ≙
  `a_Dpi_send_blocking_cmd`, disabled sans contenu. Flags `URL_*`
  ajoutés à `url.rs` (`URL_Post` pour digest). 18 nouveaux tests.
- `dillo-net::{timeout,web,cache,capi}` : lot C — glue centrale
  navigateur. `timeout.cc` : file thread-local one-shot ≙
  `Fl::add_timeout`, `poll_timeouts` drainé main-thread (réentrant),
  `a_Timeout_remove` no-op amont porté, `actually_remove` par id.
  `web.cc` : `dispatch_by_type` — root→seam `WebDispatch`, non-root→
  `image/*` uniquement (filtre mime, pas `WEB_Image` — quirk amont) ;
  `ValidWebs` ≙ `HttpWeb::valid` partagé `Rc`/`Cell`. `cache.c` :
  entrées triées `url_cmp`, clients à clés, `process_dbuf` verbatim
  (header jusqu'à `\n\n`, strip `\r`/NUL, unfold garde l'espace de
  continuation — quirk, `100 Continue` reset), `parse_header` complet
  (30x→REDIRECT/FORCE/TEMP + blocage domainrc/dpi, 401→challenges,
  Set-Cookie→`cookies_set`, HSTS, Content-Length→GOT_LENGTH, HTTP/1.0
  retire KEEP_ALIVE, Content-Type hdr/meta/norm/det, Content-
  Disposition), chaîne transfer→content→charset→`UTF8Data` lazy
  refcountée, `process_queue` verbatim (dispatch, `AbortEntry`+
  `OfferDownload`, savelink différé, redirect→`nav.push`+
  `redirect_level`, page « blocked redirection » `text/html`, auth via
  timeout 0), delayed queue ≙ `timeout_add(0)`, `about:cache`/
  `about:dicache` `CA_InternalUrl` jamais retirés. Seams
  `BrowserOps`/`UiOps`/`NavOps`/`DicacheOps`/`WebDispatch` injectées.
  `capi.c` : `capi_conn_t`→`Rc<RefCell>` ≙ `Ref`+`CapiConns`, abort
  différé `pending_abort` si CCC en vol (`RefCell` impose le report ≙
  synchrone amont), CCC 4 branches verbatim (`"FD"`→sock_fd+relais,
  `"DpidOK"`→`conn_resume` des `PENDING`, `"DpidERROR"`/`"Both"`,
  `"send_page_2eof"`→`process_dbuf`+`"reply_complete"`,
  `send_status_message`/`chat`/`dialog`/`reload_request`→seams,
  `IO_CLOSE`/`IO_ABORT`→`process_dbuf`) ; `open_url` dispatch
  (download→fichier/dpi, `url_uses_dpi` verbatim `?`/`bm`→`bookmarks`/
  `proto.*`, `dpi_verify_request` GET|POST depuis page dpi seulement,
  `request_permitted` mixed-content https sauf `data:` + domainrc
  sauf cache-hit, http→2 branches recv-avant-send ≙ course DNS) ;
  seams `DpiOps`/`dpi_ccc` injectées (phase DPI). 24 nouveaux tests
  (137 total `dillo-net`).
- `dillo-common::{bitvec,klist}` + `dillo-browser::{nav,actions,
  bookmark,dpiapi}` : lot D. `bitvec.c` : quirk `BVEC_SIZE=1` (1
  octet/bit, bit 0 seul). `klist.c` : clés `i32` compteur, wrap
  complet à 0 seulement (clés négatives émises entre-temps),
  `Clean`. `nav.c` : `Nav` = champs `bw->nav_*`/`meta_refresh_*`
  amont ; pile verbatim, `expect_done` (ops pile uniquement si
  `expecting` — back/forw n'empilent pas), repush/reload/
  redirection0 différés `NavSched`, scroll saved/restored,
  `Nav_save_cb` via `HttpWeb::stream`/`saved_bytes` (Cell+RefCell,
  ouvert par capi WEB_Download) ; seams `NavBw`/`NavUi`/`NavSched`.
  `actions.c` : `label:cmd` strtok-verbatim, `Command` détaché +
  `DILLO_PID`. `bookmark.c` : machine chat `bookmarks` (H/I/O).
  `dpiapi.c` : dialog dpip → `DialogChoice` → `cmd=answer`. 18
  nouveaux tests.
- `dillo-images::{imgbuf,image,dicache}` : lot E (noyau images).
  `imgbuf.cc` : buffer RGB `Rc`-partagé, cascade de clamps
  `excessiveImageDimensions`/`MAX_WIDTH`/`MAX_HEIGHT`/`gamma`
  verbatim, `rgb_line` (INDEXED/GRAY/CMYK_INV `*white/0x100`/RGB).
  `image.cc` : `DilloImage` + seam `ImgRenderer`≙`I2IR`, machine
  `IMG_*`, multi-scan `BitVec`/`ScanNumber`. `dicache.c` : registre
  thread-local trié, versions `DIF_Last`/`DIF_Valid`, `surv_cleanup`,
  `decoded_size` incrémental, trait `ImageDecoder`≙`Decoder`/
  `DecoderData` fusionnés (take/appel/restore anti-réentrance
  `RefCell`), `dicache_image` + `ImageFactory`≙`a_Image_new_with_dw`,
  `about:dicache` verbatim, impl `cache::DicacheOps`. Glue :
  `HttpWeb::image` (`Any`≙`web->Image`), `cache_close_client`,
  `NavBw::bg_color`≙`a_Web_new(bw->bgColor)`. 16 nouveaux tests.
### dw-2a — widget.cc + layout.cc (socle arbre/negotiation/events)
- `[dw]::widget` port de `dw/widget.cc`+`.hh` sous modèle
  ADR-0005 : `WidgetNode` arène + `WidgetKind` dispatch,
  flags verbatim (`RESIZE_QUEUED`…`WAS_ALLOCATED`), hiérarchie
  parent/quasi/container/generator, `intersects`, machine
  `queueResize`/`mark*`/`containerSizeChanged`, `sizeRequest`/
  `getExtremes` avec cache `SizeParams` (fallback intrinsic=-1
  verbatim), `sizeAllocate`, géométrie box (margin/border/
  padding areas), famille `getAvail*`/`calc*`/`correct*`
  (ratio aspect 3 passes verbatim), `setStyle`/`getBgColor`/
  `getFgColor` remontée ancêtres, `drawBox`/`drawWidgetBox`/
  `drawInterruption`, events + `WidgetImgRenderer` (draws
  différés via `pending_draws` — modèle D).
- `[dw]::layout` port de `dw/layout.cc`+`.hh` verbatim :
  attach/detachView (projection état), resizeIdle (limites
  100/1000, QUEUED→NEEDS, canvasHeightGreater), scroll
  (scrollTo0/scrollIdle/adjustScrollPos/calcScrollInto), draw
  (bg image + toplevel, defer après scroll), anchors,
  `LayoutImgRenderer` (`layout*` → état sync + `Weak` arène),
  events complets (moveToWidget piste verbatim, clamp zone
  visible, fallback `emitLink*`), `drain_idles` seam ≙ toolkit.
- `[dw]::platform` module déclaré + `take_pending_idles` seam ;
  `[dw]::events` `EventCrossing`→`Option<WidgetId>` + `ButtonEventType`.
- Seam documentées : `numSizeRequestReferences`/refs tables,
  `stackingContextMgr`/OOF (`IMPL_POS` verbatim `false`),
  `container_add_child` (≙ `addWidget` des kinds amont),
  findtext/selection stubs fidèles (`NOT_FOUND`, reset/copy).
- 10 nouveaux tests (hiérarchie, propagation resize, scroll
  clamp, anchors, link fallback, intersects, fond hérité,
  cache sizeRequest, hit-test conteneur).
### dw-1c — platform/view/style draw complet
- `[dw]::platform` trait `dw::core::Platform` verbatim (attachement
  layout/view, mesures texte, navigation glyphes, DPI, idle,
  factories font/color/tooltip/imgbuf, copie sélection,
  `resourceFactory`).
- `[dw]::view` trait `dw::core::View` complet (viewport/scroll/
  queue_draw, toutes primitives draw_*, clipping views) —
  `Layout*`→handle `usize` (modèle D).
- `[dw]::style` fin dw-1c : `StyleImage` + `StyleImgRenderer` +
  `ImgRendererDist` (buffer tuilé `MIN/OPT_BG_IMG` verbatim),
  `ExternalImgRenderer`/`ExternalWidgetImgRenderer` traits + helpers,
  `drawBorder{Top,Bottom,Left,Right}` trapèzes pixel-verbatim
  (7 styles + double), `drawBackground`/`drawBackgroundImage`/
  `calcBackgroundRelatedValues` (tuilage, positions %/abs,
  `drawBackgroundLineByLine=false` porté en const).
- `[dw]::imgbuf` : receveurs `&self` (mutabilité intérieure ≙
  méthodes appelées via `Imgbuf*` partagés amont) ; `copy_to` prend
  `&dyn Imgbuf`. 8 nouveaux tests (tuilage, bordures, fond).
### dw-3a — couche OOF (floats)
- `[dw]::oof` (~3 200 lignes) : `outofflowmgr.hh` (trait `OofMgr`
  complet), `oofawarewidget.cc/.hh` verbatim (`OofAwareData`,
  recherche de containers, `notifySetParent`/`notifySetAsTopLevel`,
  `initOutOfFlowMgrs`, `addWidgetOOF`/`addWidgetInFlow`,
  `createOOFWidgetRef`, `markSizeChange(Extremes)`,
  `correctRequisition/ExtremesByOOF`, `sizeAllocateImpl`,
  `drawOOF`/`getWidgetOOFAtPoint`, `moveExternalIndices`),
  `oofawarewidget_iterator.cc` (`OofAwareIterator` : sections
  IN_FLOW→FLOATS→ABS→REL→FIXED, `parent_ref` encodé
  subRef/oofmIndex bit à bit, `compare` cross-section verbatim),
  `ooffloatsmgr.cc/.hh` (floats triés left/right bsearch GB,
  `tellPosition`+clamp, `collides` vertical-puis-horizontal,
  `getBorder`/`getGB`/`getClearPosition`, `updateGenerators`,
  `mark*`/`sizeAllocate*`/`getFloatsAtPoint` verbatim),
  `regardingborder.*` (tag → `is_regarding_border_kind`).
- `WidgetKind::OofAware` + dispatchs `w_*` dans `widget.rs` ;
  `detach_widget`/`widget_delete` ≙ `~OOFAwareWidget` (enfants
  OOF détachés, tombstones d'arène).
- `IMPL_POS=false` verbatim : mgrs positionnés = `None`
  (inatteignables amont — `getOOFMIndex`≡FLOATS, consommateurs
  NULL-check → identique observablement, documenté A08-D1).
- Seams `SC_BOTTOM`/`SC_TOP` posées pour `stackingcontextmgr`.
- 19 tests oof (codecs parent_ref, recherche containers,
  collisions/clear/tellPosition, sections itérateur,
  détachement). Audit `docs/audits/reports/A08_dw-oof.md`.
### dw-1b — imgbuf dw/imgbridge aligné
- `[dw]::imgbuf` trait complet (`set_cmap`/`copy_row`/`new_scan`/
  `get_scaled_buf`/`get_row_area`/`root_w·h`/`similar_buf`/`copy_to`/
  `ref·unref`/`last_reference`/`set_delete_on_unref`/`width·height`).
- `[img]::imgbuf` impl le trait dw : init **222** amont, arbre de
  scaled-buffers ≙`fltkimgbuf` (`copiedRows`, formules map root↔scaled
  verbatim, `newScan`/`copyRow` propagés aux fils), `getRowArea`,
  `copyTo` clip+blit, `setCMap` vide ≙`FltkImgbuf` RGB-only.
- `[img]::image` : seam renderer passée à `Rc<dyn Imgbuf>` (≙
  `I2IR::setBuffer`), `Bitvec` dérive `Debug`/`Clone`.
- **Déviation** : `scaleMode` amont `BEAUTIFUL_GAMMA` → `SIMPLE`
  (pixels non gamma-corrigés) — `BEAUTIFUL` en phase platform.


# PORTING.md — Mapping C/C++ → Rust

Ground truth : `D:\Projet\dillo` (read-only). Statuts :
`not started` → `in progress` → `ported` (compile + tests unitaires)
→ `verified` (parité comportementale/golden render) — ou
`won't port` + raison.

Légende cible : `[common]`=dillo-common · `[net]`=dillo-net ·
`[css]`=dillo-css · `[html]`=dillo-html · `[dw]`=dillo-dw ·
`[img]`=dillo-images · `[plat]`=dillo-platform ·
`[browser]`=dillo-browser · `[dpi]`=dillo-dpi · `[bin]`=dillo (facade/bin)

## dlib/ — utilitaires C

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dlib/dlib.c` + `dlib.h` | `[common]::dlib` | ported | `Dstr`/`Dlist<T>`/`dStr*`/`dIs*`/`parse_rc_line`/`getline` ; `dMalloc`/`dClose`/`dUsleep`/`dGetcwd` non portés (std) |

## lout/ — utilitaires C++

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `lout/container.cc` + `.hh` | collections std | won't port | containers non-typés → `Vec`/`HashMap`/`VecDeque` |
| `lout/debug.hh` + `debug_rtfl.hh` | `tracing`/`debug_assert` | won't port | macros debug → `tracing`, `_MSG` → `debug!` |
| `lout/identity.cc` + `.hh` | `[common]::lout::identity` | ported | `Identifiable` trait (`class_name`/`ancestors`/`instance_of`) |
| `lout/misc.cc` + `.hh` | `[common]::lout::misc` | ported | `SimpleVector`/`NotSoSimpleVector`/`StringBuffer`/`BitSet`/`round_int` ; `ZoneAllocator` non porté (propriété Rust) ; sur-allocation BitSet reproduite |
| `lout/msg.h` | `tracing` | won't port | macros MSG → `tracing::{debug,info,error}` |
| `lout/object.cc` + `.hh` | `[common]::lout::object` | ported | `Object`/`Comparable` traits + enum `Value` (remplace wrappers Pointer/Integer/…/Pair des argv de signaux) |
| `lout/signal.cc` + `.hh` | `[common]::lout::signal` | ported | `Emitter<R>` + `Weak<RefCell<R>>` ; `emit_bool` sans court-circuit (note amont préservée) ; `ObservedObject`/`DeletionEmitter` → `Drop` |
| `lout/unicode.cc` + `.hh` | `[common]::lout::unicode` | ported | décodeur UTF-8 lenient (octet brut en fallback), table `is_alpha` verbatim, `next_utf8_char`/`num_utf8_chars` (+variantes `len` fidèles) |

## src/IO/ — couche I/O

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `src/IO/IO.c` + `IO.h` | `[net]::io` | not started | cœur fd + callbacks → tokio |
| `src/IO/Url.h` | `[net]::url` (types) | ported | voir `src/url.c` — un seul type `DilloUrl` |
| `src/IO/about.c` | `[net]::about` | not started | schéma about:* |
| `src/IO/control.c` + `.h` | `[net]::control` | not started | contrôle shell → UI |
| `src/IO/dpi.c` | `[net]::dpi` | not started | client dpi:// |
| `src/IO/http.c` | `[net]::http` | not started | client HTTP/1.1 maison — wire exact |
| `src/IO/iowatch.cc` + `.hh` | `[net]::watch` | not started | → tokio reactor glue |
| `src/IO/mime.c` + `.h` | `[net]::mime` | not started | MIME type dispatch |
| `src/IO/proto.c` | `[net]::proto` | not started | dispatch schémas locaux |
| `src/IO/tls.c` + `tls.h` | `[net]::tls` | not started | trait Tls → rustls (ADR-0003) |
| `src/IO/tls_mbedtls.c` + `.h` | — | won't port | remplacé par rustls |
| `src/IO/tls_openssl.c` + `.h` | — | won't port | remplacé par rustls |

## src/ — parsing & engines

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `src/html.cc` + `html.hh` | `[html]::parser` | not started | machine d'état tag-soup tolérante |
| `src/html_common.hh` | `[html]::common` | not started | |
| `src/html_charrefs.h` | `[html]::entities` | not started | table entités → codegen/static |
| `src/doctree.hh` | `[html]::doctree` | not started | |
| `src/plain.cc` | `[html]::plain` | not started | doc text/plain |
| `src/table.cc` + `table.hh` | `[html]::table` | not started | glue HTML→dw table |
| `src/form.cc` + `form.hh` | `[html]::form` | not started | modèle forms + submit |
| `src/css.cc` + `css.hh` | `[css]::css` | not started | decl sets, matching, specificity |
| `src/cssparser.cc` + `cssparser.hh` | `[css]::parser` | not started | tokenizer CSS tolérant |
| `src/styleengine.cc` + `styleengine.hh` | `[css]::styleengine` | not started | cascade → dw::core::style |
| `src/utf8.cc` + `utf8.hh` | `[common]::utf8` | ported | end_of_char/decode/encode(1-6 octets)/test/ideographic/combining/char_count ; constantes U+FFFD & U+200B |

## src/ — réseau & services document

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `src/url.c` + `url.h` | `[net]::url` | ported | parse lenient + résolution relative (batterie RFC-3986), hostname/port lazy, encodage chars illégaux, heuristique `http:`, public-suffix/TLD, `cmp` ; switch HSTS isolé dans `switch_to_https` (branchement prefs/hsts reporté à leur port) |
| `src/dns.c` + `dns.h` | `[net]::dns` | not started | thread → tokio lookup |
| `src/decode.c` + `decode.h` | `[net]::decode` | ported | chunked (strtol hex non-borné, troncature `long`→`int`, quirks whitespace/`0x`), gzip (en-tête RFC1952 + CRC/ISIZE vérifiés à la main sur `flate2` rust_backend), deflate→raw fallback sur `Z_DATA_ERROR`, brotli (crate `brotli`), charset via `encoding_rs` (labels WHATWG — couvre iconv courant ; skip 1 octet/EILSEQ, pas WHATWG-maximal) |
| `src/chain.c` + `chain.h` | `[net]::chain` | not started | chaîne callbacks → typed pipeline |
| `src/cache.c` + `cache.h` | `[browser]::cache` | not started | cache doc + client queue |
| `src/capi.c` + `capi.h` | `[browser]::capi` | not started | cache API côté client |
| `src/dicache.c` + `dicache.h` | `[img]::dicache` | not started | cache images décodage incrémental |
| `src/web.cc` + `web.hh` | `[browser]::web` | not started | DilloWeb request wrapper |
| `src/nav.c` + `nav.h` | `[browser]::nav` | not started | navigation stack |
| `src/history.c` + `history.h` | `[browser]::history` | not started | |
| `src/auth.c` + `auth.h` | `[browser]::auth` | not started | basic/digest dialogs glue |
| `src/digest.c` + `digest.h` | `[browser]::digest` | not started | digest auth (hash→md-5 crate) |
| `src/md5.c` + `md5.h` | — | won't port | crate `md-5` |
| `src/cookies.c` + `cookies.h` | `[browser]::cookies` | not started | fichier cookies byte-exact |
| `src/domain.c` + `domain.h` | `[browser]::domain` | not started | règles domaines cookies |
| `src/hsts.c` + `hsts.h` | `[browser]::hsts` | not started | + `hsts_preload` data |
| `src/bookmark.c` + `bookmark.h` | `[browser]::bookmark` | not started | bm file format |
| `src/bw.c` + `bw.h` | `[browser]::bw` | not started | BrowserWindow glue |
| `src/actions.c` + `actions.h` | `[browser]::actions` | not started | bookmarks/navigation actions |
| `src/bitvec.c` + `bitvec.h` | `[common]::bitvec` | not started | → bitvec/std |
| `src/klist.c` + `klist.h` | `[common]::klist` | not started | |
| `src/misc.c` + `misc.h` | `[common]::misc` | ported | escape/tabs/sniff Content-Type/parse type+cmp+check/geometry/search_url/base64/disposition/file2dstr ; quirks amont documentés (check -1 inatteignable, sniff UTF-8 écrase binaire ASCII) |
| `src/colors.c` + `colors.h` | `[common]::colors` | ported | table 147 couleurs verbatim (EXTENDED), parse `#`/`0x`/nom/hex nu + err 0/1/2, `color_vc` scores verbatim |
| `src/prefs.c` + `prefs.h` | `[browser]::prefs` | ported | `Prefs` (pas de global — owned par le navigateur), défauts `a_Prefs_init` verbatim, UA `Dillo/3.3.0-rc1` |
| `src/prefsparser.cc` + `.hh` | `[browser]::prefsparser` | ported | fusionné dans `prefs::parse_dillorc`/`parse_option` — symbol table → match, STRINGS vident le défaut à la 1re occurrence, types BOOL/COLOR/STRING/STRINGS/URL/INT32/DOUBLE/FRACTION_100/GEOMETRY/PANEL_SIZE |
| `src/paths.cc` + `paths.hh` | `[browser]::paths` | not started | chemins ~/.dillo → dirs |
| `src/dpiapi.c` + `dpiapi.h` | `[browser]::dpiapi` | not started | API navigateur→plugins |
| `src/timeout.cc` + `timeout.hh` | `[browser]::timeout` | not started | Fl::add_timeout → tokio/winit |
| `src/version.cc` + `version.hh` | `[bin]::version` | not started | |

## src/ — UI shell (chrome)

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `src/ui.cc` + `ui.hh` | `[browser]::ui` | not started | chrome/toolbars/tabs (ADR-0002) |
| `src/uicmd.cc` + `uicmd.hh` | `[browser]::uicmd` | not started | commandes UI |
| `src/keys.cc` + `keys.hh` + `keysrc` | `[browser]::keys` | not started | key bindings + keysrc file |
| `src/menu.cc` + `menu.hh` | `[browser]::menu` | not started | menus contextuels |
| `src/dialog.cc` + `dialog.hh` | `[browser]::dialog` | not started | dialogues (auth, prefs) |
| `src/findbar.cc` + `findbar.hh` | `[browser]::findbar` | not started | |
| `src/tipwin.cc` + `tipwin.hh` | `[browser]::tipwin` | not started | tooltips |
| `src/xembed.cc` + `xembed.hh` | — | won't port | embedding X11 spécifique |
| `src/dillo.cc` | `[bin]` main | not started | entry point |
| `src/dilloc.c` | `[bin]::dilloc` | not started | utilitaire message |
| `src/binaryconst.h`, `dgif.h`, `djpeg.h`, `dpng.h`, `dwebp.h`, `dsvg.h`, `pixmaps.h`, `msg.h`, `list.h`, `domainrc`, `keysrc`, `dillorc` | données/config | not started | headers/data → intégrés aux modules |

## src/ — images

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `src/image.cc` + `image.hh` | `[img]::image` | not started | DilloImage state machine |
| `src/imgbuf.cc` + `imgbuf.hh` | `[img]::imgbuf` | not started | buffer scanlines + callbacks |
| `src/gif.c` | `[img]::gif` | not started | glue → crate `gif` |
| `src/png.c` | `[img]::png` | not started | glue → crate `png` |
| `src/jpeg.c` | `[img]::jpeg` | not started | glue → `jpeg-decoder` |
| `src/webp.c` | `[img]::webp` | not started | glue → `image-webp` |
| `src/svg.c` | `[img]::svg` | not started | glue → `resvg`/`usvg` |
| `src/nanosvg.h`, `nanosvgrast.h` | — | won't port | remplacé par resvg/usvg (pur Rust) |

## dw/ — moteur de layout (cœur)

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dw/core.hh` | `[dw]::core` | not started | namespaces/traits centraux |
| `dw/types.cc` + `.hh` | `[dw]::types` | not started | Rect, Extremes, Allocation… |
| `dw/style.cc` + `.hh` | `[dw]::style` | not started | Style attrs + Font/Color attrs |
| `dw/events.hh` | `[dw]::events` | not started | event types → enums |
| `dw/platform.hh` | `[dw]::platform` (trait) | not started | trait Platform — impl `[plat]` |
| `dw/view.hh` | `[dw]::view` (trait) | not started | |
| `dw/widget.cc` + `.hh` | `[dw]::widget` | not started | base widget tree |
| `dw/layout.cc` + `.hh` | `[dw]::layout` | not started | negotiate/alloc/draw, anchors |
| `dw/iterator.cc` + `.hh` | `[dw]::iterator` | not started | |
| `dw/ui.cc` + `.hh` | `[dw]::ui` | not started | ResourceFactory/Embed widgets |
| `dw/textblock.cc` + `.hh` | `[dw]::textblock` | not started | word wrap, paragraphs |
| `dw/textblock_linebreaking.cc` | `[dw]::textblock` | not started | penalty-based breaking |
| `dw/textblock_iterator.cc` | `[dw]::textblock` | not started | |
| `dw/alignedtextblock.cc` + `.hh` | `[dw]::alignedtextblock` | not started | |
| `dw/hyphenator.cc` + `.hh` | `[dw]::hyphenator` | not started | .pat files → data |
| `dw/ruler.cc` + `.hh` | `[dw]::ruler` | not started | |
| `dw/bullet.cc` + `.hh` | `[dw]::bullet` | not started | |
| `dw/listitem.cc` + `.hh` | `[dw]::listitem` | not started | |
| `dw/findtext.cc` + `.hh` | `[dw]::findtext` | not started | |
| `dw/selection.cc` + `.hh` | `[dw]::selection` | not started | |
| `dw/image.cc` + `image.hh` | `[dw]::image` | not started | widget image |
| `dw/imgrenderer.cc` + `.hh` | `[dw]::imgrenderer` | not started | scaling/filtrage |
| `dw/imgbuf.hh` | `[dw]`/`[img]::imgbuf` iface | not started | interface buffer |
| `dw/table.cc` + `table.hh` | `[dw]::table` | not started | algo table maison |
| `dw/tablecell.cc` + `tablecell.hh` | `[dw]::table` | not started | |
| `dw/simpletablecell.cc` + `.hh` | `[dw]::table` | not started | |
| `dw/alignedtablecell.cc` + `.hh` | `[dw]::table` | not started | |
| `dw/table_iterator.cc` | `[dw]::table` | not started | |
| `dw/outofflowmgr.cc` + `.hh` | `[dw]::oof` | not started | mgr OOF générique |
| `dw/oofawarewidget.cc` + `.hh` + `oofawarewidget_iterator.cc` | `[dw]::oof` | not started | |
| `dw/ooffloatsmgr.cc` + `.hh` | `[dw]::oof::floats` | not started | floats |
| `dw/oofpositionedmgr.cc` + `.hh` | `[dw]::oof::pos` | not started | |
| `dw/oofposabsmgr.cc` + `.hh` | `[dw]::oof::abs` | not started | |
| `dw/oofposabslikemgr.cc` + `.hh` | `[dw]::oof::abslike` | not started | |
| `dw/oofposrelmgr.cc` + `.hh` | `[dw]::oof::rel` | not started | |
| `dw/oofposfixedmgr.cc` + `.hh` | `[dw]::oof::fixed` | not started | |
| `dw/stackingcontextmgr.cc` + `.hh` | `[dw]::stacking` | not started | z-order |
| `dw/regardingborder.cc` + `.hh` | `[dw]::border` | not started | |
| `dw/tools.cc` + `.hh` | `[dw]::tools` | not started | misc draw helpers |

## dw/ — couche FLTK (remplacée, ADR-0002)

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dw/fltkplatform.cc` + `.hh` + `fltkcore.hh` | `[plat]::platform` | not started | spec du trait — impl winit/skia |
| `dw/fltkviewbase.cc` + `.hh` | `[plat]::view` | not started | |
| `dw/fltkviewport.cc` + `.hh` | `[plat]::viewport` | not started | scroll/damage |
| `dw/fltkflatview.cc` + `.hh` | `[plat]::flatview` | not started | |
| `dw/fltkui.cc` + `.hh` | `[plat]::ui` | not started | ResourceFactory FLTK |
| `dw/fltkcomplexbutton.cc` + `.hh` | `[plat]::button` | not started | |
| `dw/fltkimgbuf.cc` + `.hh` | `[plat]::imgbuf` | not started | pixbuf adapter |
| `dw/fltkmisc.cc` + `.hh` | `[plat]::misc` | not started | |
| `dw/fltkpreview.cc` + `.hh` | `[plat]::preview` | not started | print preview — peut être différé |
| `dw/preview.xbm` | — | won't port | asset FLTK |

## dpi/ — plugins

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dpi/dpiutil.c` + `.h` | `[dpi]::util` | not started | escaping/filtering |
| `dpi/bookmarks.c` | `bin dpi-bookmarks` | not started | |
| `dpi/cookies.c` | `bin dpi-cookies` | not started | |
| `dpi/datauri.c` | `bin dpi-datauri` | not started | |
| `dpi/downloads.cc` | `bin dpi-downloads` | not started | |
| `dpi/file.c` | `bin dpi-file` | not started | |
| `dpi/ftp.c` | `bin dpi-ftp` | not started | |
| `dpi/hello.c` | `bin dpi-hello` | not started | plugin test |
| `dpi/vsource.c` | `bin dpi-vsource` | not started | view-source |

## dpid/ — démon

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dpid/main.c` | `bin dillo-dpid` | not started | |
| `dpid/dpid.c` + `dpid.h` | `[dpi]::daemon` | not started | registry/services |
| `dpid/dpid_common.c` + `.h` | `[dpi]::common` | not started | |
| `dpid/dpi.c` + `dpi.h` | `[dpi]::daemon::service` | not started | |
| `dpid/dpi_socket_dir.c` + `.h` | `[dpi]::sockdir` | not started | socket dir / AF_UNIX |
| `dpid/dpidc.c` | `bin dpidc` | not started | client cmdline |
| `dpid/misc_new.c` + `.h` | `[dpi]::misc` | not started | |
| `dpid/dpidrc.in` | config template | not started | |

## dpip/ — protocole

| Fichier amont | Cible Rust | Statut | Notes |
|---|---|---|---|
| `dpip/dpip.c` + `dpip.h` | `[dpi]::proto` | not started | framing byte-exact |

## Non-porté (build/meta)

`Makefile.am`, `configure.ac`, `autogen.sh`, `Doxyfile`,
`dillo.desktop`, `icons/`, `dillo-install-hyphenation`,
`install-dpi-local`, `test/Makefile.am` — `won't port` (build
amont remplacé par Cargo).

## Synthèse

| Statut | Fichiers |
|---|---|
| not started | ~150 lignes de mapping |
| ported | 0 |
| verified | 0 |
| won't port | tls_mbedtls, tls_openssl, md5.c, nanosvg*.h, xembed.cc, preview.xbm, build/meta |

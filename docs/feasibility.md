# Plan de faisabilité — Port de Dillo en Rust natif

**Date** : 2026-10-16 · **Source** : `../dillo` · **Cible** :
`../dillorust`

## Verdict

**Faisable**, avec un profil de risque très différent d'un port réseau
type libtorrent : ici, ~80 % de la difficulté est concentrée dans le
moteur de layout `dw/` et le découplage de FLTK — pas dans les
protocoles. Aucun bloqueur technique identifié ; les points
d'attention sont la fidélité pixel du layout et la stack GUI.

## 1. Inventaire

| Composant | Fichiers | LOC | Rôle |
|---|---|---|---|
| `src/` | ~110 | ~49 700 | navigateur : parseur HTML, CSS, cache, cookies, UI FLTK |
| `src/IO/` | 18 | ~6 500 | couche I/O événementielle, HTTP, TLS, dpi client |
| `dw/` | ~80 | ~35 800 | moteur de layout/rendu (widget tree, tables, floats) |
| `lout/` | ~15 | ~4 500 | utilitaires C++ (object, signal, unicode, container) |
| `dlib/` | 2 | ~1 200 | utilitaires C (Dstr, Dlist…) |
| `dpi/` + `dpid/` + `dpip/` | ~20 | ~9 800 | système de plugins + démon + protocole |
| `test/` | — | ~5 300 | tests unitaires, dw, html render |
| **Total** | ~250 fichiers source | **~130 k LOC** | |

Mixte C (style `a_Foo_bar` procedural) et C++03 léger (classes,
`lout::object`, templates quasi absents). Pas de dépendance exotique :
FLTK, OpenSSL/mbedTLS, libpng/jpeg/gif/webp, pthread (DNS).

## 2. Analyse de risque par sous-système

### 2.1 `lout` + `dlib` — **faible**
Utilitaires auto-contenus : `Dstr`→`String`, `Dlist`→`Vec`,
`object::Signal`→callbacks/`mpsc`. Le système de signaux typés de
`lout` se traduit proprement en traits Rust. Semaine de travail.

### 2.2 `src/IO` + réseau — **faible-moyen**
Modèle événementiel single-thread (`iowatch` + callbacks `IO`) →
mapping direct vers tokio. `http.c` est un client HTTP/1.1 écrit
main (requêtes formatées, redirects, auth) : à porter tel quel plutôt
que remplacer par reqwest — le format wire exact fait partie du
comportement. TLS : deux backends (openssl/mbedtls) remplacés par
rustls derrière le trait `Tls`. `decode.c` (chunked/gzip/deflate) via
`miniz_oxide` (pur Rust). DNS : thread helper → `tokio::net::lookup_host`.

### 2.3 Parseur HTML + CSS — **moyen**
`html.cc` (~5k LOC) est un parseur *tolérant pré-HTML5* avec des
règles de récupération spécifiques (fermetures implicites, nesting
tag-soup). **Risque de déviation comportementale** : un parseur
« correct » diverge. Mitigation : porter la machine d'état telle
quelle + fuzz différentiel vs upstream. `cssparser.cc`/`css.cc`/
`styleengine.cc` : CSS 2.1 partiel, plus simple, tests dorés faciles.

### 2.4 `dw/` moteur de layout — **élevé (le cœur du projet)**
36k LOC d'algorithmes de layout incrémental : négociation de taille
`getExtremes`/`setSize`/`draw`, arrondis entiers volontaires
(`devdoc/rounding-errors.doc`), tables (`table.cc` algorithme
maison), OOF/floats/abs/rel/fixed via managers interdépendants,
césure/hyphenation, sélection. **Le risque n'est pas la difficulté
du port mais la parité pixel** : un écart d'arrondi décale des
rendus entiers. Mitigation : (a) harness golden-render headless dès
la phase 0, (b) porter `devdoc/*.doc` comme référence, (c) audits
dédiés (05–08).

**Risque n°1 — modèle de propriété** : `dw/widget.hh` tient
l'arbre par 4 pointeurs croisés bruts (`parent`, `quasiParent`,
`generator`, `container`) sans refcount, traversés dans les deux
sens pendant le layout incrémental ; le parent de contenu ≠ parent
de layout pour les OOF. `Rc<RefCell>` (cycles + `BorrowMutError`
en itération) et `Box<dyn>` naïf (liens latéraux inexprimables)
sont des pièges. Arbitrage arène+`WidgetId` vs `Rc`/`Weak` vs
Layout-propriétaire tranché par **prototype en phase 0**
(ADR-0005) — décision bloquante avant tout port dw.

### 2.5 FLTK → winit + softbuffer + tiny-skia — **élevé**
Dillo n'utilise FLTK que pour : fenêtre/événements (`fltkview*`),
surface de dessin (`fltkplatform` implémente `dw::core::Platform` :
drawLine/Rectangle/Text/Pixbuf, mesure de fontes, clipping),
widgets chrome (`ui.cc`, menus, dialogs, findbar, tipwin).
`dw::core::Platform` est **déjà une interface d'abstraction** — le
port n'a qu'à en fournir une impl Rust. Points chauds :
- **Fontes** : FLTK mesure via la fonte système ; tiny-skia ne fait
  pas de shaping/mesure → `cosmic-text` ou `fontdue`+`ab_glyph` pur
  Rust. La métrique de fonte impacte le layout → zone sensible de
  parité, à calibrer.
- **Scroll/redraw partiel** : FLTK damage rects → softbuffer présente
  un buffer complet ; redessiner par région via tiny-skia clip est
  équivalent.
- **Chrome** (boutons, tabs, menus, dialogs) : pas d'équivalent
  widget toolkit — à reconstruire en natif dans `dillo-platform`/
  `dillo-browser` (travail net mais sans risque de fidélité, puisque
  le chrome n'est pas « spec »).
Alternative rejetée : `fltk-rs` (violerait le « 100% natif ») ;
`egui` (modèle immédiat incompatible avec le widget tree retenu de dw).

### 2.6 Images — **faible-moyen**
`dicache`/`imgbuf` : décodage incrémental *par scanline* avec callback
au widget — garder cette granularité. Décodeurs C
(libpng/jpeg/gif/webp/nanosvg) → crates Rust pures : `png`,
`jpeg-decoder`, `gif`, `image-webp`/`webp`, `resvg`/`usvg`. Tous pur
Rust disponibles. Le glue (`gif.c` etc.) est mince.

### 2.7 Services navigateur — **faible**
`cache.c`/`capi.c` (chaîne de callbacks producteur→consommateur →
channels typés), cookies/hsts/bookmarks (fichiers ligne — format
byte-exact à respecter), `digest.c` (MD5 → crate `md-5`), `nav`/
`history`. Travail de volume, faible risque.

### 2.8 DPI/dpid/dpip — **moyen**
Protocole texte sur socket Unix (`dpip.c` framing byte-exact —
interopérable avec les plugins C restants). Port complet demandé :
`dpid` démon (socket dir + registry), `dpip` lib, plugins file/
datauri/downloads/bookmarks/cookies/vsource/ftp. Sur Windows :
équivalent named-pipe/`AF_UNIX` (Windows 10+ supporte AF_UNIX).
Risque : spawn/supervision de plugins diffère POSIX/Windows —
isoler dans `dillo-dpi`.

## 3. « 100 % Rust natif » — frontière des dépendances

Autorisé : crates pur Rust uniquement (pas de `*-sys` liant du C).
Sélection validée : `winit`, `softbuffer`, `tiny-skia`, `rustls` +
`rustls-native-certs` (ou `webpki-roots`), `miniz_oxide`,
`png`/`jpeg-decoder`/`gif`/`image-webp`, `resvg`/`usvg`,
`cosmic-text` (ou `fontdue`), `unicode-linebreak`, `md-5`,
`thiserror`, `tracing`, `tokio`. **Exception à arbitrer** :
`rustls` utilise `ring` (asm/C) ou `aws-lc-rs` — choisir le provider
`rustls` + `ring` est accepté par la communauté mais contient du C ;
alternative 100 % Rust = `rustls` avec `aws-lc-rs` exclu au profit
de… arbitrage à trancher dans ADR-0003 (ring = asm précompilé,
recommandé toléré, documenté).

## 4. Stratégie de vérification

| Oracle | Usage |
|---|---|
| `test/unit/*` | portés tels quels |
| `test/html/render` (~dizaines de pages) | golden renders headless tiny-skia, diff pixel vs snapshots upstream |
| `test/dw/*` | exemples layout → tests headless |
| Fuzz différentiel | html/css/decode/cookies vs binaire C upstream |
| `devdoc/*.doc` | spécification interne dw — lecture obligatoire |
| `dillorc`, fichiers cookies/hsts/bookmarks | round-trip byte-exact |

## 5. Contraintes / points de vigilance

1. **Parité pixel** : arrondis entiers de dw à reproduire à l'unité
   près ; métrique de fontes = plus grande source de dérive.
2. **Incrementalité** : parsing/décodage/layout par chunks — ne pas
   batcher (comportement observable : rendu progressif).
3. **Tolérance** : Dillo ne crashe jamais sur entrée malformée —
   `forbid(unsafe)` + pas de panique = aligné, à tester en fuzz.
4. **Windows** : upstream POSIX-first ; `dillo-platform` isole
   AF_UNIX/paths dès le jour 1.
5. **Taille** : ~130 k LOC → ordre de grandeur comparable à un port
   de plusieurs mois-homme ; la granularité par phases (ROADMAP)
   permet une montée en valeur incrémentale (navigateur utilisable
   dès la phase 7 sur du HTML simple).

## 6. Conclusion

Port **faisable et pertinent** : l'architecture de Dillo (abstraction
`Platform`, moteur synchrone, I/O événementiel) se prête bien à Rust.
Facteur clé de succès = discipline de parité (PORTING.md + audits +
golden renders), pas la faisabilité technique. Recommandation :
procéder selon `ROADMAP.md`, phases 0–4 en priorité car elles portent
tout le risque de fidélité.

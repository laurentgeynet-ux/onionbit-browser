# Perf — mesures DilloRust

Mesures locales (Windows x86_64, release `lto=thin` + `codegen-units=1`).
Pas de binaire amont sur cette machine — la comparaison chiffrée avec
Dillo C/C++ exige un build amont sous Linux ; les chiffres amont cités
sont des ordres de grandeur publiés (binaire ~1-3 Mo, RSS ~10-30 Mo).

## Binaires (`target/release`, non strippés)

| Binaire | Taille |
|---|---|
| `onionbit-browser.exe` | 9,2 Mo |
| `dpi-downloads.exe` | 2,6 Mo (embarque tiny-skia pour le rendu liste) |
| `dillo-dpid`/`dpidc`, `dilloc`, `dpi-*` (7 autres) | 0,1-0,25 Mo chacun |

Package `dist/onionbit-browser-0.0.0-windows-x86_64.zip` ≈ 14 Mo décompressé,
tous binaires + `dillorc` + `dpidrc` gabarits.

## Démarrage

`onionbit-browser --version` (spawn → exit) : **~60 ms** — dominé par le chargement
du processus, pas par du code métier.

## Mémoire au repos

Fenêtre ouverte, page par défaut, mesuré via `WorkingSet64` après 6 s
(`packaging/measure-rss.ps1`) : **WS ≈ 39,6 Mo / Private ≈ 42,3 Mo**.
Inclut `fontdb` (scan des fontes système) + `cosmic-text` +
`tiny-skia`/`softbuffer` — stacks plus lourds que `Fl::`+Xft amont,
mais dans l'ordre de grandeur visé (« 40× moins qu'un navigateur
mainstream » ADR-0030 suite OnionBit).

## Rendu — corpus golden

`render_bench` (`cargo test --release --test render_ref -- --ignored`) :
pipeline complète `DilloHtml` → `StyleEngine` → `Layout` → `SkiaView`
offscreen, médiane de 10 rendus par fichier.

- Par fixture : **médiane 33-40 ms**, min 32-39 ms.
- Corpus complet : 78 rendus ≈ **2,76 s** de médianes.

**Attention** : chaque `render()` recrée `SkiaPlatform` (init `fontdb`)
— le plancher ~33 ms est le coût fixe du harness, pas le coût marginal
par page dans le navigateur (fontes chargées une fois au démarrage).
L'écart inter-fixtures (~1-8 ms) reflète le vrai coût parse+layout+draw
par page ; les pages du corpus sont volontairement minimales.

## Rejouer

```sh
cargo test --release -p dillo --test render_ref render_bench -- --ignored --nocapture
powershell -File packaging/measure-rss.ps1   # RAM idle (ouvre une fenêtre ~6 s)
```

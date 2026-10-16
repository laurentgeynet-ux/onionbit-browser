# ADR-0004 — Sous-système DPI : port complet, protocole byte-exact

- **Statut** : accepté · **Date** : 2026-10-16

## Contexte

Dillo externalise des services (bookmarks, cookies, downloads,
file:, ftp:, datauri, vsource) dans des processus plugins parlant le
protocole **dpi** (texte/FD via `dpip`), supervisés par le démon
**dpid** (répertoire de sockets Unix + registry `dpi_socket_dir`).
Décision utilisateur : port complet.

## Décision

1. `crates/dillo-dpi` : lib `dpip` (framing `dH_*`/`a_Dpip_*`
   **byte-exact** — des plugins C amont doivent pouvoir s'y brancher
   pendant la transition et réciproquement).
2. `dpid` porté comme binaire `dillo-dpid` : socket dir Unix sous
   Linux/macOS ; sous Windows `AF_UNIX` (supporté depuis Win10) avec
   fallback named-pipe si nécessaire — isolation dans un trait
   `DpiTransport`.
3. Plugins portés en binaires du workspace (`dpi-file`, `dpi-datauri`,
   `dpi-downloads`, `dpi-bookmarks`, `dpi-cookies`, `dpi-vsource`,
   `dpi-hello`, `dpi-ftp`) ; `dpiutil.c` → module partagé.
4. Côté client, `src/IO/dpi.c` → `dillo-net::dpi` : schéma `dpi://`
   et `dpid` spawn-on-demand conservés.
5. Sécurité : upstream fait confiance au socket-dir local ; on porte
   le même modèle de confiance, documenté — le durcissement
   (vérification d'arguments, échappement `dpiutil`) est fidèle au C.

## Conséquences

- Interopérabilité mixte C↔Rust pendant la migration = test de
  parité réel du protocole.
- Le superviseur/spawn de plugins diffère POSIX/Windows : tests
  d'intégration dédiés sur les deux OS.

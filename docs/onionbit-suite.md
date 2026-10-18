# Contexte d'intégration — suite OnionBit (contrat de coutures)

**Statut** : note de contexte, **pas un ordre de travail**. La
référence est l'ADR-0030 du projet OnionBit
(`D:\Projet\Tribler-Rust-Torrent\docs\architecture\decisions\0030-navigateur-onion-dillorust-proxy-tor.md`),
actuellement **Proposée** — ses questions §8 ne sont pas arbitrées.
Ne rien figer au-delà de ce qui suit.

**Règle d'or** : le port reste fidèle à `D:\Projet\dillo` (AGENTS.md
§1). Cette note ne demande **aucun** code de mode « suite » — elle
identifie uniquement les endroits où un choix de portage fermerait
une porte dont l'intégration aura besoin. Quand fidélité et couture
coincident (souvent le cas : l'amont a déjà la bonne abstraction),
la fidélité prime.

## Ce que la suite attendra du navigateur (pour mémoire)

- Dillorust = navigateur `.onion` **bête** : aucun code Tor, aucune
  résolution DNS, aucune connaissance arti. Tout `http(s)` sort via
  `http_proxy` vers un forward proxy loopback servi par le daemon.
- Profil `.onion` : cookies/HSTS/historique/dicache éphémères ;
  aucun fichier persistant d'amont écrit dans ce mode.
- Bookmarks : backend délégué à l'API REST du daemon (DB chiffrée,
  portable) — jamais d'accès fichier direct.
- Plugins dpi qui ouvrent une socket sortante : **bannis** du profil
  `.onion` (ftp, gemini, gopher…). Seuls les plugins locaux
  (file, datauri, vsource, bookmarks, cookies, downloads) sont
  admis dans ce mode.

## Les 4 coutures à ne pas refermer pendant le port

1. **CLI + env proxy** (`dillo.cc`, phase 6 — en cours) : point
   d'entrée `<binaire> <url>` ; `http_proxy`/`no_proxy` honorés par
   l'environnement. Déjà fidèle amont — vérifier simplement que le
   chemin reste accessible et non contourné.

2. **Registre dpi par profil** (phase 8, non commencée) : concevoir
   `dpid`/le chargement des plugins de façon qu'un plugin soit
   activable/désactivable par profil (l'amont l'a déjà via
   `dillorc` — porter cette granularité, pas la moyenner).

3. **Services dpi persistants** (bookmarks, cookies — phase 8) :
   le stockage derrière une interface injectable — l'amont fait de
   ces services des processus séparés parlant `dpip`, ce qui est
   déjà l'abstraction voulue. Un backend « API daemon » deviendra
   *une implémentation*, pas une réécriture.

4. **Répertoire de profil déplaçable** : `cookies.txt`, `hsts`,
   `bm.txt` doivent accepter un répertoire passé en config/CLI
   (`paths.cc` est en cours de port — la couture est là). Un profil
   jetable futur = un répertoire temporaire, rien de plus.

## Ce qui n'est PAS à faire ici

- Mode suite, profil éphémère, auth `Proxy-Authorization`,
  isolation par origine, en-têtes normalisés : travaux post-port,
  jalons 129+ du côté OnionBit (dépendent du spike proxy et des
  arbitrages §8).
- Toute fonctionnalité sans contrepartie amont ne rentre pas dans
  ce dépôt : c'est le côté OnionBit qui porte proxy, politique et
  API.

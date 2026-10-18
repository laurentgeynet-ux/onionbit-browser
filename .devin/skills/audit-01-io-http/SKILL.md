---
name: audit-01-io-http
description: "[V1] Audit sémantique vs Dillo — Chaîne I/O & HTTP (IO.c, chain.c, http.c, decode.c, tls, dns)"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A01 — Chaîne I/O, HTTP & décodage transfert

**Périmètre** : couche `src/IO/` + `src/{chain,decode,dns,http…}` —
framing HTTP/1.1 exact, redirects, encodages de transfert, TLS
rustls, résolution DNS, granularité par chunk de la chaîne de
callbacks.

## Fichiers

- **Upstream code** (lecture seule) : `src/IO/{IO.c,IO.h,iowatch.cc,
  http.c,mime.c,proto.c,about.c,control.c,tls.c,dpi.c}`,
  `src/{chain.c,decode.c,dns.c,url.c}`
- **Upstream tests** : `test/unit/*` relatifs, comportement via
  `dillo <url>` — pas de suite wire : la spec = le code amont + HTTP
  réel.
- Checkout upstream : `$DILLO_ROOT` si définie → `../dillo`
  (poste local).
- **Rust** : `crates/dillo-net/src/{io,http,chain,decode,dns,tls,
  url,mime}.rs`, `crates/dillo-browser/src/cache.rs` (consommateur)

## Checklist spécifique

- [ ] Framing HTTP : requête formatée à l'identique (`Host`, `User-Agent`,
      `Accept`…), HTTP/1.1 keep-alive vs close, status line parsing lenient
- [ ] Redirects : codes gérés, limites de boucle, Location relative, même
      politique referer qu'amont
- [ ] `decode.c` : chunked (tailles hex lenientes, trailers), gzip/deflate
      (zlib vs raw), cascade des encodages — erreurs mid-stream
- [ ] Chunking : chaque `read` upstream pousse un callback — le pipeline
      Rust délivre-t-il la même granularité (rendu progressif) ?
- [ ] DNS : timeout, annulation à la fermeture de requête, IPv6, ordre
      des résultats
- [ ] TLS : négociation rustls vs matrix openssl/mbedtls amont —
      erreurs certificat remontées comme amont (message page d'erreur)
- [ ] `chain.c` : fan-in/fan-out, ordre des Link/Info/Bck, fermeture
      propage l'Abort dans les deux sens
- [ ] Backpressure : queue cache pleine → upstream throttle la socket ;
      Rust ne doit ni bufferiser à l'infini ni dropper des chunks
- [ ] Proxy (`http_connect`), `HTTP_PROXY` env, auth proxy
- [ ] `about:`/`dpi:`/`file:` dispatch `mime.c`/`proto.c` identique

## Protocole d'audit — deux modes

Défaut : **`rapport`** (read-only, parallélisable). **`apply`** =
invocation séparée sur rapport commité + relu.

### Analyse (4 étapes, MANDATOIRE)

1. **Cartographie Rust + inventaire des tests** : machine d'état,
   granularité chunks, canaux, erreurs.
2. **Exploration upstream** : cycle de vie complet (constructeur →
   callbacks → Abort/destructeur), valeurs par défaut, chemins
   d'erreur — ne pas se limiter à ±80 lignes.
3. **Grille à 5 lentilles** pour chaque item :
   - **L1 — Fuzzing & bornes** : réponses/serveurs malformés, chunks
     tronqués, entêtes géants, casts réseau, zéro panique.
   - **L2 — Tolérance & récupération** : upstream dégrade en silence
     (page partielle affichée) vs Rust qui drop — divergences
     d'UX observables.
   - **L3 — Cycle de vie & invariants** : requêtes en vol purgées à
     l'abort, générations/tokens pour résultats tardifs (DNS
     post-cancel), fuites de watchers/timers.
   - **L4 — Événements & backpressure** : `mpsc` saturés
     (`try_send` drop vs `send().await`), starvation timers,
     deadlocks.
   - **L5 — Parité observable** : bytes wire exacts (requête,
     protocole), messages d'erreur utilisateur, progression/chunk.
4. **Test gap analysis** : scénarios amont sans équivalent Rust.

### Mode `rapport`

5. Écrire `docs/audits/reports/A01_io-http.md` — seul fichier écrit.
   Template : `## Verdicts checklist` (un verdict/item :
   `conforme`/`déviation`/`N-A` + preuve `fichier.rs:ligne` vs
   `src/foo.c:ligne` + lentilles + test protecteur — **un
   « conforme » sans preuve est irrecevable**) · `## Déviations`
   (sketch de fix + sévérité `critique`/`wire-visible`/`état-interne`/
   `cosmétique` + test de régression) · `## Invariants de frontière` ·
   `## Fichiers du fix` · `## Non-bugs tranchés`.
6. Commit pathspec-limited :
   `git add docs/audits/reports/A01_io-http.md && git commit -m
   "docs(audit): rapport A01 io-http" -- docs/audits/reports/A01_io-http.md`
7. STOP — pas de fix.

### Mode `apply`

8. `git worktree add ../wt/dillorust-a01 -b audit/01-io-http`
   (worktree sur C:/SSD ; checkout principal = `../dillorust`)
9. Fixes en Rust idiomatique + **tests de régression par déviation**
10. Gates : `cargo fmt --all` · `clippy --workspace --all-targets --
    -D warnings` · `cargo test --workspace` · `cargo doc --no-deps`
11. Changelog fragment `docs/audits/changelog/A01.md` (**pas**
    CHANGELOG.md) + enrichir les rows `docs/PORTING.md` du périmètre
12. Commit `fix(<crate>): <scope> parity — <mot-clé>` sur la branche,
    **local only** (pas de push/PR)
13. `git rebase main` dans le worktree →
    `git -C ../dillorust merge --no-ff audit/01-io-http` →
    `git worktree remove … && git branch -d …`
14. Section `## Résultat` dans le rapport (corrigé/tests/hash),
    commitée avant merge.

## Pièges connus

- Chunked tronqué = page partielle affichée, pas d'erreur amont.
- `dReturn_if_fail` : préconditions debug-only — en Rust,
  `debug_assert!` + comportement dégradé identique en release.
- `IOWrite` en non-bloquant : upstream écrit partiellement et
  re-queue — vérifier qu'aucun octet n'est perdu ni réordonné.
- Gzip concaténé multi-membres (serveurs réels le font) — upstream
  zlib gère ; `miniz_oxide` streaming doit aussi.
- TLS : upstream ignore certaines erreurs de certificat selon prefs
  (`http.c`/`tls.c` messages exacts).
- `unwrap`/`expect`/`todo!` interdits hors tests ;
  `#![forbid(unsafe_code)]`.

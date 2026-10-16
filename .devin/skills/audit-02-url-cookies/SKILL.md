---
name: audit-02-url-cookies
description: "[V1/V7] Audit sémantique vs Dillo — URL, cookies, domaines, auth digest, HSTS"
allowed-tools:
  - read
  - grep
  - glob
  - exec
  - edit
---

# Audit A02 — URL, cookies, domaines, auth & HSTS

**Périmètre** : parsing URL lenient, résolution relative, jar cookies
(fichier partagé avec le C), règles `domainrc`, basic/digest auth,
HSTS + preload — **formats de fichiers byte-exacts**.

## Fichiers

- **Upstream** (lecture seule, `$DILLO_ROOT`→`D:/Projet/dillo`) :
  `src/{url.c,url.h,cookies.c,domain.c,domainrc,auth.c,digest.c,
  hsts.c,hsts_preload}`, `doc/Cookies.txt`
- **Upstream tests** : `test/unit/cookies.c`, `test/unit/disposition.c`
- **Rust** : `crates/dillo-net/src/url.rs`,
  `crates/dillo-browser/src/{cookies,domain,auth,digest,hsts}.rs`

## Checklist spécifique

- [ ] `a_Url_new` : parse lenient (espaces, caractères invalides,
      schéma absent → http), champs vides vs absents
- [ ] Résolution relative : RFC-à-la-Dillo (cas `../`, `//`, `?`
      seul, `#`), pas de crate url stricte — **divergence = bug**
- [ ] Cookies : parsing `Set-Cookie` lenient (expires formats
      multiples, sans `=`, quoted), règles `domainrc` allow/deny,
      third-party policy, session vs persistants, fichier
      `~/.dillo/cookies.txt` **byte-exact en écriture**
- [ ] `domain.c` : matching suffix vs exact, ordre des règles
- [ ] Digest auth : MD5 via `md-5`, `qop=auth`/`auth-int`?, nonce
      count, algorithm=MD5-sess — comparer réponses à un serveur réel
- [ ] HSTS : parse `Strict-Transport-Security` (max-age,
      includeSubDomains), fichier hsts round-trip, preload table
- [ ] Punycode/IDN si amont le gère — même comportement
- [ ] Écriture fichiers : permissions/atomicité comme amont

## Protocole d'audit — deux modes

Défaut **`rapport`**, `apply` sur rapport relu.

### Analyse — 4 étapes + 5 lentilles

1. Cartographie Rust + tests existants.
2. Upstream en profondeur : defaults, chemins d'erreur, formats
   d'écriture exacts (fin de ligne, ordre des champs).
3. Lentilles : **L1** fuzzing/bornes (entêtes/fichiers malformés,
   dates limites, dépassements int) · **L2** tolérance (ligne
   corrompue = skip pas abort du fichier entier) · **L3** cycle de
   vie (flush cookies en sortie, locks implicites) · **L4**
   événements (ordre d'application HSTS vs première requête) ·
   **L5** parité observable (fichiers byte-exact, en-têtes
   Cookie/Authorization identiques, comportement redirect).
4. Test gap analysis.

### Mode `rapport`

5. Rapport `docs/audits/reports/A02_url-cookies.md` — template
   identique aux autres audits (verdicts + preuves + déviations
   sketchées + frontière + fichiers du fix + non-bugs).
6. Commit pathspec-limited sur `main`.
7. STOP.

### Mode `apply`

8. Worktree `C:/Projet/wt/dillorust-a02` branche `audit/02-url-cookies`.
9. Fixes + tests de régression (golden files cookies/hsts,
   vecteurs digest vs amont).
10. Gates fmt/clippy/test/doc → fragment changelog
    `docs/audits/changelog/A02.md` + rows PORTING.md.
11. Commit branche → `git rebase main` → `git -C D:/Projet/dillorust
    merge --no-ff` → cleanup. `## Résultat` dans le rapport.

## Pièges connus

- `cookies.txt` est lu **et** écrit par le C Dillo : fin de ligne,
  champs tab/space, commentaires — byte-exact obligatoire.
- `a_Url_str` : sérialisation normalise différemment selon champs
  vides (`:` vs `://` vs `/@`).
- Digest : upstream accepte des réponses `WWW-Authenticate`
  malformées qu'un parseur strict rejetterait.
- HSTS : `max-age=0` = suppression immédiate ; `preload` n'est
  appliqué qu'aux domaines listés, pas en sous-domaine sauf flag.
- Pas de panique sur dates `expires` > i64 ou entêtes dupliqués.

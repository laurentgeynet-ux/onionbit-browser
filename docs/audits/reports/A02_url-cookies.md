# Audit A02 — URL, cookies, domaines, auth & HSTS

Passe 2 — après merge A01-p2 (`5ebb518`). Mode : `rapport`.
Passe 1 : commit `5aacbcd` (apply) — verdicts + D1–D5 conservés
ci-dessous en section « passe 1 ».

**Nouveau périmètre auditable depuis la passe 1** : jar cookies
`dpi/cookies.c` → `crates/dillo-dpi/src/cookies.rs` + démon
`crates/dillo-dpi/src/bin/cookies.rs`, transport
`a_Dpi_send_blocking_cmd`/`dsh_*`/`a_Dpip_check_auth` →
`crates/dillo-net/src/dpi.rs` + `crates/dillo-dpi/src/dpip.rs`,
glue `cookies_get_query`/`cookies_set` → `dillo-net/src/cookies.rs`.
Les fichiers de la passe 1 (`url/domain/cookies(net)/auth/digest/
hsts/history/dpip`) sont inchangés depuis `5aacbcd` — les fixes
D1–D5 sont revérifiés en place (`decode_hex_octet` tolérant,
`frag_eq` NULL≡"", `fgets_lines` utilisé par `net::cookies`/`hsts`,
création `cookiesrc` 0600 côté `main.rs`, strip ASCII).

## Verdicts checklist (passe 2 — nouveau périmètre)

- **`Set-Cookie` parsing** : `conforme` — `parse_attr`/`parse_value`/
  `eat_value`, `first_attr` nameless→rejet, défaut « +1 an »
  (`gmtime`+`tm_year++`→`mktime`, fallback `future_time`),
  `Max-Age` >1an clamp / `<0`→0 / ignore `Expires` post-Max-Age,
  `Secure`/`HttpOnly`/inconnu → `eat_value`, `session_only =
  !expires` — `cookies.rs(dpi):604-921` vs `dpi/cookies.c:799-1010`.
  L1/L5. Un écart : D12.
- **Dates `Expires`** : `conforme` — `get_timefield` 1-2 digits,
  `get_month` 3 lettres ci, `get_year` 2-4 digits + pivot 70 +
  rejet 5e digit, `date_delim` verbatim, `server_timediff` =
  `now − mktime(server)` ajouté aux secondes — vs `cookies.c:460-651`.
  L1. Tests : `parse_dates` (RFC 850/asctime/1123/rejet).
- **`Cookies_get` matching** : `conforme` — ordre verbatim
  `.host`(domain) → `host`(host_only) → `host`(domain) → suffixes
  `.x`/x en marchant les dots ; tri par path long d'abord stable ;
  purge expirés au passage ; `Cookie: a=b; c=d\r\n` ; compteur
  incrémenté seulement si chaîne non vide — `cookies.rs:414-509` vs
  `cookies.c:1256-1405`. L5. Edge : D13.
- **`Cookies_add_cookie` caps** : `conforme` — remplacement
  (name,path,host_only) avant test d'expiration, rejet expiré,
  `MAX_DOMAIN_COOKIES=20` → `rm_expired(node)` puis LRU, `>=20`
  retirés → re-find (nœud pu être détruit), `MAX_TOTAL=1200` idem,
  re-find du nœud post-purge, suppression du nœud vide en fin —
  `cookies.rs:263-383` vs `cookies.c:656-794`. L3.
- **`cookiesrc` (jar)** : `conforme` — longest-first, `DEFAULT` →
  `default_action` non insérée, règle inconnue → skip, `enabled`
  ssi une action non-DENY (y compris DEFAULT), retour 0/1/2 —
  `cookies.rs:172-229` vs `cookies.c:1419-1537`. L2. Écarts :
  D8/D9 (borne de ligne + whitespace ASCII).
- **`cookies.txt` round-trip** : `conforme` — en-tête verbatim,
  `domain\tF/T?\tpath\tT/F?\tsecs-depuis-epoch-local\tname\tvalue`
  octet-exact, tri domaines, session_only exclus, ligne ≥4096
  ignorée, relecture `F`→host_only / `T`→secure / contrôle
  `cookiesrc` appliqué au chargement — `cookies.rs:514-592` vs
  `cookies.c:248-329,390-455`. L5. Écarts : D8/D9/D10/D11.
- **Transport `send_blocking_cmd`/`check_auth`/`dsh`** : `conforme`
  — start_dpid → connect → write → read, réponse dpip ou `None` ;
  `check_auth` = `cmd=auth` + clé `dpid_comm_keys` (parsing strtol
  + queue hex verbatim) — `dpi.rs:724-735`, `dpi/dpip.rs:479-494`.
  L4/L5.
- **`srv_parse_tok`** : `conforme` — auth d'abord, `DpiBye`→exit(0),
  `set_cookie`→`set_cookie_answer ok|not set`+close(2),
  `get_cookie`→answer+close(2), inconnu→1 — `bin/cookies.rs:41-89`
  vs `cookies.c:1546-1614`. Écart mineur : D15 (write-fail).
- **Cycle de vie démon** : `déviation` — D7 (disabled inversé) +
  D14 (permissions création) + D16 documentée (EOF→continue vs
  exit(1) amont, choix porté).

## Déviations (passe 2)

| # | Déviation | Lentille | Sévérité | Preuves |
|---|-----------|----------|----------|---------|
| D6 | `validate_domain` : `cookie.domain[1..len-1]` via `saturating_sub` — domaine de 1 caractère (`Domain=x` depuis host `x`, ou `Domain=.` etc.) → range `1..0` → **panic du démon** (toutes les cookies mortes). Amont `for i=1; i<strlen-1` → 0 itérations → rejet propre. | L1 | **fonctionnelle** | `cookies.c:1195-1199` vs `cookies.rs:1009` |
| D7 | `bin/cookies.rs` : `if enabled != 2 && txt.is_some() { jar.disabled = false }` — amont `Cookie_control_init()!=0` → `disabled=TRUE` → `exit(1)` **avant** d'ouvrir `cookies.txt`. Ici `1` (cookiesrc tout-DENY — le fichier auto-créé « DEFAULT DENY » !) active quand même le démon : il répond `not set`/`""` au lieu de sortir, et crée `cookies.txt` inutilement. | L3/L5 | **fonctionnelle** | `cookies.c:349-364,1662-1663` vs `bin/cookies.rs:138-169` |
| D8 | `LINE_MAXLEN=4096` non reproduit dans le jar : `load`/`control_init` itèrent `contents.lines()` sans borne — amont `fgets` coupe la ligne et re-parse le reste comme ligne suivante. (Même classe que D3 passe-1, corrigée côté `net` via `fgets_lines` — pas propagée au crate dpi.) | L1 | mineure | `cookies.c:250,1424,1440` vs `cookies.rs:174,515` |
| D9 | `trim()`/`split_whitespace` Unicode vs `dStrstrip`/`dIsspace` ASCII (C locale) dans `load` et `control_init` — U+00A0 strippé chez nous, pas amont. (D5 passe-1 resurface dans le jar.) | L1 | cosmétique | `cookies.c:264,1448` vs `cookies.rs:175,179,516` |
| D10 | `load` : `epoch_time + secs` sur `i64::MAX` parsé → overflow (panic debug / wrap release → négatif = expiré). Amont `strtol`→LONG_MAX → `tm_min += secs/60` → `mktime` → `(time_t)-1` → cookie expiré, sans UB. De plus `parse::<i64>` strict vs `strtol` tolérant-préfixe (`"99abc"`→99 amont, −1 chez nous — même issue « expiré » ici). Fix : `saturating_add` + parse préfixe. | L1 | mineure | `cookies.c:291-304` vs `cookies.rs:525-528` |
| D11 | `load` : amont n'exige que `name`/`value` **non-NULL** (champ vide accepté — `dStrdup("")`), ligne `dom\tF\t/\tF\t99\t\tv` → cookie `""=v` stocké. Le port exige `!name.is_empty()` → ligne droppée. Wire-visible marginal. | L5 | mineure | `cookies.c:305-313` vs `cookies.rs:529-539` |
| D12 | `Max-Age` : `v.parse::<i64>()` strict → `Max-Age=3600abc` échoue → `i64::MAX` → clamp 1 an ; amont `strtol` → 3600. | L5 | mineure | `cookies.c:939-961` vs `cookies.rs:863-881` |
| D13 | `Cookies_get` boucle de suffixes : sur hôte à dots consécutifs (`a..com`), le port émet `.com` en double **et** `com` nu (recherche du '.' suivant à partir de `p+1` alors qu'amont repart après `domain_str++`, soit `p+2`). Domaines suffixes matchés en trop sur hôte malformé. | L1/L5 | mineure | `cookies.c:1367-1381` vs `cookies.rs:483-494` |
| D14 | `cookies_fopen` : amont crée `0600` (`S_IRUSR|S_IWUSR`) + `FD_CLOEXEC` ; le port `File::create` → permissions umask (0666&umask — `cookies.txt` potentiellement lisible-groupe/autres). | L3/L5 | wire-visible | `cookies.c:191-223` vs `bin/cookies.rs:93-112` |
| D15 | `get_cookie` : `a_Dpip_dsh_write_str` en échec → amont `ret=1` → exit(1) ; le port ignore le résultat → ret 2 (dégradation silencieuse au lieu de redémarrage via dpid). | L3 | mineure | `cookies.c:1603-1608` vs `bin/cookies.rs:80-85` |

## Invariants de frontière

- `CookieJar` reste mono-état global du démon — `Rc<RefCell<Cookie>>`
  partagé entre `all_cookies` et nœuds domaine ≙ pointeurs amont
  entre les deux Dlist (mutation `last_used` visible des deux côtés).
- Persistance = `String` produit par `save()`, écrite par le caller
  (truncate+write) — ≙ `rewind`+`ftruncate`+`fprintf` amont ; le
  lock POSIX (`fcntl F_SETLK`, disable si concurrent) **non porté** :
  documenté dans `bin/cookies.rs` (risque = 2 instances, dpid n'en
  lance qu'une) — divergence assumée, à réévaluer si double instance.
- `epoch_time`/`future_time` = `mktime` **heure locale** via `time`
  crate — DST approximé (`current_local_offset` au moment de
  l'appel) : ≙ mais pas bit-exact pour dates traversant un
  changement d'heure ; observable seulement sur expirations à la
  frontière DST.
- Le reste de la passe 1 reste la frontière : consommation
  `Cookie:`/`WWW-Authenticate` → A11 ; protocole/dpid/dsh → A12.

## Fichiers du fix (apply ciblé)

- `crates/dillo-dpi/src/cookies.rs` — D6 (borne `[1..len-1]`),
  D8 (`fgets_lines` 4095 ou helper partagé), D9 (strip ASCII),
  D10 (`saturating_add` + strtol-préfixe), D11 (name vide),
  D12 (strtol-préfixe Max-Age), D13 (repartir de `p+2`).
- `crates/dillo-dpi/src/bin/cookies.rs` — D7 (`enabled == 0`
  requis + return avant création txt), D14 (mode 0600 unix),
  D15 (write-fail → 1).
- `dillo-net/src/lib.rs::fgets_lines` — à rendre `pub` ou déplacer
  vers `dillo-common` pour usage cross-crate (D8).

## Non-bugs tranchés (passe 2)

- `dList_remove_fast(all_cookies)` vs `retain` : l'ordre de
  `all_cookies` n'est observable ni dans LRU (min by key) ni dans
  `save` (itère les nœuds triés) — pas un bug.
- Re-find `node_idx` inconditionnel après purge totale vs
  `else if (domain_cookies)` amont : quand `domain_cookies` était
  NULL le find retourne aussi None — même résultat.
- `srv_parse_tok` EOF→`break` (démon survit) vs `exit(1)` amont :
  choix porté documenté `bin/cookies.rs:190-195` — amont tue le
  démon à chaque déconnexion prématurée et dpid le relance ; ici le
  démon persiste. Divergence d'ops, pas de protocole.
- `Max-Age=-5abc` → '-' → age 0 des deux côtés ; `Max-Age=` vide →
  ignoré des deux côtés (`dIsdigit(0)`=faux ≙ `starts_with` faux).
- `value` des lignes `cookies.txt` peut contenir `\t` (splitn(7)
  ≙ `line_marker` résiduel incluant tabs amont) — conforme.

## Passe 1 (conservée)

### Verdicts

`a_Url_new` lenient, résolution relative RFC-à-la-Dillo,
`a_Url_str`, `cookiesrc` (net), politique cookies, `domain.c`,
auth challenges + arbre realms, digest (MD5/md-5, RFC 2617 golden),
HSTS (+preload), history, dpip helpers — tous `conforme`
(preuves dans l'historique du rapport, commit `5aacbcd`).
`Set-Cookie`/jar/écriture fichiers : étaient `N-A` → désormais
audités (ci-dessus).

### Apply passe 1

- D1 `decode_hex_octet` tolérance `strtol` (`%+f`,`% f`) — corrigé.
- D2 `frag_eq` fragment absent ≡ `""` — corrigé.
- D3 `fgets_lines` borne 4095 (net::cookies/hsts) — corrigé ;
  **D8 passe-2** est la même borne manquante côté jar dpi.
- D4 création `cookiesrc` 0600 + `DEFAULT DENY\n` — corrigé.
- D5 strip ASCII vs `trim()` — corrigé dans `dillo-net` ;
  **D9 passe-2** est la resurface dans le jar dpi.

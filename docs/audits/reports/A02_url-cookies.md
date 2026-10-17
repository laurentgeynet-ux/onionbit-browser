# Audit A02 — URL, cookies, domaines, auth & HSTS

Date : après commit `110cb6b` (lot B pré-requis `cache.c`/`capi.c`).
Mode : `rapport`.
Périmètre couvert : `src/{url.c,url.h,domain.c,cookies.c,cookies.h,
auth.c,auth.h,digest.c,digest.h,hsts.c,hsts.h,history.c,history.h}` +
`dpip/dpip.c` (helpers) → `crates/dillo-net/src/{url,domain,cookies,
auth,digest,hsts,history,dpip}.rs` (3 503 lignes).
Note chemin : le skill attendait `[browser]::{cookies,domain,auth,
digest,hsts}` — placés dans `dillo-net` car leurs seules dépendances
sont `url`/`dlib`/`dpip` (graphe descendant conservé).

Hors périmètre porté à ce jour : jar cookies (`dpi/cookies.c` —
parsing `Set-Cookie`, fichier `~/.dillo/cookies.txt`, éviction 20) →
phase DPI / A12 ; transport `a_Dpi_send_blocking_cmd` + `dsh_*` +
`a_Dpip_check_auth` → phase DPI ; consommateurs `cache.c`/`nav`/`uicmd`
→ lot C.

## Verdicts checklist

- **`a_Url_new` lenient** : `conforme` — strip `URL:`/`<…>`/chars
  illégaux, schéma absent→http, espaces supprimés (pas terminateur),
  champs vides vs absents préservés (`Option<Vec<u8>>`), hostname
  lazy ≙ `a_Url_hostname`, heuristique `isalpha+'.'`→dns — `url.rs`
  vs `src/url.c:96-560`. Tests : batterie RFC-3986 complète +
  quirks (18 tests). L1/L5.
- **Résolution relative** : `conforme` — `resolve_relative`
  RFC-3986-à-la-Dillo : dot-segments, `//`, `?` seul (query remplace),
  `#` seul, `base` sans scheme — `url.rs` vs `src/url.c` (batterie
  du vecteur `Test_url_resolve`). Pas de crate stricte. L5.
- **`a_Url_str`** : `conforme` — sérialisation verbatim : `scheme:`,
  `//authority`, `/` inséré si authority + path non-`/` (`url.c:81`),
  `?query`, `#fragment` — `url.rs:165-190` vs `url.c:65-90`. L5.
- **Cookies — `cookiesrc`** : `conforme` — `Cookie_control_init` :
  `dStrstrip`, `#`/vide skip, domain=run non-espace, rule=run suivant,
  `ACCEPT|ACCEPT_SESSION|DENY` ci, `DEFAULT`→`default_action`, insertion
  longest-first, `enabled` ssi une règle non-DENY (y compris DEFAULT),
  code retour 0/1/2 — `cookies.rs:85-140` vs `cookies.c:242-330`. L1/L2.
- **Cookies — politique** : `conforme` — `control_check_domain` :
  `.dom` → suffixe `diff>=0` SANS match apex (différence volontaire
  vs `domain.c`), sinon exact ci, premier match = plus spécifique,
  défaut sinon ; `DENY`→drop ; requester cross-org non-racine→`""`
  (`same_organization` + `is_root_url`) — `cookies.rs:143-243` vs
  `cookies.c:150-227`. Commandes dpip `set_cookie`/`get_cookie`
  octet-exactes (test golden `get_query_builds_dpip_cmd` :
  `<cmd='get_cookie' scheme='http' host='h.tld' path='/dir/x' '>`).
  L4/L5.
- **`domain.c`** : `conforme` (audité lot A) — `domainrc` strtok,
  texte surnuméraire = warn mais règle ajoutée, `Domain_match` :
  `*`, exact ci, `.suffixe` frontière (`diff==-1` = avec le point),
  `permit` short-circuit `data:`/host vide, match inverse
  `default_deny` — `domain.rs` vs `domain.c`. L1/L2.
- **Auth — parse challenges** : `conforme` — `Auth_parse_token_value`
  verbatim : cb-0 n'interrompt que le token courant (retourne 1 →
  les tokens suivants SONT lus — `auth.c:218-225`), token-run +
  LWS + `=`, `stale`/`algorithm` non-quotés (`expect_quoted` vs
  `(*auth)[-1]=='"'` ≙ `was_quoted`), `Auth_unquote_value` avec
  `\x` escape (`value[1]` falsy si NUL ≙ `v[i+1]!=0`), `realm` ci
  / autres tokens cs (`strcmp` amont), offset `Digest `=7/`Basic `=6,
  exigences finales realm (Basic) / realm+nonce (Digest) —
  `auth.rs:95-330` vs `auth.c:104-351`. L1/L2.
- **Auth — arbre realms** : `conforme` — `AuthHost` (scheme+authority
  ci) → `AuthRealm` ; `realm_add_path` stocke `URL_PATH` **complet**
  (fichier inclus — les frères `/dir/x` ne matchent PAS `/dir/page` ;
  test `basic_flow_end_to_end` prouve), trailing-`/` strippé,
  chemins couverts supprimés, `path_is_inside` = préfixe+frontière
  `/`, longest-path via `realm_idx_by_path` ; `stale`→refresh nonce
  sans dialog ; Digest avant Basic ; dialog injecté `AuthDialog` ≙
  `a_Dialog_user_password` ; `dStrshred` du password (move+fill(0) —
  safe) — `auth.rs:390-590` vs `auth.c:353-686`. L3/L5.
- **Digest** : `conforme` — MD5 via `md-5` (≙ `md5.c` vendu, jamais
  porté), cnonce 32-hex `fastrand` ≙ `rand()%16`, A1/A2/response
  verbatim (`MD5`/`MD5-sess`/`auth`/`auth-int` tous calculés —
  l'activation reste filtrée côté challenge), ordre de champs IE
  verbatim (`username,realm,nonce,uri,[cnonce,nc=%08x],[algorithm],
  response,[opaque],[qop]`, `algorithm` non-quoté, `qop` **quoté**),
  `nonce_count` incrémenté en place avant usage, `opaque`
  `Option<String>` ≙ NULL-vs-"" (`auth.c:194`), `POST` selon
  `URL_Post`, `URL_DATA` entité — `digest.rs` vs `digest.c`. Test :
  vecteur RFC 2617 §3.5 (`response="6629fae4…"`). L5.
- **HSTS** : `conforme` — `a_Hsts_set` : `cert_clean` injecté ≙
  `a_Tls_certificate_is_clean` (refus si warnings), attrs
  `max-age`/`includeSubDomains`/`preload`(ignoré)/inconnu(warn+eat),
  `strtol`+ERANGE→`INT_MAX` ≙ `strtol10`, `dIsdigit(*value)` requis,
  `max-age=0`→`remove_policy`, expiry `now > expires_at`, marche des
  domaines parents `strchr(host+1,'.')`+`subdomains`,
  `latest_representable=INT_MAX` ≙ mktime(2038) — `hsts.rs:100-199`
  vs `hsts.c:200-306`. Preload `domain true|false` → `ONE_YEAR`,
  `dStrstrip`≙trim — `hsts.rs:202-220` vs `hsts.c:308-353`. L1/L4.
- **History** : `conforme` (1 déviation, D2) — dedup `url_cmp`==0 +
  fragment, `get_title`/`_by_url` (`force`→`URL_STR` — de l'url
  passée, verbatim), `set_title_by_url` scan inverse + `MSG_ERR` —
  `history.rs` vs `history.c`. L3.
- **DPIP helpers** : `conforme` — `build_cmd` : `%s`→`'arg'` stuffing
  `'`→`''`, `'` dans format→NULL, EOT ` '>'` ; `get_attr_l` : machine
  `SEEK_NAME→MATCH_NAME→SKIP_VALUE/SKIP_QUOTE→FOUND` verbatim (y
  compris l'itération `FOUND` qui avance `i` de 2 — `start=i+1`),
  destuff à la copie, quote de fin suivie d'espace requise —
  `dpip.rs` vs `dpip.c:60-186`. Tests goldens. L1/L5.
- **Punycode/IDN** : `N-A` — amont ne fait pas d'IDN (pas de libidn
  dans le build — grep `libidn|idna|punycode` vide) ; comportement
  identique par construction.
- **`Set-Cookie` + `cookies.txt` jar** : `N-A` — vit dans
  `dpi/cookies.c` (plugin, phase DPI) ; `test/unit/cookies.c` amont =
  test d'intégration **live** (fork/exec dpid + sockets — toomany
  éviction 20, maxage ±, expires vs server_date) → tests A12.
- **Écriture fichiers** : `N-A` à cette couche — `Cookies_fopen`
  (0600, `O_CREAT|O_TRUNC`, `init_str="DEFAULT DENY\n"`, `FD_CLOEXEC`)
  = glue fichier que le caller devra reproduire (cookies `init`
  reçoit `Option<&str>` ≙ `!stream`→code 2) — voir D4.

## Déviations relevées

| # | Déviation | Lentille | Sévérité | Preuves |
|---|-----------|----------|----------|---------|
| D1 | `decode_hex_str` : amont `Url_decode_hex_octet` = `strtol(hex,…,16)` sur buffer 2 chars → accepte signe/espace/`0x` : `%+f`→`0x0f`, `% f`→`0x0f` (et `%-f`→`-15`→littéral car `val<0`). Le port exige 2 `is_ascii_hexdigit` stricts → ces `%` restent littéraux. Divergence wire-visible sur URLs malformées (Location, liens). | L1/L5 | mineure | `src/url.c:567-602` vs `url.rs` `decode_hex_str` |
| D2 | `history::frag_eq` : amont `URL_FRAGMENT`= `NPTR2STR` → `NULL`≡`""` → `a_History_add_url` déduplique `x` et `x#` (fragment absent vs vide). Le port compare `Option` bruts → doublon possible. | L3 | mineure | `src/history.c:50-52` + `url.h:53-54` vs `history.rs:37-39` |
| D3 | `LINE_MAXLEN=4096` non reproduit : amont `fgets` coupe les lignes >4095 octets → le reste est re-parsé comme une « ligne » (cookiesrc, hsts preload). Le port itère `content.lines()` sans borne. | L1 | cosmétique | `cookies.c:50,263`, `hsts.c:310,320` vs `cookies.rs` `init`, `hsts.rs` `preload` |
| D4 | `Cookies_fopen` : création du `cookiesrc` (0600, `O_CREAT|O_TRUNC`, contenu `DEFAULT DENY\n`, `FD_CLOEXEC`) reportée sur le caller (seam injectée). À honorer dans la glue browser — sinon pas de création auto + permissions non conformes. | L3/L5 | à l'interconnexion | `cookies.c:80-111,253-254` vs `cookies.rs` `init` (seam `Option<&str>`) |
| D5 | `dStrstrip` = `isspace()` ASCII ; le port `str::trim()` = whitespace Unicode — une ligne commençant par U+00A0 est strippée chez nous, pas amont (cookiesrc + preload). | L1 | cosmétique | `cookies.c:272`, `hsts.c:328` vs `.trim()` |

## Analyse des gaps de test

- `decode_hex_str` : pas de test sur `%+f`/`% f` — la déviation D1
  passerait inaperçue ; un golden `assert_eq!(decode_hex_str(b"%+f"),
  b"\x0f")` documentera le comportement amont au moment du fix.
- `history` : pas de test fragment `""`-vs-`None`.
- `cookies`/`hsts` : pas de test ligne >4095.
- Digest : vecteur RFC 2617 présent ✓ ; pas de test `stale`→refresh
  sans dialog (comportement amont couvert par lecture, à goldenner
  avec le fix).
- DPIP : goldens stuffing/destuff présents ✓.

## Frontière avec les audits suivants

- `a_Dpi_send_blocking_cmd`, sockets dpi, `dsh_*`, `a_Dpip_check_auth`,
  jar `dpi/cookies.c` (parsing `Set-Cookie`, `cookies.txt` byte-exact,
  éviction, `max-age`/`expires` vs `server_date`) → **A12-dpi**.
- Consommation des en-têtes `Cookie`/`Authorization`/`WWW-Authenticate`
  dans la réponse → lot C (`cache.c`/`capi.c`/`web.cc`) → **A11**.
- `timeout.cc` (`Auth`-retry notamment) → lot C.

## Recommandation apply

Attendre le lot C (`cache.c`/`capi`/`web`) — D4 ne devient testable
qu'avec la glue fichier navigateur ; D1/D2/D3/D5 sont locaux et
bornés (≤ ~20 lignes + goldens). Coût de dérive : faible (preuves
`fichier:ligne`).

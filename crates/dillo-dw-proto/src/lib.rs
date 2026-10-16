//! dillo-dw-proto — banc d'essai TEMPORAIRE pour l'ADR-0005.
//!
//! Prototype du modèle de propriété de l'arbre de widgets :
//! options A (arène + `WidgetId`), B (`Rc`/`Weak`), D (hybride
//! arène + enum dispatch + `LayoutCtx`) testées sur 3 widgets
//! (`Container`, `Textblock`, `Image`) avec un `queue_resize`
//! traversant `parent` et `generator` pendant une itération.
//!
//! Ce crate sera supprimé une fois l'ADR-0005 tranchée.
//!
//! ## Scénario de stress commun (`build()` dans chaque module)
//!
//! `root = [t1(Textblock trigger→img), img(generator=t1), t2]`.
//! Pendant `size_request(t1)` — donc *sous* l'itération de `root` —
//! t1 appelle `queue_resize(img)` : appel croisé frère + remontée
//! `mark_size_change` vers un ancêtre occupé. C'est le pire cas
//! réel de `dw/` (`Widget::queueResize` en pleine itération).
//!
//! ## Résultats (mesurés, cf. tests `resize_during_iteration_*`)
//!
//! | Option | Panique ? | Fidélité C++ | Friction visible |
//! |---|---|---|---|
//! | A (arène + `dyn`) | non | moyenne (objets≠nœuds) | take/replace `Null` sur *chaque* appel + downcast `Any` pour la config |
//! | B (`Rc`/`Weak`) | non **via defer** | forte | `try_borrow`+déféré partout ; ordre des events ré-ordonné ; registre id→widget |
//! | D (arène + `enum` + ctx) | non | forte (nœud=liens, kind=dispatch) | discipline take/copy des données avant récursion — localisée, mécanique |
//!
//! B « fonctionne » mais uniquement en réinventant la file déférée
//! de D/A *plus* un `try_borrow` sur chaque remontée : le coût est
//! partout, pas localisé. A paie le `dyn` sans gain. D concentre la
//! friction dans un pattern mécanique (extraire→rappeler→réécrire)
//! et garde la fidélité structurelle aux liens croisés amont.
#![forbid(unsafe_code)]

pub mod a;
pub mod b;
pub mod d;

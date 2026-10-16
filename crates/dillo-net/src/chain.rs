//! Port of `src/chain.c` + `src/chain.h` — Concomitant Control Chain
//! (CCC) : chaîne bidirectionnelle de callbacks reliant les modules I/O.
//!
//! Mapping (ADR-0005) :
//! - `ChainLink*` → [`LinkId`] dans une arène [`Chain`] (slab + free-list).
//!   `dFree(Info)` amont → [`Chain::free_link`].
//! - `Info->LocalKey` (`void*` état du module) → `Box<dyn Any>` par lien,
//!   accès typé via [`Chain::local_key_mut`].
//! - `void *Data1/Data2` → [`ChainData`] (tags amont : `"FD"`,
//!   `"send_page_2eof"`, `"reply_complete"`, `"Both"`, `"DpidOK"`, noms
//!   de schéma).
//! - `ChainFunction_t` → `Rc<dyn Fn>` clonée avant appel : unlink ou free
//!   du lien pendant le callback est sûr (l'appelant retient la copie).

use std::any::Any;
use std::collections::HashMap;
use std::rc::Rc;

use tracing::{trace, warn};

/// Ops CCC (`OpStart`…`OpAbort`).
pub const OP_START: i32 = 1;
pub const OP_SEND: i32 = 2;
pub const OP_STOP: i32 = 3;
pub const OP_END: i32 = 4;
pub const OP_ABORT: i32 = 5;

/// Flags CCC.
pub const CCC_STOPPED: i32 = 1 << 0;
pub const CCC_ENDED: i32 = 1 << 1;
pub const CCC_ABORTED: i32 = 1 << 2;

/// Direction de liaison.
pub const FWD: i32 = 1;
pub const BCK: i32 = 2;

/// Identifiant de lien dans l'arène (≙ `ChainLink*`).
pub type LinkId = u32;

/// `DataBuf` — buffer passé entre nœuds (`Buf`/`Size`/`Code` amont ;
/// `buf` owned, `size` peut différer de `buf.len()` comme en amont).
#[derive(Debug, Clone)]
pub struct DataBuf {
    pub buf: Vec<u8>,
    pub size: i32,
    pub code: i32,
}

impl DataBuf {
    /// `a_Chain_dbuf_new`.
    #[must_use]
    pub fn new(buf: Vec<u8>, size: i32, code: i32) -> Self {
        Self { buf, size, code }
    }
}

/// `void *Data1`/`Data2` des callbacks CCC — les usages amont observés :
/// `DataBuf*`, `int*` (descripteur/socket), `char*` (payload texte ou
/// tag `"FD"`), pointeur opaque de module (`DilloWeb*`, `IOData_t*`…).
///
/// `Clone` : `bfcb` livre le même payload aux deux côtés, comme amont
/// livre le même pointeur ; `Owned` partage via `Rc` (downcast
/// `Rc<dyn Any>::downcast::<T>` → `Rc<T>`).
#[derive(Debug, Clone)]
pub enum ChainData {
    /// `NULL`.
    None,
    /// `DataBuf*` (payload binaire I/O).
    Buf(Box<DataBuf>),
    /// `int*` — descripteur ou code (`SockFD`, status…).
    Int(i32),
    /// `char*` — tag ou payload texte (`"FD"`, `"http"`, requêtes dpi…).
    Str(String),
    /// `void*` opaque — handle/ID vers l'état d'un module.
    Handle(usize),
    /// Valeur hétérogène partagée (`Rc<dyn Any>` ≙ `void*` partagé).
    Owned(Rc<dyn Any>),
}

impl ChainData {
    /// `char*` view si `Str`.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// `(char*)Data == tag` — comparaison amont `!strcmp(Data2, tag)`.
    #[must_use]
    pub fn is_tag(&self, tag: &str) -> bool {
        self.as_str() == Some(tag)
    }
}

impl From<&str> for ChainData {
    fn from(s: &str) -> Self {
        Self::Str(s.to_string())
    }
}

/// `ChainFunction_t` : `(Op, Branch, Dir, Info, Data1, Data2)`.
/// `Info` = le lien côté *appelé* (`FcbInfo`/`BcbInfo` amont).
/// `Fn` (pas `FnMut`) : tout l'état mutable passe par `&mut Chain`
/// (`local_key`), la réentrance est ainsi possible comme en C.
pub type ChainCb = Rc<dyn Fn(&mut Chain, i32, i32, i32, LinkId, ChainData, ChainData)>;

/// `struct ChainLink` verbatim (le `void* LocalKey` devient `Any`).
pub struct ChainLink {
    /// `LocalKey` — état privé du module propriétaire du lien.
    pub local_key: Option<Box<dyn Any>>,
    pub flags: i32,

    /// `FcbInfo` — lien du côté appelé en FWD.
    pub fcb_info: Option<LinkId>,
    /// `Fcb` — callback du côté FWD.
    pub fcb: Option<ChainCb>,
    pub fcb_branch: i32,

    /// `BcbInfo`/`Bcb`/`BcbBranch` — côté BCK.
    pub bcb_info: Option<LinkId>,
    pub bcb: Option<ChainCb>,
    pub bcb_branch: i32,
}

/// Arène des liens + registre des modules (`static`s amont → instance).
///
/// Le registre [`Chain::set_module`]/[`Chain::module_mut`] remplace les
/// variables statiques de module (`ValidIOs`, `dns_*`) : les callbacks
/// CCC n'ont accès qu'à `&mut Chain`, donc tout état partagé y vit.
pub struct Chain {
    links: Vec<Option<Box<ChainLink>>>,
    free: Vec<LinkId>,
    modules: HashMap<std::any::TypeId, Box<dyn Any>>,
}

impl Chain {
    #[must_use]
    pub fn new() -> Self {
        Self {
            links: Vec::new(),
            free: Vec::new(),
            modules: HashMap::new(),
        }
    }

    /// `a_Chain_new` — lien frais, tous champs à zéro.
    pub fn new_link(&mut self) -> LinkId {
        let link = Box::new(ChainLink {
            local_key: None,
            flags: 0,
            fcb_info: None,
            fcb: None,
            fcb_branch: 0,
            bcb_info: None,
            bcb: None,
            bcb_branch: 0,
        });
        if let Some(id) = self.free.pop() {
            self.links[id as usize] = Some(link);
            id
        } else {
            self.links.push(Some(link));
            LinkId::try_from(self.links.len())
                .unwrap_or(LinkId::MAX)
                .saturating_sub(1)
        }
    }

    /// `dFree(Info)` — libère le lien (les callbacks en vol conservent
    /// leur `Rc` ; les `LinkId` pendants deviennent inertes).
    pub fn free_link(&mut self, id: LinkId) {
        if let Some(slot) = self.links.get_mut(id as usize) {
            if slot.take().is_some() {
                self.free.push(id);
            }
        }
    }

    /// Accès au lien (`NULL` amont → `None`).
    #[must_use]
    pub fn link(&self, id: LinkId) -> Option<&ChainLink> {
        self.links.get(id as usize)?.as_deref()
    }

    /// Accès mutable au lien.
    pub fn link_mut(&mut self, id: LinkId) -> Option<&mut ChainLink> {
        self.links.get_mut(id as usize)?.as_deref_mut()
    }

    /// `Info->LocalKey = v` — installe l'état du module.
    pub fn set_local_key<T: Any>(&mut self, id: LinkId, v: T) {
        if let Some(l) = self.link_mut(id) {
            l.local_key = Some(Box::new(v));
        }
    }

    /// `Info->LocalKey` typé — `None` si lien mort ou type erroné.
    pub fn local_key_mut<T: Any>(&mut self, id: LinkId) -> Option<&mut T> {
        self.link_mut(id)?.local_key.as_mut()?.downcast_mut()
    }

    /// `Info->LocalKey` (lecture).
    #[must_use]
    pub fn local_key<T: Any>(&self, id: LinkId) -> Option<&T> {
        self.link(id)?.local_key.as_ref()?.downcast_ref()
    }

    /// Registre de module (statique amont → par-instance).
    pub fn set_module<T: Any>(&mut self, m: T) {
        self.modules
            .insert(std::any::TypeId::of::<T>(), Box::new(m));
    }

    /// Accès mutable à un module du registre.
    pub fn module_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.modules
            .get_mut(&std::any::TypeId::of::<T>())?
            .downcast_mut()
    }

    /// `a_Chain_link_new` — crée le lien B côté `Direction` de `a`.
    ///
    /// `dir == BCK` : le nouveau lien appelle `a_func` en FWD vers `a`,
    /// et `a` reçoit `b_func` sur sa face BCK. `dir == FWD` : symétrique.
    #[allow(clippy::too_many_arguments)]
    pub fn link_new(
        &mut self,
        a: LinkId,
        a_func: ChainCb,
        dir: i32,
        b_func: ChainCb,
        a_to_b: i32,
        b_to_a: i32,
    ) -> LinkId {
        let new_link = self.new_link();
        if dir == BCK {
            if let Some(nl) = self.link_mut(new_link) {
                nl.fcb = Some(a_func);
                nl.fcb_info = Some(a);
                nl.fcb_branch = b_to_a;
            }
            if let Some(ol) = self.link_mut(a) {
                ol.bcb = Some(b_func);
                ol.bcb_info = Some(new_link);
                ol.bcb_branch = a_to_b;
            }
        } else {
            if let Some(nl) = self.link_mut(new_link) {
                nl.bcb = Some(a_func);
                nl.bcb_info = Some(a);
                nl.bcb_branch = b_to_a;
            }
            if let Some(ol) = self.link_mut(a) {
                ol.fcb = Some(b_func);
                ol.fcb_info = Some(new_link);
                ol.fcb_branch = a_to_b;
            }
        }
        new_link
    }

    /// `a_Chain_unlink`.
    pub fn unlink(&mut self, id: LinkId, dir: i32) {
        if let Some(l) = self.link_mut(id) {
            if dir == FWD {
                l.fcb = None;
                l.fcb_info = None;
                l.fcb_branch = 0;
            } else {
                l.bcb = None;
                l.bcb_info = None;
                l.bcb_branch = 0;
            }
        }
    }

    /// `a_Chain_fcb` — émet le callback FWD du lien. `false` si non
    /// opérationnel (Ended/Aborted) ou sans callback.
    pub fn fcb(&mut self, op: i32, info: LinkId, d1: ChainData, d2: ChainData) -> bool {
        let Some(l) = self.link(info) else {
            return false;
        };
        if l.flags & (CCC_ENDED | CCC_ABORTED) != 0 {
            return false;
        }
        let Some(cb) = l.fcb.clone() else {
            return false;
        };
        let (branch, target) = (l.fcb_branch, l.fcb_info);
        if op == OP_END || op == OP_ABORT {
            if let Some(l) = self.link_mut(info) {
                l.flags |= if op == OP_END { CCC_ENDED } else { CCC_ABORTED };
            }
        }
        cb(self, op, branch, FWD, target.unwrap_or(u32::MAX), d1, d2);
        true
    }

    /// `a_Chain_bcb` — émet le callback BCK du lien.
    pub fn bcb(&mut self, op: i32, info: LinkId, d1: ChainData, d2: ChainData) -> bool {
        let Some(l) = self.link(info) else {
            return false;
        };
        if l.flags & (CCC_ENDED | CCC_ABORTED) != 0 {
            return false;
        }
        let Some(cb) = l.bcb.clone() else {
            return false;
        };
        let (branch, target) = (l.bcb_branch, l.bcb_info);
        if op == OP_END || op == OP_ABORT {
            if let Some(l) = self.link_mut(info) {
                l.flags |= if op == OP_END { CCC_ENDED } else { CCC_ABORTED };
            }
        }
        cb(self, op, branch, BCK, target.unwrap_or(u32::MAX), d1, d2);
        true
    }

    /// `a_Chain_bfcb` — BCB puis FCB (OpAbort/OpStop) ; le flag est
    /// ré-autorisé entre les deux comme en amont. Le payload est livré
    /// aux deux côtés (amont passe le même pointeur → `clone`).
    pub fn bfcb(&mut self, op: i32, info: LinkId, d1: ChainData, d2: ChainData) -> bool {
        let ret = self.bcb(op, info, d1.clone(), d2.clone());
        if ret {
            if let Some(l) = self.link_mut(info) {
                if op == OP_END {
                    l.flags &= !CCC_ENDED;
                } else if op == OP_ABORT {
                    l.flags &= !CCC_ABORTED;
                }
            }
            return self.fcb(op, info, d1, d2);
        }
        false
    }

    /// `a_Chain_check` — log + `false` si la chaîne est terminée.
    pub fn check(&self, func: &str, op: i32, branch: i32, dir: i32, info: LinkId) -> bool {
        let flags = self.link(info).map_or(-1, |l| l.flags);
        trace!(func, op, branch, dir, info, flags, "CCC");
        if flags & (CCC_ENDED | CCC_ABORTED) != 0 {
            warn!(
                "CCC: call on already finished chain. Flags={}{}",
                if flags & CCC_ENDED != 0 {
                    "CCC_Ended "
                } else {
                    ""
                },
                if flags & CCC_ABORTED != 0 {
                    "CCC_Aborted"
                } else {
                    ""
                }
            );
            return false;
        }
        true
    }
}

impl Default for Chain {
    fn default() -> Self {
        Self::new()
    }
}

/// `a_Chain_dbuf_new` libre (compat).
#[must_use]
pub fn chain_dbuf_new(buf: Vec<u8>, size: i32, code: i32) -> DataBuf {
    DataBuf::new(buf, size, code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    type Log = Rc<RefCell<Vec<String>>>;

    fn cb(name: &'static str, log: Log) -> ChainCb {
        Rc::new(move |_ch, op, branch, dir, _info, _d1, _d2| {
            let ops = ["", "OpStart", "OpSend", "OpStop", "OpEnd", "OpAbort"];
            log.borrow_mut().push(format!(
                "{name}:{}:{branch}:{}",
                ops[usize::try_from(op).unwrap_or(0)],
                dir
            ));
        })
    }

    #[test]
    fn link_fwd_and_flow() {
        let mut ch = Chain::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let a = ch.new_link();
        let b = ch.link_new(a, cb("A", log.clone()), FWD, cb("B", log.clone()), 1, 2);
        // a.fcb → B ; b.bcb → A
        assert!(ch.fcb(OP_SEND, a, ChainData::None, ChainData::None));
        assert!(ch.bcb(OP_SEND, b, ChainData::None, ChainData::None));
        assert_eq!(
            *log.borrow(),
            vec!["B:OpSend:1:1".to_string(), "A:OpSend:2:2".to_string()]
        );
    }

    #[test]
    fn ended_blocks_callback() {
        let mut ch = Chain::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let a = ch.new_link();
        let _b = ch.link_new(a, cb("A", log.clone()), FWD, cb("B", log.clone()), 1, 1);
        assert!(ch.fcb(OP_END, a, ChainData::None, ChainData::None));
        // CCC_Ended : plus opérationnel
        assert!(!ch.fcb(OP_SEND, a, ChainData::None, ChainData::None));
        assert!(!ch.check("t", OP_SEND, 1, FWD, a));
        assert_eq!(log.borrow().len(), 1);
    }

    #[test]
    fn bfcb_abort_resets_flag_between() {
        let mut ch = Chain::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let a = ch.new_link();
        let b = ch.link_new(a, cb("A", log.clone()), FWD, cb("B", log.clone()), 1, 1);
        // a.bcb existe ? non — link_new FWD ne pose que a.fcb et b.bcb.
        // bfcb(b) → bcb(b) = A (ret 1), flag nettoyé, puis fcb(b) = rien
        // (b.fcb absent) → a_Chain_fcb retourne 0 → false, comme amont.
        assert!(!ch.bfcb(OP_ABORT, b, ChainData::None, ChainData::None));
        assert_eq!(*log.borrow(), vec!["A:OpAbort:1:2".to_string()]);
    }

    #[test]
    fn reentrant_abort_inside_callback() {
        // un callback qui émet un OpAbort réentrant (pattern amont fréquent :
        // `a_Chain_bcb/fcb` dans le corps d'un handler CCC)
        let mut ch = Chain::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let log2 = log.clone();
        // callback de B (côté FWD de a) : réémet OpAbort en BCK vers A
        let b_cb: ChainCb = Rc::new(move |ch, _op, _b, _d, info, _d1, _d2| {
            log2.borrow_mut().push("B".to_string());
            ch.bcb(OP_ABORT, info, ChainData::None, ChainData::None);
        });
        let a = ch.new_link();
        let _b = ch.link_new(a, cb("K", log.clone()), FWD, b_cb, 1, 1);
        ch.fcb(OP_SEND, a, ChainData::None, ChainData::None);
        // "B" puis "K" (bcb abort réentrant appelle A via b.bcb)
        assert_eq!(
            *log.borrow(),
            vec!["B".to_string(), "K:OpAbort:1:2".to_string()]
        );
    }

    #[test]
    fn free_link_makes_id_inert() {
        let mut ch = Chain::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let a = ch.new_link();
        let b = ch.link_new(a, cb("A", log.clone()), FWD, cb("B", log.clone()), 1, 1);
        ch.free_link(b);
        assert!(ch.link(b).is_none());
        // fcb(a) tente d'appeler B via un lien mort : l'appel se fait
        // quand même (Info passé = id mort) — le cb de B survit via Rc ?
        // amont : Info->FcbInfo dangling → UB ; ici : inerte propre.
        ch.fcb(OP_SEND, a, ChainData::None, ChainData::None);
        assert_eq!(log.borrow().len(), 1); // B appelé avec un LinkId mort
    }
}

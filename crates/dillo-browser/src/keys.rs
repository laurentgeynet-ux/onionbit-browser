//! Port de `src/keys.cc` + `src/keys.hh` : bindings clavier (`keysrc`),
//! table de touches par défaut et génération `about:keys`.
//!
//! Les codes de touches sont les *keysyms* X11/FLTK (valeurs `FL_*`),
//! verbatim, afin que les entrées `0x…` du `keysrc` utilisateur restent
//! compatibles. Les bits de modificateurs sont internes au port (espace
//! compact — producteur `key_cmd` et parseur `keysrc` partagent les mêmes
//! constantes).

#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

use std::cmp::Ordering;
use std::fmt::Write;

use dillo_common::dlib;
use dillo_common::utf8;
use dillo_net::cache::ca_flags as ca;
use dillo_net::cache::cache_entry_inject;
use dillo_net::url::url_new;

/// Bits de modificateurs internes (≙ `FL_SHIFT`/`FL_CTRL`/`FL_ALT`/…).
pub mod mods {
    /// `FL_SHIFT`.
    pub const SHIFT: i32 = 1 << 0;
    /// `FL_CTRL`.
    pub const CTRL: i32 = 1 << 1;
    /// `FL_ALT`.
    pub const ALT: i32 = 1 << 2;
    /// `FL_META`.
    pub const META: i32 = 1 << 3;
    /// `FL_BUTTON1` (≙ keys.cc `modifierNames`).
    pub const BUTTON1: i32 = 1 << 4;
    /// `FL_BUTTON2`.
    pub const BUTTON2: i32 = 1 << 5;
    /// `FL_BUTTON3`.
    pub const BUTTON3: i32 = 1 << 6;
}

/// Keysyms X11 = valeurs `FL_*` (verbatim — compat `keysrc` `0x…`).
pub mod keysym {
    /// `FL_BackSpace`.
    pub const BACKSPACE: i32 = 0xff08;
    /// `FL_Tab`.
    pub const TAB: i32 = 0xff09;
    /// `FL_Enter`.
    pub const ENTER: i32 = 0xff0d;
    /// `FL_Escape`.
    pub const ESCAPE: i32 = 0xff1b;
    /// `FL_Home`.
    pub const HOME: i32 = 0xff50;
    /// `FL_Left`.
    pub const LEFT: i32 = 0xff51;
    /// `FL_Up`.
    pub const UP: i32 = 0xff52;
    /// `FL_Right`.
    pub const RIGHT: i32 = 0xff53;
    /// `FL_Down`.
    pub const DOWN: i32 = 0xff54;
    /// `FL_Page_Up`.
    pub const PAGE_UP: i32 = 0xff55;
    /// `FL_Page_Down`.
    pub const PAGE_DOWN: i32 = 0xff56;
    /// `FL_End`.
    pub const END: i32 = 0xff57;
    /// `FL_Print`.
    pub const PRINT: i32 = 0xff61;
    /// `FL_Insert`.
    pub const INSERT: i32 = 0xff63;
    /// `FL_Menu`.
    pub const MENU: i32 = 0xff67;
    /// `FL_F + n` : F1 = `0xffbe`.
    pub const F1: i32 = 0xffbe;
    /// `FL_Delete`.
    pub const DELETE: i32 = 0xffff;
    /// `FL_Back` (`XF86Back`).
    pub const BACK: i32 = 0x1008_ff26;
    /// `FL_Forward` (`XF86Forward`).
    pub const FORWARD: i32 = 0x1008_ff27;
    /// `FL_Stop` (`XF86Stop`).
    pub const STOP: i32 = 0x1008_ff28;
    /// `FL_Refresh` (`XF86Refresh`).
    pub const REFRESH: i32 = 0x1008_ff29;
    /// `FL_Sleep` (`XF86Sleep`).
    pub const SLEEP: i32 = 0x1008_ff2f;
    /// `FL_Home_Page` (`XF86HomePage`).
    pub const HOME_PAGE: i32 = 0x1008_ff18;
    /// `FL_Mail` (`XF86Mail`).
    pub const MAIL: i32 = 0x1008_ff19;
    /// `FL_Search` (`XF86Search`).
    pub const SEARCH: i32 = 0x1008_ff1b;
    /// `FL_Favorites` (`XF86Favorites`).
    pub const FAVORITES: i32 = 0x1008_ff30;
    /// `FL_Volume_Mute` (`XF86AudioMute`).
    pub const VOLUME_MUTE: i32 = 0x1008_ff12;
    /// `FL_Volume_Down` (`XF86AudioLowerVolume`).
    pub const VOLUME_DOWN: i32 = 0x1008_ff11;
    /// `FL_Volume_Up` (`XF86AudioRaiseVolume`).
    pub const VOLUME_UP: i32 = 0x1008_ff13;
    /// `FL_Media_Play` (`XF86AudioPlay`).
    pub const MEDIA_PLAY: i32 = 0x1008_ff14;
    /// `FL_Media_Stop` (`XF86AudioStop`).
    pub const MEDIA_STOP: i32 = 0x1008_ff15;
    /// `FL_Media_Prev` (`XF86AudioPrev`).
    pub const MEDIA_PREV: i32 = 0x1008_ff16;
    /// `FL_Media_Next` (`XF86AudioNext`).
    pub const MEDIA_NEXT: i32 = 0x1008_ff17;
}

/// `KeysCommand_t` (verbatim `keys.hh`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum KeysCommand {
    /// `KEYS_INVALID` — commande inconnue (retour de `cmd_code`).
    Invalid = -1,
    /// `KEYS_NOP` — aucune opération liée.
    #[default]
    Nop = 0,
    /// `KEYS_OPEN`.
    Open,
    /// `KEYS_NEW_WINDOW`.
    NewWindow,
    /// `KEYS_NEW_TAB`.
    NewTab,
    /// `KEYS_LEFT_TAB`.
    LeftTab,
    /// `KEYS_RIGHT_TAB`.
    RightTab,
    /// `KEYS_CLOSE_TAB`.
    CloseTab,
    /// `KEYS_FIRST_TAB`.
    FirstTab,
    /// `KEYS_LAST_TAB`.
    LastTab,
    /// `KEYS_FIND`.
    Find,
    /// `KEYS_WEBSEARCH`.
    WebSearch,
    /// `KEYS_BOOKMARKS`.
    Bookmarks,
    /// `KEYS_RELOAD`.
    Reload,
    /// `KEYS_STOP`.
    Stop,
    /// `KEYS_SAVE`.
    Save,
    /// `KEYS_HIDE_PANELS`.
    HidePanels,
    /// `KEYS_FILE_MENU`.
    FileMenu,
    /// `KEYS_CLOSE_ALL`.
    CloseAll,
    /// `KEYS_BACK`.
    Back,
    /// `KEYS_FORWARD`.
    Forward,
    /// `KEYS_GOTO`.
    Goto,
    /// `KEYS_HOME`.
    Home,
    /// `KEYS_VIEW_SOURCE`.
    ViewSource,
    /// `KEYS_SCREEN_UP`.
    ScreenUp,
    /// `KEYS_SCREEN_DOWN`.
    ScreenDown,
    /// `KEYS_SCREEN_LEFT`.
    ScreenLeft,
    /// `KEYS_SCREEN_RIGHT`.
    ScreenRight,
    /// `KEYS_LINE_UP`.
    LineUp,
    /// `KEYS_LINE_DOWN`.
    LineDown,
    /// `KEYS_LEFT`.
    Left,
    /// `KEYS_RIGHT`.
    Right,
    /// `KEYS_TOP`.
    Top,
    /// `KEYS_BOTTOM`.
    Bottom,
    /// `KEYS_COPY`.
    Copy,
    /// `KEYS_ZOOM_IN`.
    ZoomIn,
    /// `KEYS_ZOOM_OUT`.
    ZoomOut,
    /// `KEYS_ZOOM_RESET`.
    ZoomReset,
    /// `KEYS_FOCUS_TAB1` … `KEYS_FOCUS_TAB10`.
    FocusTab1,
    /// `KEYS_FOCUS_TAB2`.
    FocusTab2,
    /// `KEYS_FOCUS_TAB3`.
    FocusTab3,
    /// `KEYS_FOCUS_TAB4`.
    FocusTab4,
    /// `KEYS_FOCUS_TAB5`.
    FocusTab5,
    /// `KEYS_FOCUS_TAB6`.
    FocusTab6,
    /// `KEYS_FOCUS_TAB7`.
    FocusTab7,
    /// `KEYS_FOCUS_TAB8`.
    FocusTab8,
    /// `KEYS_FOCUS_TAB9`.
    FocusTab9,
    /// `KEYS_FOCUS_TAB10`.
    FocusTab10,
}

/// `KeyBinding_t` (verbatim).
#[derive(Debug)]
struct KeyBinding {
    name: String,
    cmd: KeysCommand,
    modifier: i32,
    key: i32,
}

/// `Mapping_t` (verbatim).
struct Mapping {
    name: &'static str,
    value: i32,
}

/// `keyNames[]` (verbatim `keys.cc` — valeurs X11 keysym).
const KEY_NAMES: &[Mapping] = &[
    Mapping {
        name: "Backspace",
        value: keysym::BACKSPACE,
    },
    Mapping {
        name: "Delete",
        value: keysym::DELETE,
    },
    Mapping {
        name: "Down",
        value: keysym::DOWN,
    },
    Mapping {
        name: "End",
        value: keysym::END,
    },
    Mapping {
        name: "Esc",
        value: keysym::ESCAPE,
    },
    Mapping {
        name: "F1",
        value: keysym::F1,
    },
    Mapping {
        name: "F2",
        value: keysym::F1 + 1,
    },
    Mapping {
        name: "F3",
        value: keysym::F1 + 2,
    },
    Mapping {
        name: "F4",
        value: keysym::F1 + 3,
    },
    Mapping {
        name: "F5",
        value: keysym::F1 + 4,
    },
    Mapping {
        name: "F6",
        value: keysym::F1 + 5,
    },
    Mapping {
        name: "F7",
        value: keysym::F1 + 6,
    },
    Mapping {
        name: "F8",
        value: keysym::F1 + 7,
    },
    Mapping {
        name: "F9",
        value: keysym::F1 + 8,
    },
    Mapping {
        name: "F10",
        value: keysym::F1 + 9,
    },
    Mapping {
        name: "F11",
        value: keysym::F1 + 10,
    },
    Mapping {
        name: "F12",
        value: keysym::F1 + 11,
    },
    Mapping {
        name: "Home",
        value: keysym::HOME,
    },
    Mapping {
        name: "Insert",
        value: keysym::INSERT,
    },
    Mapping {
        name: "Left",
        value: keysym::LEFT,
    },
    Mapping {
        name: "Menu",
        value: keysym::MENU,
    },
    Mapping {
        name: "PageDown",
        value: keysym::PAGE_DOWN,
    },
    Mapping {
        name: "PageUp",
        value: keysym::PAGE_UP,
    },
    Mapping {
        name: "Print",
        value: keysym::PRINT,
    },
    Mapping {
        name: "Return",
        value: keysym::ENTER,
    },
    Mapping {
        name: "Right",
        value: keysym::RIGHT,
    },
    Mapping {
        name: "Space",
        value: ' ' as i32,
    },
    Mapping {
        name: "Tab",
        value: keysym::TAB,
    },
    Mapping {
        name: "Up",
        value: keysym::UP,
    },
    /* multimedia keys */
    Mapping {
        name: "Back",
        value: keysym::BACK,
    },
    Mapping {
        name: "Favorites",
        value: keysym::FAVORITES,
    },
    Mapping {
        name: "Forward",
        value: keysym::FORWARD,
    },
    Mapping {
        name: "HomePage",
        value: keysym::HOME_PAGE,
    },
    Mapping {
        name: "Mail",
        value: keysym::MAIL,
    },
    Mapping {
        name: "MediaNext",
        value: keysym::MEDIA_NEXT,
    },
    Mapping {
        name: "MediaPlay",
        value: keysym::MEDIA_PLAY,
    },
    Mapping {
        name: "MediaPrev",
        value: keysym::MEDIA_PREV,
    },
    Mapping {
        name: "MediaStop",
        value: keysym::MEDIA_STOP,
    },
    Mapping {
        name: "Refresh",
        value: keysym::REFRESH,
    },
    Mapping {
        name: "Search",
        value: keysym::SEARCH,
    },
    Mapping {
        name: "Sleep",
        value: keysym::SLEEP,
    },
    Mapping {
        name: "Stop",
        value: keysym::STOP,
    },
    Mapping {
        name: "VolumeDown",
        value: keysym::VOLUME_DOWN,
    },
    Mapping {
        name: "VolumeMute",
        value: keysym::VOLUME_MUTE,
    },
    Mapping {
        name: "VolumeUp",
        value: keysym::VOLUME_UP,
    },
];

/// `modifierNames[]` (verbatim — bits internes `mods::*`).
const MODIFIER_NAMES: &[Mapping] = &[
    Mapping {
        name: "Shift",
        value: mods::SHIFT,
    },
    Mapping {
        name: "Ctrl",
        value: mods::CTRL,
    },
    Mapping {
        name: "Alt",
        value: mods::ALT,
    },
    Mapping {
        name: "Meta",
        value: mods::META,
    },
    Mapping {
        name: "Button1",
        value: mods::BUTTON1,
    },
    Mapping {
        name: "Button2",
        value: mods::BUTTON2,
    },
    Mapping {
        name: "Button3",
        value: mods::BUTTON3,
    },
];

/// `default_keys[]` (verbatim — 52 entrées).
const DEFAULT_KEYS: &[(&str, KeysCommand, i32, i32)] = &[
    ("nop", KeysCommand::Nop, 0, 0),
    ("open", KeysCommand::Open, mods::CTRL, 'o' as i32),
    ("new-window", KeysCommand::NewWindow, mods::CTRL, 'n' as i32),
    ("new-tab", KeysCommand::NewTab, mods::CTRL, 't' as i32),
    (
        "left-tab",
        KeysCommand::LeftTab,
        mods::CTRL | mods::SHIFT,
        keysym::TAB,
    ),
    (
        "left-tab",
        KeysCommand::LeftTab,
        mods::CTRL,
        keysym::PAGE_UP,
    ),
    ("right-tab", KeysCommand::RightTab, mods::CTRL, keysym::TAB),
    (
        "right-tab",
        KeysCommand::RightTab,
        mods::CTRL,
        keysym::PAGE_DOWN,
    ),
    ("close-tab", KeysCommand::CloseTab, mods::CTRL, 'w' as i32),
    ("find", KeysCommand::Find, mods::CTRL, 'f' as i32),
    ("websearch", KeysCommand::WebSearch, mods::CTRL, 's' as i32),
    ("bookmarks", KeysCommand::Bookmarks, mods::CTRL, 'b' as i32),
    ("reload", KeysCommand::Reload, mods::CTRL, 'r' as i32),
    ("stop", KeysCommand::Stop, 0, 0),
    ("save", KeysCommand::Save, 0, 0),
    ("hide-panels", KeysCommand::HidePanels, 0, keysym::ESCAPE),
    ("file-menu", KeysCommand::FileMenu, mods::ALT, 'f' as i32),
    ("close-all", KeysCommand::CloseAll, mods::CTRL, 'q' as i32),
    ("back", KeysCommand::Back, 0, keysym::BACKSPACE),
    ("back", KeysCommand::Back, 0, ',' as i32),
    (
        "forward",
        KeysCommand::Forward,
        mods::SHIFT,
        keysym::BACKSPACE,
    ),
    ("forward", KeysCommand::Forward, 0, '.' as i32),
    ("goto", KeysCommand::Goto, mods::CTRL, 'l' as i32),
    ("home", KeysCommand::Home, mods::CTRL, 'h' as i32),
    (
        "view-source",
        KeysCommand::ViewSource,
        mods::CTRL,
        'u' as i32,
    ),
    ("screen-up", KeysCommand::ScreenUp, 0, keysym::PAGE_UP),
    ("screen-up", KeysCommand::ScreenUp, 0, 'b' as i32),
    ("screen-down", KeysCommand::ScreenDown, 0, keysym::PAGE_DOWN),
    ("screen-down", KeysCommand::ScreenDown, 0, ' ' as i32),
    ("screen-left", KeysCommand::ScreenLeft, 0, 0),
    ("screen-right", KeysCommand::ScreenRight, 0, 0),
    ("line-up", KeysCommand::LineUp, 0, keysym::UP),
    ("line-down", KeysCommand::LineDown, 0, keysym::DOWN),
    ("left", KeysCommand::Left, 0, keysym::LEFT),
    ("right", KeysCommand::Right, 0, keysym::RIGHT),
    ("top", KeysCommand::Top, 0, keysym::HOME),
    ("bottom", KeysCommand::Bottom, 0, keysym::END),
    ("copy", KeysCommand::Copy, mods::CTRL, 'c' as i32),
    ("zoom-in", KeysCommand::ZoomIn, mods::CTRL, '+' as i32),
    ("zoom-in", KeysCommand::ZoomIn, mods::CTRL, '=' as i32), /* US + */
    ("zoom-out", KeysCommand::ZoomOut, mods::CTRL, '-' as i32),
    ("zoom-reset", KeysCommand::ZoomReset, mods::CTRL, '0' as i32),
    ("focus-tab1", KeysCommand::FocusTab1, mods::ALT, '1' as i32),
    ("focus-tab2", KeysCommand::FocusTab2, mods::ALT, '2' as i32),
    ("focus-tab3", KeysCommand::FocusTab3, mods::ALT, '3' as i32),
    ("focus-tab4", KeysCommand::FocusTab4, mods::ALT, '4' as i32),
    ("focus-tab5", KeysCommand::FocusTab5, mods::ALT, '5' as i32),
    ("focus-tab6", KeysCommand::FocusTab6, mods::ALT, '6' as i32),
    ("focus-tab7", KeysCommand::FocusTab7, mods::ALT, '7' as i32),
    ("focus-tab8", KeysCommand::FocusTab8, mods::ALT, '8' as i32),
    ("focus-tab9", KeysCommand::FocusTab9, mods::ALT, '9' as i32),
    (
        "focus-tab10",
        KeysCommand::FocusTab10,
        mods::ALT,
        '0' as i32,
    ),
];

/// `class Keys` (verbatim `keys.hh`) — la liste `bindings` devient un
/// champ (l'amont utilise une variable statique globale).
pub struct Keys {
    /// `bindings` — liste triée `(key, modifier)` (≙ `dList_insert_sorted`
    /// + `dList_find_sorted` avec `nodeByKeyCmp`).
    bindings: Vec<KeyBinding>,
}

/// `Keys::nodeByKeyCmp` (verbatim).
fn node_by_key_cmp(node: &KeyBinding, key: i32, modifier: i32) -> Ordering {
    if node.key == key {
        (node.modifier - modifier).cmp(&0)
    } else {
        (node.key - key).cmp(&0)
    }
}

impl Keys {
    /// `Keys::init` (verbatim) — remplissage depuis `default_keys`.
    #[must_use]
    pub fn new() -> Self {
        let mut k = Keys {
            bindings: Vec::with_capacity(32),
        };
        for (name, cmd, modifier, key) in DEFAULT_KEYS {
            if *key != 0 {
                k.insert(KeyBinding {
                    name: (*name).to_string(),
                    cmd: *cmd,
                    modifier: *modifier,
                    key: *key,
                });
            }
        }
        k
    }

    /// `dList_insert_sorted` (tri stable sur `(key, modifier)`).
    fn insert(&mut self, node: KeyBinding) {
        let pos = self
            .bindings
            .iter()
            .position(|n| {
                matches!(
                    node_by_key_cmp(n, node.key, node.modifier),
                    Ordering::Greater
                )
            })
            .unwrap_or(self.bindings.len());
        self.bindings.insert(pos, node);
    }

    /// `dList_find_sorted` (recherche binaire ≙ `nodeByKeyCmp`).
    fn find(&self, key: i32, modifier: i32) -> Option<usize> {
        self.bindings
            .binary_search_by(|n| {
                if n.key == key {
                    (n.modifier - modifier).cmp(&0)
                } else {
                    (n.key - key).cmp(&0)
                }
            })
            .ok()
    }

    /// `Keys::getKeyCmd` (verbatim).
    ///
    /// `event_key`/`event_text`/`event_state` ≙ `Fl::event_key()`/
    /// `Fl::event_text()`/`Fl::event_state()`. L'appelant (couche
    /// fenêtre) traduit l'événement natif en keysym + texte UTF-8 + mods.
    #[must_use]
    pub fn key_cmd(&self, event_key: i32, event_text: &[u8], event_state: i32) -> KeysCommand {
        let mut modifier = event_state & (mods::SHIFT | mods::CTRL | mods::ALT | mods::META);
        let key;
        // verbatim `dIscntrl(Fl::event_text()[0])` : texte vide → `'\0'`,
        // qui est un caractère de contrôle → `key = event_key`.
        if dlib::is_cntrl(event_text.first().copied().unwrap_or(0)) {
            key = event_key;
        } else {
            let (k, _len) = utf8::decode(event_text, 0, event_text.len());
            key = k;
            /* BUG: The idea is to drop the modifiers if their use results
             * in a different character (e.g., if shift-8 gives '*', drop
             * the shift, but if ctrl-6 gives '6', keep the ctrl), but we
             * have to compare a keysym with a Unicode codepoint, which
             * only works for characters below U+0100 (those known to
             * latin-1). [commentaire amont verbatim] */
            if key != event_key {
                modifier = 0;
            }
        }
        self.find(key, modifier)
            .map_or(KeysCommand::Nop, |i| self.bindings[i].cmd)
    }

    /// `Keys::delKeyCmd` (verbatim).
    fn del_key_cmd(&mut self, key: i32, modifier: i32) {
        if let Some(i) = self.find(key, modifier) {
            self.bindings.remove(i);
        }
    }

    /// `Keys::getKeyCode` (verbatim).
    #[must_use]
    pub fn key_code(key_name: &str) -> i32 {
        for m in KEY_NAMES {
            if dlib::str_ascii_casecmp(m.name.as_bytes(), key_name.as_bytes()) == 0 {
                return m.value;
            }
        }
        -1
    }

    /// `Keys::getKeyName` (verbatim — `static char buf` devient `String`).
    #[must_use]
    pub fn key_name(key: i32) -> String {
        for m in KEY_NAMES {
            if m.value == key {
                return m.name.to_string();
            }
        }
        if u8::try_from(key).is_ok_and(dlib::is_ascii) {
            char::from_u32(key as u32).map_or_else(|| format!("0x{key:x}"), |c| c.to_string())
        } else {
            /* Otherwise print hexadecimal */
            format!("0x{key:x}")
        }
    }

    /// `Keys::getCmdCode` (verbatim — recherche dans `default_keys`).
    #[must_use]
    pub fn cmd_code(command_name: &str) -> KeysCommand {
        for (name, cmd, _, _) in DEFAULT_KEYS {
            if dlib::str_ascii_casecmp(name.as_bytes(), command_name.as_bytes()) == 0 {
                return *cmd;
            }
        }
        KeysCommand::Invalid
    }

    /// `Keys::getModifier` (verbatim).
    #[must_use]
    pub fn modifier(modifier_name: &str) -> i32 {
        for m in MODIFIER_NAMES {
            if dlib::str_ascii_casecmp(m.name.as_bytes(), modifier_name.as_bytes()) == 0 {
                return m.value;
            }
        }
        -1
    }

    /// `Keys::getShortcut` (verbatim).
    #[must_use]
    pub fn shortcut(&self, cmd: KeysCommand) -> i32 {
        for node in &self.bindings {
            if cmd == node.cmd {
                return node.modifier + node.key;
            }
        }
        0
    }

    /// `Keys::parseKey` (verbatim `keysrc` : `"<mod>key = command"`).
    pub fn parse_key(&mut self, key: &str, command_name: &str) {
        // Get command code
        let symcode = Self::cmd_code(command_name);
        if symcode == KeysCommand::Invalid {
            eprintln!("Keys::parseKey: Invalid command name: '{command_name}'");
            return;
        }

        let kb = key.as_bytes();
        let mut i = 0usize;
        // Skip space
        while i < kb.len() && dlib::is_space(kb[i]) {
            i += 1;
        }
        // Get modifiers
        let mut keymod = 0i32;
        while i < kb.len() && kb[i] == b'<' {
            let Some(p) = kb[i..].iter().position(|&c| c == b'>').map(|q| i + q) else {
                break;
            };
            i += 1;
            let modstr = &key[i..p];
            let st = Self::modifier(modstr);
            if st == -1 {
                eprintln!("Keys::parseKey unknown modifier: {modstr}");
            } else {
                keymod |= st;
            }
            i = p + 1;
        }
        // Allow trailing space after keyname
        // (verbatim `dStrndup(key, p - key - 1)` : première espace à partir
        // de l'index 1, copie `pos - 1` octets — quirk amont préservé ;
        // en pratique `parse_rc_line` a déjà strippé la valeur).
        let rest = &key[i..];
        let keystr: &str = if rest.is_empty() {
            rest
        } else if let Some(pos) = rest[1..].find(' ').map(|q| q + 1) {
            &rest[..pos.saturating_sub(1)]
        } else {
            rest
        };
        // Get key code
        let mut keycode = 0i32;
        if rest.len() == 1 {
            keycode = rest.as_bytes()[0] as i32;
        } else if utf8::char_count(keystr.as_bytes()) == 1 {
            let (k, _l) = utf8::decode(keystr.as_bytes(), 0, keystr.len());
            keycode = k;
        } else if rest.len() > 2 && &rest[..2] == "0x" {
            /* keysym */
            keycode = i32::from_str_radix(&rest[2..], 0x10).unwrap_or(0);
        } else {
            let st = Self::key_code(keystr);
            if st == -1 {
                eprintln!("Keys::parseKey unknown keyname: {keystr}");
            } else {
                keycode = st;
            }
        }

        // Set binding
        if keycode != 0 {
            self.del_key_cmd(keycode, keymod);
            if symcode != KeysCommand::Nop {
                self.insert(KeyBinding {
                    name: command_name.to_string(),
                    cmd: symcode,
                    modifier: keymod,
                    key: keycode,
                });
            }
        }
    }

    /// `Keys::parse` (verbatim — lit le fichier `keysrc` ligne à ligne).
    pub fn parse(&mut self, content: &str) {
        // scan the file line by line
        for (lineno, raw) in (1..).zip(content.lines()) {
            match dlib::parse_rc_line(raw.as_bytes()) {
                dlib::RcLine::Pair { name, value } => {
                    let keycomb = String::from_utf8_lossy(&name);
                    let command = String::from_utf8_lossy(&value);
                    self.parse_key(&keycomb, &command);
                }
                dlib::RcLine::Error => {
                    eprintln!(
                        "Keys::parse: Syntax error in keysrc line {lineno}: \
                         keycomb=\"{raw}\""
                    );
                }
                dlib::RcLine::Skip => {}
            }
        }
    }

    /// `Keys::genAboutKeys` (verbatim — table HTML injectée dans le
    /// cache sous `about:keys`).
    pub fn gen_about_keys(&self) {
        let mut table = String::from(
            "<!DOCTYPE HTML>\n\
             <html>\n\
             <head>\n  <title>Keyboard shortcuts</title>\n  <style>\n    \
             body {\n      background: white;\n      margin: 3em;\n      \
             font-size: 16px;\n      font-family: sans-serif;\n      \
             line-height: 1.4em;\n    }\n    .main { max-width: 40em; }\n    \
             p { margin-top: 1em; }\n    table {\n      \
             border-collapse: collapse;\n    }\n    th,td {\n      \
             padding: 0.25em;\n      padding: 0.25em;\n      \
             border-spacing: 0px;\n      border-top: solid 1px #ccc;\n      \
             border-bottom: solid 1px #ccc;\n    }\n    td.key {\n      \
             min-width: 15em;\n    }\n    td.action {\n      \
             min-width: 10em;\n    }\n    th {\n      \
             background-color: #eee;\n    }\n    kbd {\n      \
             display: inline-block;\n      border: solid 1px #aaa;\n      \
             padding: 1px 5px;\n    }\n    kbd.mod {\n      \
             background: #f5f5f5;\n    }\n  </style>\n</head>\n<body>\n\
             <div class=\"main\">\n\n<h1>Keyboard shortcuts</h1>\n\n\
             <p>The following table contains the current key bindings in \
             Dillo.\nTo change them, edit the configuration file \
             <code>~/.dillo/keysrc</code> \nand restart the browser.</p>\n\
             <table>\n<tr><th>Shortcut<th>Action</tr>\n",
        );

        for node in &self.bindings {
            let key = Self::key_name(node.key);
            table.push_str("<tr><td class=key><code>");
            for m in MODIFIER_NAMES {
                if m.value & node.modifier != 0 {
                    let _ = write!(table, "<kbd class=mod>{}</kbd> ", m.name);
                }
            }
            let _ = writeln!(
                table,
                "<kbd class=key>{key}</kbd></code></td><td class=action>\
                 <code>{}</code></td></tr>",
                node.name
            );
        }

        table.push_str("</table>\n</div>\n</body>\n</html>\n");

        /* inject keymaps after loading them */
        if let Some(url) = url_new(b"about:keys", None) {
            cache_entry_inject(
                &url,
                table.as_bytes(),
                ca::GOT_HEADER | ca::GOT_LENGTH | ca::INTERNAL_URL,
            );
        }
    }
}

impl Default for Keys {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_lookup() {
        let k = Keys::new();
        // verbatim : Ctrl+o → Open (caractère contrôle → keysym direct)
        assert_eq!(
            k.key_cmd('o' as i32, &[0x0f], mods::CTRL),
            KeysCommand::Open
        );
        // Backspace → Back (pas de texte)
        assert_eq!(k.key_cmd(keysym::BACKSPACE, &[0x08], 0), KeysCommand::Back);
        // Ctrl+Tab → RightTab
        assert_eq!(
            k.key_cmd(keysym::TAB, &[0x09], mods::CTRL),
            KeysCommand::RightTab
        );
    }

    #[test]
    fn shifted_char_drops_mods() {
        // BUG amont verbatim : '+' via shift → mods tombés si keysym≠texte.
        let k = Keys::new();
        // event_key='=' (0x3d) produisant '+' avec shift → key='+' ≠ '=' →
        // modifier = 0 → pas de match ('+' non lié sans ctrl) → Nop.
        assert_eq!(k.key_cmd('=' as i32, b"+", mods::SHIFT), KeysCommand::Nop);
        // ctrl+'=' → ZoomIn (texte '=' = keysym, ctrl conservé)
        assert_eq!(
            k.key_cmd('=' as i32, &[0x1d], mods::CTRL | mods::SHIFT),
            // ctrl caractère → keysym '=' + SHIFT : pas lié → Nop
            KeysCommand::Nop
        );
        // ctrl caractère → keysym '=' + CTRL : lié → ZoomIn
        assert_eq!(
            k.key_cmd('=' as i32, &[0x1d], mods::CTRL),
            KeysCommand::ZoomIn
        );
    }

    #[test]
    fn parse_and_rebind() {
        let mut k = Keys::new();
        k.parse("<ctrl><shift>x = open\n# comment\n<alt>F5 = reload\n");
        // Ctrl+Shift+x désormais Open
        assert_eq!(
            k.key_cmd('x' as i32, &[0x18], mods::CTRL | mods::SHIFT),
            KeysCommand::Open
        );
        assert_eq!(
            k.key_cmd(keysym::F1 + 4, &[], mods::ALT),
            KeysCommand::Reload
        );
        // rebinding supprime l'ancien : Ctrl+o → Nop après rebind
        k.parse("<ctrl>o = nop\n");
        assert_eq!(k.key_cmd('o' as i32, &[0x0f], mods::CTRL), KeysCommand::Nop);
    }

    #[test]
    fn cmd_code_and_names() {
        assert_eq!(Keys::cmd_code("open"), KeysCommand::Open);
        assert_eq!(Keys::cmd_code("nosuch"), KeysCommand::Invalid);
        assert_eq!(Keys::key_code("Home"), keysym::HOME);
        assert_eq!(Keys::key_code("nosuch"), -1);
        assert_eq!(Keys::modifier("Ctrl"), mods::CTRL);
        assert_eq!(Keys::key_name(keysym::PAGE_UP), "PageUp");
        assert_eq!(Keys::key_name('a' as i32), "a");
        assert_eq!(Keys::key_name(0x12345), "0x12345");
        let k = Keys::new();
        assert_eq!(k.shortcut(KeysCommand::Open), mods::CTRL + 'o' as i32);
        assert_eq!(k.shortcut(KeysCommand::ScreenLeft), 0);
    }
}

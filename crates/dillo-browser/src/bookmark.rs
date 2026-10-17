//! Port de `src/bookmark.c` + `bookmark.h` — client du serveur dpi
//! `bookmarks` (exemple de « chat » dpip amont).
//!
//! Amont garde `cmd1..cmd4` + `bw` en statiques de fonction ; ici un
//! struct [`Bookmarks`] (même cycle de vie : un par navigateur, les
//! commandes construites paresseusement une fois).
//!
//! Le dialogue : `chat_add(bw, cmd, NULL)` envoie `"Hi server"` ; le
//! serveur répond `"Hi browser"` (→ `"I want to set a bookmark"`),
//! `"Is it worth?"` (→ `"Sure it is!"`), `"OK, send it!"` (→ la
//! commande `add_bookmark` stockée). `chat_add` est aussi la cible de
//! `DpiOps::chat_add` côté `capi` quand le serveur `bookmarks` cause.

use dillo_net::capi;
use dillo_net::dpip;
use dillo_net::history;
use dillo_net::url::DilloUrl;

/// État du chat bookmarks (statiques de fonction amont).
#[derive(Default)]
pub struct Bookmarks {
    /// `cmd1` — `"chat" "Hi server"` (lazy).
    cmd1: Option<String>,
    /// `cmd2` — `"chat" "I want to set a bookmark"`.
    cmd2: Option<String>,
    /// `cmd3` — `"chat" "Sure it is!"`.
    cmd3: Option<String>,
    /// `cmd4` — la commande `add_bookmark` en attente.
    cmd4: Option<String>,
    /// `bw` statique amont — mémorisée au premier appel avec bw non-nul.
    bw: u32,
    /// `bw` vue au moins une fois (amont compare `Bw` à NULL).
    have_bw: bool,
}

impl Bookmarks {
    /// `a_Bookmarks_chat_add` — machine d'état du dialogue.
    ///
    /// `bw` = `Some(id)` ou `None` (≙ `NULL` — amont garde la dernière
    /// non-NULL). `cmd` = commande à envoyer quand le serveur dit « OK,
    /// send it! ». `answer` = `None` pour amorcer, `Some(text)` pour
    /// répondre (la fonction n'est rappelée que par le serveur).
    pub fn chat_add(&mut self, bw: Option<u32>, cmd: Option<&str>, answer: Option<&str>) {
        // Construction lazy des cmd1..3 (bloc `if (!cmd1)` amont).
        if self.cmd1.is_none() {
            self.cmd1 = dpip::build_cmd("cmd=%s msg=%s", &["chat", "Hi server"]);
            self.cmd2 = dpip::build_cmd("cmd=%s msg=%s", &["chat", "I want to set a bookmark"]);
            self.cmd3 = dpip::build_cmd("cmd=%s msg=%s", &["chat", "Sure it is!"]);
        }

        if let Some(b) = bw {
            self.bw = b;
            self.have_bw = true;
        }
        if self.cmd4.is_none() {
            if let Some(c) = cmd {
                self.cmd4 = Some(c.to_string());
            }
        }

        let bw_id = self.bw;
        match answer {
            None => {
                // Amorce : "Hi server" (flag 1 = `DpidBranch` amont).
                if let Some(c) = &self.cmd1 {
                    let c = c.clone();
                    capi::capi_dpi_send_cmd(None, bw_id, &c, "bookmarks", 1);
                }
            }
            Some(a) => {
                // Le serveur a répondu : dispatch sur le 1er caractère.
                let send = match a.as_bytes().first() {
                    Some(b'H') => self.cmd2.clone(), // "Hi browser"
                    Some(b'I') => self.cmd3.clone(), // "Is it worth?"
                    Some(b'O') => self.cmd4.take(),  // "OK, send it!"
                    _ => None,
                };
                if let Some(c) = send {
                    capi::capi_dpi_send_cmd(None, bw_id, &c, "bookmarks", 0);
                }
            }
        }
    }

    /// `a_Bookmarks_add` — titre = `a_History_get_title_by_url(url, 1)`
    /// (fallback `URL_STR` côté history), commande `add_bookmark`.
    pub fn add(&mut self, bw: u32, url: &mut DilloUrl) {
        let title = history::history_get_title_by_url(url, true).unwrap_or_default();
        let url_str = String::from_utf8_lossy(url.url_str()).into_owned();
        let cmd = dpip::build_cmd(
            "cmd=%s url=%s title=%s",
            &["add_bookmark", &url_str, &title],
        );
        if let Some(cmd) = cmd {
            self.chat_add(Some(bw), Some(&cmd), None);
        }
    }

    /// Cible `DpiOps::chat_add` — le serveur `bookmarks` a causé
    /// (`a_Dpi_ccc` → `a_Bookmarks_chat_add(NULL, NULL, msg)` amont).
    pub fn server_said(&mut self, msg: &str) {
        if self.have_bw {
            self.chat_add(None, None, Some(msg));
        }
    }
}

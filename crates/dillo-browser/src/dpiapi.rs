//! Port de `src/dpiapi.c` + `dpiapi.h` — interface dpip côté Dillo.
//!
//! `a_Dpiapi_dialog` : un serveur dpi envoie un tag `dialog` — on parse
//! les attributs `title`/`msg`/`alt1..5` via `a_Dpip_get_attr_l`, on
//! affiche `a_Dialog_choice` (seam [`DialogChoice`], modal amont →
//! réponse immédiate), puis on renvoie
//! `cmd=answer to_cmd=dialog msg=<n>` au serveur via `a_Capi_dpi_send_cmd`.
//!
//! `dialog_server` amont est un static module — ici champ de [`Dpiapi`]
//! (un par navigateur, même cycle de vie observable).

use dillo_net::capi;
use dillo_net::dpip;

/// Seam `a_Dialog_choice` — retourne l'index du bouton choisi
/// (0 = premier… ; amont passe les `alt*` comme boutons dans l'ordre).
pub trait DialogChoice {
    /// `a_Dialog_choice(title, msg, alt1..alt5, NULL)`.
    fn choice(&mut self, title: &str, msg: &str, alts: &[Option<String>]) -> i32;
}

/// État `dpiapi` (static `dialog_server` amont).
#[derive(Default)]
pub struct Dpiapi {
    /// `dialog_server` — serveur à répondre (dernier dialog reçu).
    dialog_server: String,
}

impl Dpiapi {
    /// `Dpiapi_dialog_answer_cb` — fabrique le tag `answer` et l'envoie.
    fn answer_cb(&mut self, bw: u32, answer: i32) {
        let numstr = answer.to_string();
        if let Some(cmd) =
            dpip::build_cmd("cmd=%s to_cmd=%s msg=%s", &["answer", "dialog", &numstr])
        {
            let server = std::mem::take(&mut self.dialog_server);
            capi::capi_dpi_send_cmd(None, bw, &cmd, &server, 0);
        }
    }

    /// `a_Dpiapi_dialog` — traite un tag `dialog` d'un serveur dpi :
    /// parse les attrs, affiche le choix, renvoie la réponse.
    pub fn dialog(&mut self, bw: u32, server: &str, dpip_tag: &str, dlg: &mut dyn DialogChoice) {
        self.dialog_server = server.to_string();

        let attrs: Vec<Option<String>> = ["title", "msg", "alt1", "alt2", "alt3", "alt4", "alt5"]
            .iter()
            .map(|name| dpip::get_attr(dpip_tag, name))
            .collect();

        let title = attrs[0].clone().unwrap_or_default();
        let msg = attrs[1].clone().unwrap_or_default();
        let alts = &attrs[2..];

        // `a_Dialog_choice` modal amont → la réponse part de suite.
        let ret = dlg.choice(&title, &msg, alts);
        self.answer_cb(bw, ret);
    }
}

//! Port of `src/form.cc` + `form.hh` — formulaires HTML.
//!
//! `DilloHtmlForm`/`DilloHtmlInput`/`DilloHtmlSelect`/
//! `DilloHtmlOption` verbatim : widgets `Embed` + `Resource` (factory
//! du layout), receivers `activate`/`clicked`, soumission GET
//! (query) ou POST (`urlencoded`/`multipart`).
//!
//! Écart documenté (PORTING.md) : pas d'`iconv` — `submit_charset`
//! non-UTF-8 émet le `MSG_WARN` verbatim et soumet en UTF-8.
#![allow(clippy::doc_markdown)]
#![allow(clippy::too_many_lines)] // fonctions verbatim monolithiques.
#![allow(clippy::cast_possible_truncation)] // `int`/`float` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use dillo_common::{dlib, misc};
use dillo_css::css::{CssPropertyName as Prop, CssValueType};
use dillo_dw::WidgetId;
use dillo_dw::events::{EventButton, button_state};
use dillo_dw::ui::{ActivateReceiver, ClickedReceiver, Resource, SelectionMode, SelectionResource};
use dillo_net::url::{self, DilloUrl};

use crate::common::{DocType, ParseMode, in_flags};
use crate::html::{DilloHtml, HtmlShared};

/// `dStrAsciiCasecmp`.
fn str_icmp(a: &[u8], b: &[u8]) -> i32 {
    dlib::str_ascii_casecmp(a, b)
}

// ------------------------------------------------------------------
// Typedefs (form.cc:44-63, form.hh:12-24)
// ------------------------------------------------------------------

/// `DilloHtmlInputType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    /// `DILLO_HTML_INPUT_UNKNOWN`.
    Unknown,
    /// `DILLO_HTML_INPUT_TEXT`.
    Text,
    /// `DILLO_HTML_INPUT_PASSWORD`.
    Password,
    /// `DILLO_HTML_INPUT_CHECKBOX`.
    Checkbox,
    /// `DILLO_HTML_INPUT_RADIO`.
    Radio,
    /// `DILLO_HTML_INPUT_IMAGE`.
    Image,
    /// `DILLO_HTML_INPUT_FILE`.
    File,
    /// `DILLO_HTML_INPUT_BUTTON`.
    Button,
    /// `DILLO_HTML_INPUT_HIDDEN`.
    Hidden,
    /// `DILLO_HTML_INPUT_SUBMIT`.
    Submit,
    /// `DILLO_HTML_INPUT_RESET`.
    Reset,
    /// `DILLO_HTML_INPUT_BUTTON_SUBMIT`.
    ButtonSubmit,
    /// `DILLO_HTML_INPUT_BUTTON_RESET`.
    ButtonReset,
    /// `DILLO_HTML_INPUT_SELECT`.
    Select,
    /// `DILLO_HTML_INPUT_SEL_LIST`.
    SelList,
    /// `DILLO_HTML_INPUT_TEXTAREA`.
    Textarea,
    /// `DILLO_HTML_INPUT_INDEX`.
    Index,
}

/// `DilloHtmlMethod`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// `DILLO_HTML_METHOD_UNKNOWN`.
    Unknown,
    /// `DILLO_HTML_METHOD_GET`.
    Get,
    /// `DILLO_HTML_METHOD_POST`.
    Post,
}

/// `DilloHtmlEnc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enc {
    /// `DILLO_HTML_ENC_URLENCODED`.
    Urlencoded,
    /// `DILLO_HTML_ENC_MULTIPART`.
    Multipart,
}

// ------------------------------------------------------------------
// Optbase / Optgroup / OptgroupClose / Option (form.cc:163-217)
// ------------------------------------------------------------------

/// `DilloHtmlOptbase` — la hiérarchie devient une enum (variantes
/// verbatim : groupe ouvrant, fermeture de groupe, option).
enum Optbase {
    /// `DilloHtmlOptgroup` — `pushGroup`.
    Optgroup {
        /// `label`.
        label: String,
        /// `enabled`.
        enabled: bool,
    },
    /// `DilloHtmlOptgroupClose` — `popGroup`.
    OptgroupClose,
    /// `DilloHtmlOption`.
    Option {
        /// `value`.
        value: Option<String>,
        /// `label`.
        label: Option<String>,
        /// `content` — texte stashé par `Html_option_finish`.
        content: Option<String>,
        /// `selected`.
        selected: bool,
        /// `enabled`.
        enabled: bool,
    },
}

impl Optbase {
    /// `isSelected` — `false` pour les groupes (verbatim).
    fn is_selected(&self) -> bool {
        match self {
            Optbase::Option { selected, .. } => *selected,
            _ => false,
        }
    }

    /// `select` — `false` pour les groupes (verbatim).
    fn select(&mut self) -> bool {
        match self {
            Optbase::Option { selected, .. } => {
                *selected = true;
                true
            }
            _ => false,
        }
    }

    /// `getValue` — `value ? value : content` (verbatim) ;
    /// `NULL` pour les groupes.
    fn get_value(&self) -> Option<&str> {
        match self {
            Optbase::Option { value, content, .. } => value.as_deref().or(content.as_deref()),
            _ => None,
        }
    }

    /// `setContent` — verbatim : seul `Option` le fait (les
    /// groupes émettent l'erreur `MSG_ERR` amont).
    fn set_content(&mut self, str_: &[u8]) {
        if let Optbase::Option { content, .. } = self {
            *content = Some(String::from_utf8_lossy(str_).into_owned());
        } else {
            tracing::error!("Form: Optbase setContent()");
        }
    }

    /// `addSelf` — `pushGroup`/`popGroup`/`addItem` verbatim.
    fn add_self(&self, res: &dyn SelectionResource) {
        match self {
            Optbase::Optgroup { label, enabled } => {
                res.push_group(label, *enabled);
            }
            Optbase::OptgroupClose => res.pop_group(),
            Optbase::Option {
                label,
                content,
                selected,
                enabled,
                ..
            } => {
                res.add_item(
                    label.as_deref().or(content.as_deref()).unwrap_or(""),
                    *enabled,
                    *selected,
                );
            }
        }
    }
}

// ------------------------------------------------------------------
// DilloHtmlSelect (form.cc:1903-2000)
// ------------------------------------------------------------------

/// `DilloHtmlSelect` — options d'un `<select>`.
pub struct HtmlSelect {
    /// `opts`.
    opts: Vec<Optbase>,
}

impl HtmlSelect {
    /// `DilloHtmlSelect()`.
    fn new() -> Self {
        HtmlSelect { opts: Vec::new() }
    }

    /// `getCurrentOpt` — dernière option (`None` ≙ `NULL`).
    fn current_opt(&mut self) -> Option<&mut Optbase> {
        self.opts.last_mut()
    }

    /// `addOpt`.
    fn add_opt(&mut self, opt: Optbase) {
        self.opts.push(opt);
    }

    /// `ensureSelection` — première option si rien de sélectionné.
    fn ensure_selection(&mut self) {
        if self.opts.is_empty() || self.opts.iter().any(Optbase::is_selected) {
            return;
        }
        for opt in &mut self.opts {
            if opt.select() {
                break;
            }
        }
    }

    /// `addOptsTo`.
    fn add_opts_to(&self, res: &dyn SelectionResource) {
        for opt in &self.opts {
            opt.add_self(res);
        }
    }

    /// `reset` — `setItem(i, isSelected)`.
    fn reset(&self, res: &dyn SelectionResource) {
        for (i, opt) in self.opts.iter().enumerate() {
            res.set_item(i as i32, opt.is_selected());
        }
    }

    /// `appendValuesTo`.
    fn append_values_to(&self, values: &mut Vec<Vec<u8>>, res: &dyn SelectionResource) {
        for (i, opt) in self.opts.iter().enumerate() {
            if res.is_selected(i as i32)
                && let Some(v) = opt.get_value()
            {
                values.push(v.as_bytes().to_vec());
            }
        }
    }
}

// ------------------------------------------------------------------
// DilloHtmlInput (form.cc:135-161)
// ------------------------------------------------------------------

/// `DilloHtmlInput`.
pub struct HtmlInput {
    /// `embed` — `None` ≙ `NULL` amont (input invisible).
    pub embed: Option<WidgetId>,
    /// `owns_embed` — gardé pour la lecture verbatim (l'arène
    /// possède tous les widgets ; rien à libérer).
    pub owns_embed: bool,
    /// `type`.
    pub ty: InputType,
    /// `name`.
    pub name: Option<Vec<u8>>,
    /// `init_str` — valeur initiale (boutons : value/label).
    pub init_str: Option<Vec<u8>>,
    /// `select` — créé pour SELECT/SEL_LIST (verbatim).
    pub select: Option<HtmlSelect>,
    /// `init_val` — état initial des toggles.
    pub init_val: bool,
    /// `file_data`.
    file_data: Option<Vec<u8>>,
    /// `html` ≙ `shared` (sink + arena + prefs) — `Weak` ≙
    /// pointeur non-possédant amont.
    shared: Weak<RefCell<HtmlShared>>,
}

impl HtmlInput {
    /// `DilloHtmlInput()` (form.cc:1722) — `assert(embed)` amont :
    /// `embed` est `Option` ici car certains inputs n'ont pas de
    /// widget (hidden non-affiché ≙ `Embed` quand même créé amont ;
    /// `None` toléré pour la robustesse).
    fn new(
        ty: InputType,
        embed: Option<WidgetId>,
        name: Option<Vec<u8>>,
        init_str: Option<Vec<u8>>,
        init_val: bool,
        owns_embed: bool,
        shared: &Rc<RefCell<HtmlShared>>,
    ) -> Self {
        let select = match ty {
            InputType::Select | InputType::SelList => Some(HtmlSelect::new()),
            _ => None,
        };
        let input = HtmlInput {
            embed,
            owns_embed,
            ty,
            name,
            init_str,
            select,
            init_val,
            file_data: None,
            shared: Rc::downgrade(shared),
        };
        input.reset();
        input
    }

    /// `getResource` — `None` ≙ `NULL`. Lookup direct sur l'arène :
    /// `layout` peut être `borrow_mut` quand un receiver tourne sous
    /// le dispatch d'événement widget (A03 passe-2 D7).
    #[must_use]
    pub fn get_resource(&self) -> Option<Rc<dyn Resource>> {
        let embed = self.embed?;
        self.shared
            .upgrade()?
            .borrow()
            .arena
            .borrow()
            .embed_resource(embed)
    }

    /// `setEnabled` (form.cc:176 — `if (embed)`).
    pub fn set_enabled(&self, enabled: bool) {
        if let Some(res) = self.get_resource() {
            res.set_enabled(enabled);
        }
    }

    /// `connectTo` (form.cc:1755) — `connectClicked` toujours ;
    /// `connectActivate` pour les types listés (verbatim).
    fn connect_to(&self, receivers: &FormReceivers) {
        let Some(res) = self.get_resource() else {
            return;
        };
        res.connect_clicked(&receivers.clicked);
        if matches!(
            self.ty,
            InputType::Submit
                | InputType::Reset
                | InputType::ButtonSubmit
                | InputType::ButtonReset
                | InputType::Image
                | InputType::File
                | InputType::Text
                | InputType::Password
                | InputType::Index
        ) {
            res.connect_activate(&receivers.activate);
        }
    }

    /// `readFile` (form.cc:1822) — `a_UIcmd_select_file` +
    /// `a_Misc_file2dstr` via le sink.
    fn read_file(&mut self) {
        let Some(sh) = self.shared.upgrade() else {
            return;
        };
        let sink = sh.borrow().sink.clone();
        let mut sink = sink.borrow_mut();
        let Some(filename) = sink.select_file() else {
            return;
        };
        sink.set_msg(b"Loading file...");
        let data = sink.file_to_string(&filename);
        if let Some(d) = data {
            sink.set_msg(b"File loaded.");
            self.file_data = Some(d);
            if let Some(res) = self.get_resource()
                && let Some(lbr) = res.as_label_button_resource()
            {
                lbr.set_label(&filename);
            }
        } else {
            let msg = format!("ERROR: can't load: {filename}");
            sink.set_msg(msg.as_bytes());
        }
    }

    /// `appendValuesTo` (form.cc:1851) — valeurs du contrôle
    /// « réussi » (`values` ≙ `Dlist`).
    pub fn append_values_to(&self, values: &mut Vec<Vec<u8>>, is_active_submit: bool) {
        match self.ty {
            InputType::Text | InputType::Password | InputType::Index | InputType::Hidden => {
                if let Some(res) = self.get_resource()
                    && let Some(e) = res.as_entry_resource()
                {
                    values.push(e.get_text().into_bytes());
                }
            }
            InputType::Textarea => {
                if let Some(res) = self.get_resource()
                    && let Some(t) = res.as_multi_line_text_resource()
                {
                    values.push(t.get_text().into_bytes());
                }
            }
            InputType::Checkbox | InputType::Radio => {
                if let (Some(res), Some(init)) = (self.get_resource(), self.init_str.as_deref())
                    && self.name.is_some()
                    && let Some(t) = res.as_toggle_button_resource()
                    && t.is_activated()
                {
                    values.push(init.to_vec());
                }
            }
            InputType::Submit | InputType::ButtonSubmit => {
                if is_active_submit && let Some(init) = &self.init_str {
                    values.push(init.clone());
                }
            }
            InputType::Select | InputType::SelList => {
                if let (Some(res), Some(select)) = (self.get_resource(), self.select.as_ref())
                    && let Some(sel) = res.as_selection_resource()
                {
                    select.append_values_to(values, sel);
                }
            }
            InputType::File => {
                if let Some(res) = self.get_resource()
                    && let Some(lbr) = res.as_label_button_resource()
                {
                    let filename = lbr.get_label();
                    let init = self
                        .init_str
                        .as_deref()
                        .map(|s| String::from_utf8_lossy(s).into_owned())
                        .unwrap_or_default();
                    if !filename.is_empty() && filename != init {
                        if let Some(file) = &self.file_data {
                            values.push(file.clone());
                        } else {
                            tracing::warn!("FORM file input \"{}\" not loaded.", filename);
                        }
                    }
                }
            }
            InputType::Image => {
                if is_active_submit
                    && let Some(res) = self.get_resource()
                    && let Some(cbr) = res.as_complex_button_resource()
                {
                    values.push(cbr.get_click_x().to_string().into_bytes());
                    values.push(cbr.get_click_y().to_string().into_bytes());
                }
            }
            _ => {}
        }
    }

    /// `reset` (form.cc:1946) — valeur initiale par type.
    pub fn reset(&self) {
        let Some(res) = self.get_resource() else {
            return;
        };
        match self.ty {
            InputType::Text | InputType::Password | InputType::Index | InputType::Hidden => {
                if let Some(e) = res.as_entry_resource() {
                    let init = self
                        .init_str
                        .as_deref()
                        .map(|s| String::from_utf8_lossy(s).into_owned())
                        .unwrap_or_default();
                    e.set_text(&init);
                }
            }
            InputType::Checkbox | InputType::Radio => {
                if let Some(t) = res.as_toggle_button_resource() {
                    t.set_activated(self.init_val);
                }
            }
            InputType::Select | InputType::SelList => {
                if let (Some(select), Some(sel)) =
                    (self.select.as_ref(), res.as_selection_resource())
                {
                    select.reset(sel);
                }
            }
            InputType::Textarea => {
                if self.init_str.is_some()
                    && let Some(t) = res.as_multi_line_text_resource()
                {
                    let init = self
                        .init_str
                        .as_deref()
                        .map(|s| String::from_utf8_lossy(s).into_owned())
                        .unwrap_or_default();
                    t.set_text(&init);
                }
            }
            InputType::File => {
                if let Some(lbr) = res.as_label_button_resource() {
                    let init = self
                        .init_str
                        .as_deref()
                        .map(|s| String::from_utf8_lossy(s).into_owned())
                        .unwrap_or_default();
                    lbr.set_label(&init);
                }
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------------
// DilloHtmlReceiver (form.cc:1669-1717)
// ------------------------------------------------------------------

/// `DilloHtmlReceiver` — `Weak<HtmlForm>` ≙ pointeur `form` amont.
pub struct FormReceiver {
    /// `form`.
    form: Weak<RefCell<HtmlForm>>,
}

impl FormReceiver {
    /// `getInput` helper — résout `input` depuis `resource`.
    ///
    /// La comparaison porte sur l'**adresse données** seule
    /// (`*const ()`) : `ptr::eq` sur `&dyn` compare aussi la vtable,
    /// qui n'est pas garantie unique entre codegen units — le même
    /// objet coercé à deux endroits peut porter des métadonnées
    /// différentes (amont compare le `Resource*` fin).
    fn find_input(form: &HtmlForm, resource: &dyn Resource) -> Option<Rc<RefCell<HtmlInput>>> {
        let needle = std::ptr::from_ref(resource).cast::<()>();
        form.inputs
            .iter()
            .find(|i| {
                i.borrow()
                    .get_resource()
                    .is_some_and(|r| std::ptr::from_ref(r.as_ref()).cast::<()>() == needle)
            })
            .cloned()
    }
}

impl ActivateReceiver for FormReceiver {
    /// `activate` — `eventHandler(resource, NULL)`.
    fn activate(&mut self, resource: &dyn Resource) {
        if let Some(f) = self.form.upgrade() {
            f.borrow_mut().event_handler(resource, None);
        }
    }

    /// `enter` (form.cc:1684) — action URL si contrôle submit-capable.
    fn enter(&mut self, resource: &dyn Resource) {
        let Some(f) = self.form.upgrade() else { return };
        let form = f.borrow();
        let input = Self::find_input(&form, resource);
        let mut msg: Vec<u8> = Vec::new();
        if let Some(input) = input {
            let input = input.borrow();
            let submits = matches!(
                input.ty,
                InputType::Submit | InputType::Image | InputType::ButtonSubmit | InputType::Index
            ) || ((form.shared.borrow().prefs.enterpress_forces_submit
                || form.num_entry_fields == 1)
                && matches!(input.ty, InputType::Password | InputType::Text));
            if submits {
                msg = form.action.clone().str_().to_vec();
            }
        }
        form.shared.borrow().sink.borrow_mut().set_msg(&msg);
    }

    /// `leave` — `set_msg("")`.
    fn leave(&mut self, _resource: &dyn Resource) {
        if let Some(f) = self.form.upgrade() {
            f.borrow().shared.borrow().sink.borrow_mut().set_msg(b"");
        }
    }
}

impl ClickedReceiver for FormReceiver {
    /// `clicked` — `eventHandler(resource, event)`.
    fn clicked(&mut self, resource: &dyn Resource, event: &EventButton) {
        if let Some(f) = self.form.upgrade() {
            f.borrow_mut().event_handler(resource, Some(event));
        }
    }
}

// ------------------------------------------------------------------
// DilloHtmlForm (form.cc:94-132, 1038-1616)
// ------------------------------------------------------------------

/// `DilloHtmlForm`.
pub struct HtmlForm {
    /// `this` amont sert d'id pour `a_UIcmd_form_popup` — ici
    /// l'indice dans `DilloHtml::core.forms` (stable).
    pub index: usize,
    /// `html` ≙ `HtmlShared` (sink, arena, prefs, page_url).
    pub shared: Rc<RefCell<HtmlShared>>,
    /// `showing_hiddens`.
    pub showing_hiddens: bool,
    /// `enabled`.
    enabled: bool,
    /// `method`.
    pub method: Method,
    /// `action`.
    pub action: DilloUrl,
    /// `content_type`.
    pub content_type: Enc,
    /// `submit_charset` (`accept-charset`).
    submit_charset: Option<Vec<u8>>,
    /// `inputs`.
    pub inputs: Vec<Rc<RefCell<HtmlInput>>>,
    /// `num_entry_fields`.
    pub num_entry_fields: i32,
    /// `form_receiver` — un receiver par émetteur (verbatim : le
    /// même `form_receiver` enregistré sur `activate` ET `clicked`).
    /// `None` transitoire pendant la construction 2 phases.
    receivers: Option<FormReceivers>,
}

/// Paire de receivers `activate`/`clicked` — deux `RefCell` distincts
/// partageant le même `Weak<HtmlForm>` (≙ le pointeur unique amont ;
/// le receiver est sans état propre).
pub struct FormReceivers {
    /// Receiver `ActivateReceiver`.
    activate: Rc<RefCell<dyn dillo_dw::ui::ActivateReceiver>>,
    /// Receiver `ClickedReceiver`.
    clicked: Rc<RefCell<dyn dillo_dw::ui::ClickedReceiver>>,
}

impl HtmlForm {
    /// `DilloHtmlForm()` (form.cc:1038) — le receiver référence la
    /// forme créée : construction en 2 phases (`Weak` évitant le
    /// cycle).
    pub fn new(
        method: Method,
        action: DilloUrl,
        content_type: Enc,
        charset: Option<Vec<u8>>,
        enabled: bool,
        shared: Rc<RefCell<HtmlShared>>,
    ) -> Rc<RefCell<HtmlForm>> {
        let form = Rc::new(RefCell::new(HtmlForm {
            index: 0,
            shared,
            showing_hiddens: false,
            enabled,
            method,
            action,
            content_type,
            submit_charset: charset,
            inputs: Vec::new(),
            num_entry_fields: 0,
            receivers: None,
        }));
        let weak = Rc::downgrade(&form);
        form.borrow_mut().receivers = Some(FormReceivers {
            activate: Rc::new(RefCell::new(FormReceiver { form: weak.clone() })),
            clicked: Rc::new(RefCell::new(FormReceiver { form: weak })),
        });
        form
    }

    /// `eventHandler` (form.cc:1070).
    fn event_handler(&mut self, resource: &dyn Resource, event: Option<&EventButton>) {
        tracing::trace!("DilloHtmlForm::eventHandler");
        if let Some(eb) = event
            && eb.button == 3
        {
            let shared = self.shared.borrow();
            let page = shared.page_url.clone();
            shared
                .sink
                .borrow_mut()
                .form_popup(&page, self.index, self.showing_hiddens);
            return;
        }
        let input = FormReceiver::find_input(self, resource);
        if let Some(input) = input {
            activate_input(&input, self, event);
        } else {
            tracing::info!("DilloHtmlForm::eventHandler: ERROR, input not found!");
        }
    }

    /// `submit` (form.cc:1085).
    pub fn submit(
        &self,
        active_input: Option<&Rc<RefCell<HtmlInput>>>,
        event: Option<&EventButton>,
    ) {
        {
            let shared = self.shared.borrow();
            let insecure = {
                let ps = shared.page_url.scheme.as_deref().unwrap_or(b"");
                let asc = self.action.scheme.as_deref().unwrap_or(b"");
                str_icmp(ps, b"https") == 0 && str_icmp(asc, b"https") != 0
            };
            if insecure {
                let choice = shared.sink.borrow_mut().dialog_choice(
                    "Dillo: Insecure form submission",
                    "A form on a SECURE page wants to use an \
                     INSECURE protocol to submit data.",
                    &["Continue", "Cancel"],
                );
                if choice != 1 {
                    return;
                }
            }
        }

        let Some(url) = self.build_query_url(active_input) else {
            return;
        };
        let shared = self.shared.borrow();
        if let Some(eb) = event
            && eb.button == 2
        {
            if shared.prefs.middle_click_opens_new_tab {
                let mut focus = shared.prefs.focus_new_tab;
                if eb.state == button_state::SHIFT_MASK {
                    focus = !focus;
                }
                shared.sink.borrow_mut().open_url_nt(&url, focus);
            } else {
                shared.sink.borrow_mut().open_url_nw(&url);
            }
            return;
        }
        shared.sink.borrow_mut().open_url(&url);
    }

    /// `buildQueryUrl` (form.cc:1122).
    fn build_query_url(&self, active_input: Option<&Rc<RefCell<HtmlInput>>>) -> Option<DilloUrl> {
        if !matches!(self.method, Method::Get | Method::Post) {
            tracing::info!("DilloHtmlForm::buildQueryUrl: Method unknown");
            return None;
        }

        let mut action_str = self.action.clone().str_().to_vec();
        tracing::trace!("DilloHtmlForm::buildQueryUrl: action={:?}", action_str);

        let active_submit = active_input.filter(|i| {
            matches!(
                i.borrow().ty,
                InputType::Submit | InputType::Image | InputType::ButtonSubmit
            )
        });
        let data = self.build_query_data(active_submit)?;

        if self.method == Method::Post {
            let mut new_url = url::url_new(&action_str, None)?;
            new_url.set_data(data);
            new_url.set_flags(new_url.flags | url::flags::POST | url::flags::E2E_QUERY);
            if self.content_type == Enc::Multipart {
                new_url.set_flags(new_url.flags | url::flags::MULTIPART_ENC);
            }
            Some(new_url)
        } else {
            /* remove <fragment> and <query> sections if present */
            if let Some(p) = action_str.iter().position(|&b| b == b'#') {
                action_str.truncate(p);
            }
            if let Some(p) = action_str.iter().position(|&b| b == b'?') {
                action_str.truncate(p);
            }
            let mut url_str = action_str;
            url_str.push(b'?');
            url_str.extend_from_slice(&data);
            let mut new_url = url::url_new(&url_str, None)?;
            new_url.set_flags(new_url.flags | url::flags::GET | url::flags::E2E_QUERY);
            Some(new_url)
        }
    }

    /// `buildQueryData` (form.cc:1177).
    fn build_query_data(&self, active_submit: Option<&Rc<RefCell<HtmlInput>>>) -> Option<Vec<u8>> {
        // `char_encoder` ≙ `iconv_t != -1` : un charset non-UTF-8
        // est demandé. Sans iconv, `encode_text` passe en clair et
        // le WARN verbatim est émis ici.
        let char_encoder = self
            .submit_charset
            .as_deref()
            .is_some_and(|c| str_icmp(c, b"UTF-8") != 0);
        if char_encoder {
            let cs = self.submit_charset.as_deref().unwrap_or(b"");
            tracing::warn!(
                "Cannot convert to character encoding '{}'",
                String::from_utf8_lossy(cs)
            );
        }

        let mut boundary: Option<Vec<u8>> = None;
        if self.content_type == Enc::Multipart {
            boundary = Some(self.make_multipart_boundary(active_submit));
            if boundary.is_none() {
                tracing::error!("Cannot generate multipart/form-data boundary.");
            }
        }

        let mut data: Option<Vec<u8>> = None;
        if self.content_type == Enc::Urlencoded || boundary.is_some() {
            let mut buf: Vec<u8> = Vec::with_capacity(4096);
            let boundary_str = boundary.clone().unwrap_or_default();
            for input_rc in &self.inputs {
                let mut values: Vec<Vec<u8>> = Vec::new();
                let (name0, ty, is_active_submit) = {
                    let input = input_rc.borrow();
                    (
                        input.name.clone().unwrap_or_default(),
                        input.ty,
                        active_submit.is_some_and(|a| Rc::ptr_eq(a, input_rc)),
                    )
                };
                let name = Self::encode_text(char_encoder, name0);
                input_rc
                    .borrow()
                    .append_values_to(&mut values, is_active_submit);

                let valcount = values.len();
                if valcount > 0 {
                    if ty == InputType::File {
                        if valcount > 1 {
                            tracing::warn!("multiple files per form control not supported");
                        }
                        let file = values.remove(0);
                        /* Get filename and encode it. Do not encode
                         * file contents. */
                        let input = input_rc.borrow();
                        if let Some(res) = input.get_resource()
                            && let Some(lbr) = res.as_label_button_resource()
                        {
                            let mut filename = lbr.get_label();
                            let init = input
                                .init_str
                                .as_deref()
                                .map(|s| String::from_utf8_lossy(s).into_owned())
                                .unwrap_or_default();
                            if !filename.is_empty() && filename != init {
                                if let Some(p) = filename.rfind('/') {
                                    filename = filename[p + 1..].to_string();
                                }
                                let dfilename =
                                    Self::encode_text(char_encoder, filename.into_bytes());
                                files_input_multipart_append(
                                    &mut buf,
                                    &boundary_str,
                                    &name,
                                    &file,
                                    &dfilename,
                                );
                            }
                        }
                    } else if ty == InputType::Index {
                        /* no name */
                        let val = values.remove(0);
                        let val = Self::encode_text(char_encoder, val);
                        str_urlencode_append(&mut buf, &val);
                    } else if ty == InputType::Image {
                        let x = values.remove(0);
                        let y = values.remove(0);
                        if self.content_type == Enc::Urlencoded {
                            image_input_urlencode_append(&mut buf, &name, &x, &y);
                        } else if self.content_type == Enc::Multipart {
                            image_input_multipart_append(&mut buf, &boundary_str, &name, &x, &y);
                        }
                    } else {
                        for val in values {
                            let val = Self::encode_text(char_encoder, val);
                            if self.content_type == Enc::Urlencoded {
                                input_urlencode_append(&mut buf, &name, &val);
                            } else if self.content_type == Enc::Multipart {
                                input_multipart_append(&mut buf, &boundary_str, &name, &val);
                            }
                        }
                    }
                }
            }
            if !buf.is_empty() {
                if self.content_type == Enc::Urlencoded {
                    if buf.last() == Some(&b'&') {
                        buf.truncate(buf.len() - 1);
                    }
                } else if self.content_type == Enc::Multipart {
                    buf.extend_from_slice(b"--");
                }
            }
            data = Some(buf);
        }
        data
    }

    /// `makeMultipartBoundary` (form.cc:1325) — concatène noms,
    /// fichiers et valeurs, puis 70 caractères aléatoires. (Amont
    /// remplit `DataStr` pour détecter une collision puis le jette —
    /// commentaire verbatim : probabilité < 1e-117.)
    fn make_multipart_boundary(&self, active_submit: Option<&Rc<RefCell<HtmlInput>>>) -> Vec<u8> {
        let mut values: Vec<Vec<u8>> = Vec::new();
        let char_encoder = self
            .submit_charset
            .as_deref()
            .is_some_and(|c| str_icmp(c, b"UTF-8") != 0);
        let mut data: Vec<u8> = Vec::new();

        for input_rc in &self.inputs {
            let is_active_submit = active_submit.is_some_and(|a| Rc::ptr_eq(a, input_rc));
            let (name0, ty) = {
                let input = input_rc.borrow();
                (input.name.clone(), input.ty)
            };
            input_rc
                .borrow()
                .append_values_to(&mut values, is_active_submit);

            if let Some(name) = name0 {
                let n = Self::encode_text(char_encoder, name);
                data.extend_from_slice(&n);
            }
            if ty == InputType::File {
                let input = input_rc.borrow();
                if let Some(res) = input.get_resource()
                    && let Some(lbr) = res.as_label_button_resource()
                {
                    let filename = lbr.get_label();
                    let init = input
                        .init_str
                        .as_deref()
                        .map(|s| String::from_utf8_lossy(s).into_owned())
                        .unwrap_or_default();
                    if !filename.is_empty() && filename != init {
                        let d = Self::encode_text(char_encoder, filename.into_bytes());
                        data.extend_from_slice(&d);
                    }
                }
            }
            for dstr in values.drain(..) {
                let d = if ty == InputType::File {
                    dstr
                } else {
                    Self::encode_text(char_encoder, dstr)
                };
                data.extend_from_slice(&d);
            }
        }
        let _ = data;

        generate_boundary()
    }

    /// `encodeText` (form.cc:1374) — sans `iconv`, `char_encoder`
    /// reste `-1` : passthrough verbatim (WARN émis à l'ouverture).
    fn encode_text(_encoder: bool, input: Vec<u8>) -> Vec<u8> {
        input
    }

    /// `reset` (form.cc:1590).
    pub fn reset(&self) {
        for input in &self.inputs {
            input.borrow().reset();
        }
    }

    /// `display_hiddens` (form.cc:1600).
    pub fn display_hiddens(&mut self, display: bool) {
        for input in &self.inputs {
            let input = input.borrow();
            if input.ty == InputType::Hidden
                && let Some(res) = input.get_resource()
            {
                res.set_displayed(display);
            }
        }
        self.showing_hiddens = display;
    }

    /// `setEnabled` (form.cc:1610).
    pub fn set_enabled(&self, enabled: bool) {
        for input in &self.inputs {
            input.borrow().set_enabled(enabled);
        }
    }

    /// `addInput` (form.cc:1616).
    pub fn add_input(&mut self, input: Rc<RefCell<HtmlInput>>, ty: InputType) {
        if let Some(r) = &self.receivers {
            input.borrow().connect_to(r);
        }
        input.borrow().set_enabled(self.enabled);
        self.inputs.push(input);
        if matches!(ty, InputType::Password | InputType::Text) {
            self.num_entry_fields += 1;
        }
    }

    /// `getInput` (form.cc:1632).
    pub fn get_input(&self, resource: &dyn Resource) -> Option<Rc<RefCell<HtmlInput>>> {
        FormReceiver::find_input(self, resource)
    }

    /// `getRadioInput` (form.cc:1649).
    #[must_use]
    pub fn get_radio_input(&self, name: &[u8]) -> Option<Rc<RefCell<HtmlInput>>> {
        self.inputs
            .iter()
            .find(|i| {
                let i = i.borrow();
                i.ty == InputType::Radio
                    && i.name.as_deref().is_some_and(|n| str_icmp(n, name) == 0)
            })
            .cloned()
    }
}

/// `input->activate()` — fonction libre : évite de garder un
/// `borrow_mut` sur l'input pendant `submit` (le form re-parcourt
/// `inputs` — RefCell).
fn activate_input(input_rc: &Rc<RefCell<HtmlInput>>, form: &HtmlForm, event: Option<&EventButton>) {
    let ty = input_rc.borrow().ty;
    match ty {
        InputType::File => input_rc.borrow_mut().read_file(),
        InputType::Reset | InputType::ButtonReset => form.reset(),
        InputType::Text | InputType::Password => {
            let force = form.shared.borrow().prefs.enterpress_forces_submit;
            if force || form.num_entry_fields == 1 {
                form.submit(Some(input_rc), event);
            }
        }
        InputType::Submit | InputType::ButtonSubmit | InputType::Image | InputType::Index => {
            form.submit(Some(input_rc), event);
        }
        _ => {}
    }
}

// ------------------------------------------------------------------
// Encodage des requêtes (form.cc:1285-1616)
// ------------------------------------------------------------------

/// `generate_boundary` (form.cc:1308) — 70 caractères aléatoires
/// (RFC 2046 §5.1.1 ; l'aléa n'est pas crypto, comme `rand()` amont).
pub(crate) fn generate_boundary() -> Vec<u8> {
    const SET: &[u8] = b"abcdefghijklmnopqrstuvwxyz\
ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut b = Vec::with_capacity(70);
    for _ in 0..70 {
        let c = SET[rand_index(SET.len())];
        b.push(c);
    }
    b
}

/// `rand() % n` — xorshift thread-local (≙ `rand` amont).
fn rand_index(n: usize) -> usize {
    use std::cell::Cell;
    use std::time::{SystemTime, UNIX_EPOCH};
    thread_local! {
        static SEED: Cell<u64> = Cell::new(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0x9e37_79b9_7f4a_7c15, |d| d.as_nanos() as u64),
        );
    }
    SEED.with(|s| {
        let mut x = s.get();
        if x == 0 {
            x = 0x9e37_79b9_7f4a_7c15;
        }
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        (x as usize) % n
    })
}

/// `strUrlencodeAppend` (form.cc:1440).
pub(crate) fn str_urlencode_append(dstr: &mut Vec<u8>, str_: &[u8]) {
    dstr.extend_from_slice(&url::encode_hex_str(str_));
}

/// `inputUrlencodeAppend` (form.cc:1451).
pub(crate) fn input_urlencode_append(data: &mut Vec<u8>, name: &[u8], value: &[u8]) {
    if name.is_empty() {
        return;
    }
    str_urlencode_append(data, name);
    data.push(b'=');
    str_urlencode_append(data, value);
    data.push(b'&');
}

/// `filesInputMultipartAppend` (form.cc:1464).
fn files_input_multipart_append(
    data: &mut Vec<u8>,
    boundary: &[u8],
    name: &[u8],
    file: &[u8],
    filename: &[u8],
) {
    if name.is_empty() {
        return;
    }
    let (mut ctype, _st) = misc::get_content_type_from_data(file);
    /* Heuristic: text/plain with ".htm[l]" extension -> text/html */
    let filename_str = String::from_utf8_lossy(filename).into_owned();
    let ext = filename_str
        .rfind('.')
        .map(|i| filename_str.as_bytes()[i..].to_vec());
    if let Some(ext) = ext
        && str_icmp(ctype.as_bytes(), b"text/plain") == 0
        && (str_icmp(&ext, b".html") == 0 || str_icmp(&ext, b".htm") == 0)
    {
        ctype = "text/html";
    }

    if data.is_empty() {
        data.extend_from_slice(b"--");
        data.extend_from_slice(boundary);
    }
    data.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
    data.extend_from_slice(name);
    data.extend_from_slice(b"\"; filename=\"");
    /* verbatim : les caractères les plus susceptibles de casser le
     * multipart sont neutralisés en '_'. */
    for &c in filename {
        let c = if c == b'"' || c == b'\r' || c == b'\n' {
            b'_'
        } else {
            c
        };
        data.push(c);
    }
    data.extend_from_slice(b"\"\r\nContent-Type: ");
    data.extend_from_slice(ctype.as_bytes());
    data.extend_from_slice(b"\r\n\r\n");
    data.extend_from_slice(file);
    data.extend_from_slice(b"\r\n--");
    data.extend_from_slice(boundary);
}

/// `inputMultipartAppend` (form.cc:1516).
fn input_multipart_append(data: &mut Vec<u8>, boundary: &[u8], name: &[u8], value: &[u8]) {
    if name.is_empty() {
        return;
    }
    if data.is_empty() {
        data.extend_from_slice(b"--");
        data.extend_from_slice(boundary);
    }
    data.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"");
    data.extend_from_slice(name);
    data.extend_from_slice(b"\"\r\n\r\n");
    data.extend_from_slice(value);
    data.extend_from_slice(b"\r\n--");
    data.extend_from_slice(boundary);
}

/// `imageInputUrlencodeAppend` (form.cc:1544).
fn image_input_urlencode_append(data: &mut Vec<u8>, name: &[u8], x: &[u8], y: &[u8]) {
    if name.is_empty() {
        data.extend_from_slice(b"x=");
        data.extend_from_slice(x);
        data.extend_from_slice(b"&y=");
        data.extend_from_slice(y);
        data.push(b'&');
        return;
    }
    str_urlencode_append(data, name);
    data.extend_from_slice(b".x=");
    data.extend_from_slice(x);
    data.push(b'&');
    str_urlencode_append(data, name);
    data.extend_from_slice(b".y=");
    data.extend_from_slice(y);
    data.push(b'&');
}

/// `imageInputMultipartAppend` (form.cc:1564).
fn image_input_multipart_append(
    data: &mut Vec<u8>,
    boundary: &[u8],
    name: &[u8],
    x: &[u8],
    y: &[u8],
) {
    let orig_len = name.len();
    let mut name = name.to_vec();
    if orig_len > 0 {
        name.push(b'.');
    }
    name.push(b'x');
    input_multipart_append(data, boundary, &name, x);
    name.truncate(name.len() - 1);
    name.push(b'y');
    input_multipart_append(data, boundary, &name, y);
    name.truncate(orig_len);
}

// ------------------------------------------------------------------
// Tag functions + `Html_add_input` (form.cc:237-1037)
// ------------------------------------------------------------------

/// `Html_add_input` (form.cc:277) — ajoute le contrôle au formulaire
/// courant ou à `inputs_outside_form`.
pub(crate) fn add_input(
    h: &mut DilloHtml,
    ty: InputType,
    embed: Option<WidgetId>,
    name: Option<Vec<u8>>,
    init_str: Option<Vec<u8>>,
    init_val: bool,
    owns_embed: bool,
) {
    tracing::trace!(
        "Html_add_input name={:?} init_str={:?} init_val={}",
        name,
        init_str,
        init_val
    );
    let input = Rc::new(RefCell::new(HtmlInput::new(
        ty,
        embed,
        name,
        init_str,
        init_val,
        owns_embed,
        &h.core.shared,
    )));
    if h.core.in_flags & in_flags::FORM != 0 {
        if let Some(form) = h.get_current_form() {
            form.borrow_mut().add_input(input, ty);
        }
    } else {
        h.core.inputs_outside_form.push(input.clone());
        let pending = {
            let sh = h.core.shared.borrow();
            sh.sink.borrow().num_pending_style_sheets()
        };
        if pending > 0 {
            input.borrow().set_enabled(false);
        }
    }
}

/// `Html_get_radio_input` (form.cc:297).
fn get_radio_input(h: &mut DilloHtml, name: Option<&[u8]>) -> Option<Rc<RefCell<HtmlInput>>> {
    let name = name?;
    let inputs: Vec<Rc<RefCell<HtmlInput>>> = if h.core.in_flags & in_flags::FORM != 0 {
        h.get_current_form()
            .map(|f| f.borrow().inputs.clone())
            .unwrap_or_default()
    } else {
        h.core.inputs_outside_form.clone()
    };
    inputs.into_iter().find(|i| {
        let i = i.borrow();
        i.ty == InputType::Radio && i.name.as_deref().is_some_and(|n| str_icmp(n, name) == 0)
    })
}

/// `Html_get_current_input` (form.cc:319).
pub(crate) fn get_current_input(h: &mut DilloHtml) -> Option<Rc<RefCell<HtmlInput>>> {
    if h.core.in_flags & in_flags::FORM != 0 {
        h.get_current_form()
            .and_then(|f| f.borrow().inputs.last().cloned())
    } else {
        h.core.inputs_outside_form.last().cloned()
    }
}

/// `Html_tag_open_form` (form.cc:334).
pub fn tag_open_form(h: &mut DilloHtml, tag: &[u8]) {
    /* Don't break paragraph if the form is under display:none */
    if h.should_display()
        && let (Some(dw), Some(ws)) = (h.core.dw, h.word_style())
    {
        h.layout().borrow_mut().tb_add_parbreak(dw, 9, &ws);
    }

    if h.core.in_flags & in_flags::FORM != 0 {
        h.bug_message("Nested <form>.");
        return;
    }
    h.core.in_flags |= in_flags::FORM;
    h.core.in_flags &= !in_flags::SELECT;
    h.core.in_flags &= !in_flags::OPTION;
    h.core.in_flags &= !in_flags::TEXTAREA;

    let mut method = Method::Get;
    if let Some(attrbuf) = h.get_attr(tag, b"method") {
        if str_icmp(&attrbuf, b"post") == 0 {
            method = Method::Post;
        } else if str_icmp(&attrbuf, b"get") != 0 {
            h.bug_message(&format!(
                "<form> submission method unknown: '{}'.",
                String::from_utf8_lossy(&attrbuf)
            ));
        }
    }
    let action = if let Some(attrbuf) = h.get_attr(tag, b"action") {
        h.url_new(Some(&attrbuf), None, false)
    } else {
        if h.core.doc_type != DocType::Html || h.core.doc_type_version <= 4.01 {
            h.bug_message("<form> requires action attribute.");
        }
        Some(h.core.shared.borrow().base_url.clone())
    };
    let mut content_type = Enc::Urlencoded;
    if method == Method::Post
        && let Some(attrbuf) = h.get_attr(tag, b"enctype")
        && str_icmp(&attrbuf, b"multipart/form-data") == 0
    {
        content_type = Enc::Multipart;
    }
    /* a list of acceptable charsets, separated by commas or spaces */
    let mut charset: Option<Vec<u8>> = None;
    if let Some(attrbuf) = h.get_attr(tag, b"accept-charset") {
        let mut rest: Option<&[u8]> = Some(&attrbuf);
        let mut first: Option<Vec<u8>> = None;
        while rest.is_some() && charset.is_none() {
            let curr = dlib::str_sep(&mut rest, b" ,");
            match curr {
                Some(c) if str_icmp(c, b"utf-8") == 0 => {
                    charset = Some(c.to_vec());
                }
                Some(c) if str_icmp(c, b"UNKNOWN") == 0 => {
                    /* defined to be whatever encoding the document is in */
                    charset.clone_from(&h.core.charset);
                }
                Some(c) if first.is_none() => {
                    first = Some(c.to_vec());
                }
                _ => {}
            }
        }
        if charset.is_none() {
            charset = first;
        }
    }
    if charset.is_none() {
        charset.clone_from(&h.core.charset);
    }
    if let Some(action) = action {
        h.form_new(method, action, content_type, charset);
    }
}

/// `Html_tag_close_form` (form.cc:395).
pub fn tag_close_form(h: &mut DilloHtml) {
    h.core.in_flags &= !in_flags::FORM;
    h.core.in_flags &= !in_flags::SELECT;
    h.core.in_flags &= !in_flags::OPTION;
    h.core.in_flags &= !in_flags::TEXTAREA;
}

/// `Html_input_get_size` (form.cc:407).
fn input_get_size(h: &mut DilloHtml, attrbuf: Option<&[u8]>) -> i32 {
    const MAX_SIZE: i32 = 1024;
    let mut size = 20;
    if let Some(a) = attrbuf {
        size = dlib::strtol(a, 10).0;
        if !(1..=MAX_SIZE).contains(&size) {
            let bad = size;
            size = if size < 1 { 20 } else { MAX_SIZE };
            h.bug_message(&format!("<input> size={bad}, using size={size} instead."));
        }
    }
    size
}

/// `Html_tag_open_input` (form.cc:437).
pub fn tag_open_input(h: &mut DilloHtml, tag: &[u8]) {
    let mut owns_embed = true;
    let mut embed: Option<WidgetId> = None;

    if h.core.in_flags & in_flags::SELECT != 0 {
        h.bug_message("<input> inside <select>.");
        return;
    }
    if h.core.in_flags & in_flags::BUTTON != 0 {
        h.bug_message("<input> inside <button>.");
        return;
    }

    let factory = h
        .core
        .shared
        .borrow()
        .layout
        .clone()
        .borrow_mut()
        .resource_factory();
    let Some(factory) = factory else { return };

    let value = h.get_attr_wdef(tag, b"value", None);
    let name = h.get_attr_wdef(tag, b"name", None);
    let ty = h
        .get_attr_wdef(tag, b"type", Some(&b""[..]))
        .unwrap_or_default();

    let mut init_val = false;
    let mut init_str: Option<Vec<u8>> = None;
    let mut resource: Option<Rc<dyn Resource>> = None;
    let mut inp_type = InputType::Unknown;

    if str_icmp(&ty, b"password") == 0 {
        inp_type = InputType::Password;
        let placeholder = h.get_attr_wdef(tag, b"placeholder", None);
        let attrbuf = h.get_attr(tag, b"size");
        let size = input_get_size(h, attrbuf.as_deref());
        resource = Some(
            factory.create_entry_resource(
                size,
                true,
                "",
                placeholder
                    .as_deref()
                    .map(|p| String::from_utf8_lossy(p).into_owned())
                    .as_deref()
                    .unwrap_or(""),
            ),
        );
        init_str.clone_from(&value);
    } else if str_icmp(&ty, b"checkbox") == 0 {
        inp_type = InputType::Checkbox;
        resource = Some(factory.create_check_button_resource(false));
        init_val = h.get_attr(tag, b"checked").is_some();
        init_str = Some(value.clone().unwrap_or_else(|| b"on".to_vec()));
    } else if str_icmp(&ty, b"radio") == 0 {
        inp_type = InputType::Radio;
        let input = get_radio_input(h, name.as_deref());
        let rb_r = input.and_then(|i| {
            i.borrow()
                .get_resource()
                .and_then(dillo_dw::ui::Resource::as_radio_button_resource_rc)
        });
        resource = Some(factory.create_radio_button_resource(rb_r, false));
        init_val = h.get_attr(tag, b"checked").is_some();
        init_str.clone_from(&value);
    } else if str_icmp(&ty, b"hidden") == 0 {
        inp_type = InputType::Hidden;
        init_str.clone_from(&value);
        let size = input_get_size(h, None);
        resource = Some(
            factory.create_entry_resource(
                size,
                false,
                name.as_deref()
                    .map(|n| String::from_utf8_lossy(n).into_owned())
                    .as_deref()
                    .unwrap_or(""),
                "",
            ),
        );
    } else if str_icmp(&ty, b"submit") == 0 {
        inp_type = InputType::Submit;
        let label = value.clone().unwrap_or_else(|| b"Submit".to_vec());
        init_str = Some(label.clone());
        resource = Some(factory.create_label_button_resource(&String::from_utf8_lossy(&label)));
    } else if str_icmp(&ty, b"reset") == 0 {
        inp_type = InputType::Reset;
        let label = value.clone().unwrap_or_else(|| b"Reset".to_vec());
        init_str = Some(label.clone());
        resource = Some(factory.create_label_button_resource(&String::from_utf8_lossy(&label)));
    } else if str_icmp(&ty, b"image") == 0 {
        let spam_safe = {
            let base = h.core.shared.borrow().base_url.clone();
            base.flags & url::flags::SPAM_SAFE != 0
        };
        if spam_safe || !h.should_display() {
            /* Don't request the image; make a text submit button instead */
            inp_type = InputType::Submit;
            let attrbuf = h.get_attr(tag, b"alt");
            let label = attrbuf
                .or_else(|| value.clone())
                .or_else(|| name.clone())
                .unwrap_or_else(|| b"Submit".to_vec());
            init_str = Some(label.clone());
            resource = Some(factory.create_label_button_resource(&String::from_utf8_lossy(&label)));
        } else {
            inp_type = InputType::Image;
            /* use a dw_image widget */
            embed = input_image(h, tag);
            owns_embed = false; /* adds it to the tree */
            init_str.clone_from(&value);
        }
    } else if str_icmp(&ty, b"file") == 0 {
        let mut valid = true;
        if h.core.in_flags & in_flags::FORM != 0
            && let Some(form) = h.get_current_form()
        {
            let form = form.borrow();
            if form.method != Method::Post {
                valid = false;
                h.bug_message("<form> with file input MUST use HTTP POST method.");
                tracing::info!("File input ignored in form not using HTTP POST method");
            } else if form.content_type != Enc::Multipart {
                valid = false;
                h.bug_message("<form> with file input MUST use multipart/form-data encoding.");
                tracing::info!("File input ignored in form not using multipart/form-data encoding");
            }
        }
        if valid {
            inp_type = InputType::File;
            init_str = Some(b"File selector".to_vec());
            resource = Some(factory.create_label_button_resource("File selector"));
        }
    } else if str_icmp(&ty, b"button") == 0 {
        inp_type = InputType::Button;
        if let Some(v) = &value {
            init_str = Some(v.clone());
            resource = Some(factory.create_label_button_resource(&String::from_utf8_lossy(v)));
        }
    } else {
        /* Text input, which also is the default */
        inp_type = InputType::Text;
        let placeholder = h.get_attr_wdef(tag, b"placeholder", None);
        let attrbuf = h.get_attr(tag, b"size");
        let size = input_get_size(h, attrbuf.as_deref());
        resource = Some(
            factory.create_entry_resource(
                size,
                false,
                "",
                placeholder
                    .as_deref()
                    .map(|p| String::from_utf8_lossy(p).into_owned())
                    .as_deref()
                    .unwrap_or(""),
            ),
        );
        init_str.clone_from(&value);
    }

    if inp_type != InputType::Unknown {
        let embed = match embed {
            Some(e) => Some(e),
            None => resource
                .as_ref()
                .map(|r| h.layout().borrow().add_embed_widget(r)),
        };
        let Some(embed_id) = embed else { return };

        if inp_type == InputType::Hidden
            && let Some(res) = &resource
        {
            res.set_displayed(false);
        }
        if matches!(inp_type, InputType::Text | InputType::Password)
            && let Some(res) = &resource
            && let Some(e) = res.as_entry_resource()
        {
            if h.get_attr(tag, b"readonly").is_some() {
                e.set_editable(false);
            }
            /* Maximum length of the text in the entry */
            if let Some(attrbuf) = h.get_attr(tag, b"maxlength") {
                let maxlen = dlib::strtol(&attrbuf, 10).0;
                e.set_max_length(maxlen);
            }
        }
        if h.prefs.show_tooltip
            && let Some(attrbuf) = h.get_attr(tag, b"title")
        {
            h.se.set_non_css_hint_str(Prop::PropertyXTooltip, CssValueType::String, &attrbuf);
        }

        /* Only add to the tree if needs to be shown. Ignore images
         * as they are already added in Html_input_image(). */
        if h.should_display() && inp_type != InputType::Image {
            if let (Some(dw), Some(style)) = (h.core.dw, h.background_style()) {
                h.layout().borrow_mut().tb_add_widget(dw, embed_id, &style);
            }
            owns_embed = false;
        }

        let init = Some(init_str.unwrap_or_default());
        add_input(
            h,
            inp_type,
            Some(embed_id),
            name,
            init,
            init_val,
            owns_embed,
        );
    }
}

/// `Html_tag_open_isindex` (form.cc:599).
pub fn tag_open_isindex(h: &mut DilloHtml, tag: &[u8]) {
    if h.core.in_flags & in_flags::FORM != 0 {
        tracing::info!("<isindex> inside <form> not handled.");
        return;
    }

    let action = match h.get_attr(tag, b"action") {
        Some(attrbuf) => h.url_new(Some(&attrbuf), None, false),
        None => Some(h.core.shared.borrow().base_url.clone()),
    };
    let Some(action) = action else { return };

    h.form_new(Method::Get, action, Enc::Urlencoded, h.core.charset.clone());
    h.core.in_flags |= in_flags::FORM;

    let factory = h
        .core
        .shared
        .borrow()
        .layout
        .clone()
        .borrow_mut()
        .resource_factory();
    let Some(factory) = factory else { return };
    let res: Rc<dyn Resource> = factory.create_entry_resource(20, false, "", "");
    let embed = h.layout().borrow().add_embed_widget(&res);

    if h.should_display() {
        if let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) {
            h.layout().borrow_mut().tb_add_parbreak(dw, 9, &ws);
            if let Some(attrbuf) = h.get_attr(tag, b"prompt") {
                h.layout()
                    .borrow_mut()
                    .tb_add_text(dw, &attrbuf, attrbuf.len(), &ws);
            }
            if let Some(style) = h.background_style() {
                h.layout().borrow_mut().tb_add_widget(dw, embed, &style);
            }
        }
        add_input(h, InputType::Index, Some(embed), None, None, false, false);
    } else {
        add_input(h, InputType::Index, Some(embed), None, None, false, true);
    }
    h.core.in_flags &= !in_flags::FORM;
}

/// `Html_tag_open_textarea` (form.cc:647).
pub fn tag_open_textarea(h: &mut DilloHtml, _tag: &[u8]) {
    h.core.in_flags |= in_flags::TEXTAREA;
}

/// `Html_tag_content_textarea` (form.cc:655).
pub fn tag_content_textarea(h: &mut DilloHtml, tag: &[u8]) {
    const MAX_COLS: i32 = 1024;
    const MAX_ROWS: i32 = 10000;

    h.stash_init();
    h.s_top_mut().parse_mode = ParseMode::Verbatim;

    let mut cols = h.get_attr(tag, b"cols").map_or_else(
        || {
            if h.core.doc_type != DocType::Html || h.core.doc_type_version <= 4.01 {
                h.bug_message("<textarea> requires cols attribute.");
            }
            20
        },
        |attrbuf| dlib::strtol(&attrbuf, 10).0,
    );
    if !(1..=MAX_COLS).contains(&cols) {
        let bad = cols;
        cols = if cols < 1 { 20 } else { MAX_COLS };
        h.bug_message(&format!(
            "<textarea> cols={bad}, using cols={cols} instead."
        ));
    }
    let mut rows = h.get_attr(tag, b"rows").map_or_else(
        || {
            if h.core.doc_type != DocType::Html || h.core.doc_type_version <= 4.01 {
                h.bug_message("<textarea> requires rows attribute.");
            }
            3
        },
        |attrbuf| dlib::strtol(&attrbuf, 10).0,
    );
    if !(1..=MAX_ROWS).contains(&rows) {
        let bad = rows;
        rows = if rows < 1 { 2 } else { MAX_ROWS };
        h.bug_message(&format!(
            "<textarea> rows={bad}, using rows={rows} instead."
        ));
    }
    let name = h.get_attr(tag, b"name");
    let placeholder = h.get_attr(tag, b"placeholder");

    /* The textarea should never call the _content hook on
     * display:none, but we still treat it as if it is possible for
     * robustness. */
    let factory = h
        .core
        .shared
        .borrow()
        .layout
        .clone()
        .borrow_mut()
        .resource_factory();
    let Some(factory) = factory else { return };
    let textres = factory.create_multi_line_text_resource(
        cols,
        rows,
        placeholder
            .as_deref()
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .as_deref()
            .unwrap_or(""),
    );

    /* Readonly or not? */
    if h.get_attr(tag, b"readonly").is_some() {
        textres.set_editable(false);
    }

    let res: Rc<dyn Resource> = textres;
    let embed = h.layout().borrow().add_embed_widget(&res);

    if h.should_display() {
        if let (Some(dw), Some(style)) = (h.core.dw, h.background_style()) {
            h.layout().borrow_mut().tb_add_widget(dw, embed, &style);
        }
        add_input(
            h,
            InputType::Textarea,
            Some(embed),
            name,
            None,
            false,
            false,
        );
    } else {
        add_input(h, InputType::Textarea, Some(embed), name, None, false, true);
    }
}

/// `Html_tag_close_textarea` (form.cc:720).
pub fn tag_close_textarea(h: &mut DilloHtml) {
    if h.core.in_flags & in_flags::TEXTAREA != 0 && h.should_display() {
        /* Remove the line ending that follows the opening tag */
        if h.core.stash.first() == Some(&b'\r') {
            h.core.stash.remove(0);
        }
        if h.core.stash.first() == Some(&b'\n') {
            h.core.stash.remove(0);
        }
        /* canonicalize line endings: '\r' → '\n' */
        let mut i = 0;
        while i < h.core.stash.len() {
            if h.core.stash[i] == b'\r' {
                if h.core.stash.get(i + 1) == Some(&b'\n') {
                    h.core.stash.remove(i);
                } else {
                    h.core.stash[i] = b'\n';
                }
            }
            i += 1;
        }

        /* The HTML3.2 spec says it can have "text and character entities". */
        let stash = h.core.stash.clone();
        let str_ = h.parse_entities(&stash);
        if let Some(input_rc) = get_current_input(h) {
            let res = input_rc.borrow().get_resource();
            input_rc.borrow_mut().init_str = Some(str_.clone());
            if let Some(res) = res
                && let Some(t) = res.as_multi_line_text_resource()
            {
                t.set_text(&String::from_utf8_lossy(&str_));
            }
        }
    }
    h.core.in_flags &= !in_flags::TEXTAREA;
}

/// `Html_tag_open_select` (form.cc:762).
pub fn tag_open_select(h: &mut DilloHtml, tag: &[u8]) {
    h.core.in_flags |= in_flags::SELECT;
    h.core.in_flags &= !in_flags::OPTION;

    let name = h.get_attr_wdef(tag, b"name", None);
    let multi = h.get_attr(tag, b"multiple").is_some();

    let mut rows = 0;
    if let Some(attrbuf) = h.get_attr(tag, b"size") {
        rows = dlib::strtol(&attrbuf, 10).0;
        if rows > 100 {
            rows = 100;
        }
    }
    if rows < 1 {
        rows = if multi { 10 } else { 1 };
    }

    let ty = if rows == 1 && !multi {
        InputType::Select
    } else {
        InputType::SelList
    };

    let factory = h
        .core
        .shared
        .borrow()
        .layout
        .clone()
        .borrow_mut()
        .resource_factory();
    let Some(factory) = factory else { return };

    let res: Rc<dyn Resource> = if ty == InputType::Select {
        factory.create_option_menu_resource()
    } else {
        let mode = if multi {
            SelectionMode::Multiple
        } else {
            SelectionMode::AtMostOne
        };
        factory.create_list_resource(mode, rows)
    };
    let embed = h.layout().borrow().add_embed_widget(&res);

    if h.should_display() {
        if h.prefs.show_tooltip
            && let Some(attrbuf) = h.get_attr(tag, b"title")
        {
            h.se.set_non_css_hint_str(Prop::PropertyXTooltip, CssValueType::String, &attrbuf);
        }
        if let (Some(dw), Some(style)) = (h.core.dw, h.background_style()) {
            h.layout().borrow_mut().tb_add_widget(dw, embed, &style);
        }
        add_input(h, ty, Some(embed), name, None, false, false);
    } else {
        add_input(h, ty, Some(embed), name, None, false, true);
    }
    h.stash_init();
}

/// `Html_tag_close_select` (form.cc:829).
pub fn tag_close_select(h: &mut DilloHtml) {
    if h.core.in_flags & in_flags::SELECT != 0 {
        if h.core.in_flags & in_flags::OPTION != 0 {
            option_finish(h);
        }
        h.core.in_flags &= !in_flags::SELECT;
        h.core.in_flags &= !in_flags::OPTION;

        if let Some(input_rc) = get_current_input(h) {
            let mut input = input_rc.borrow_mut();
            if input.ty == InputType::Select
                && let Some(select) = input.select.as_mut()
            {
                /* option menu interface requires that something be
                 * selected */
                select.ensure_selection();
            }
            /* May not have an embed or resource */
            if let (Some(res), Some(select)) = (input.get_resource(), input.select.as_ref())
                && let Some(sel) = res.as_selection_resource()
            {
                select.add_opts_to(sel);
            }
        }
    }
}

/// `Html_tag_open_optgroup` (form.cc:849).
pub fn tag_open_optgroup(h: &mut DilloHtml, tag: &[u8]) {
    if h.core.in_flags & in_flags::SELECT == 0 {
        h.bug_message("<optgroup> outside <select>.");
        return;
    }
    if h.core.in_flags & in_flags::OPTGROUP != 0 {
        h.bug_message("Nested <optgroup>.");
        return;
    }
    if h.core.in_flags & in_flags::OPTION != 0 {
        option_finish(h);
        h.core.in_flags &= !in_flags::OPTION;
    }

    h.core.in_flags |= in_flags::OPTGROUP;

    if let Some(input_rc) = get_current_input(h) {
        let mut input = input_rc.borrow_mut();
        if matches!(input.ty, InputType::Select | InputType::SelList) {
            let label = h.get_attr_wdef(tag, b"label", None);
            let enabled = h.get_attr(tag, b"disabled").is_none();
            let label = label.map_or_else(
                || {
                    h.bug_message("<optgroup> requires label attribute.");
                    String::new()
                },
                |l| String::from_utf8_lossy(&l).into_owned(),
            );
            if let Some(select) = input.select.as_mut() {
                select.add_opt(Optbase::Optgroup { label, enabled });
            }
        }
    }
}

/// `Html_tag_close_optgroup` (form.cc:882).
pub fn tag_close_optgroup(h: &mut DilloHtml) {
    if h.core.in_flags & in_flags::OPTGROUP != 0 {
        h.core.in_flags &= !in_flags::OPTGROUP;

        if h.core.in_flags & in_flags::OPTION != 0 {
            option_finish(h);
            h.core.in_flags &= !in_flags::OPTION;
        }

        if let Some(input_rc) = get_current_input(h) {
            let mut input = input_rc.borrow_mut();
            if matches!(input.ty, InputType::Select | InputType::SelList)
                && let Some(select) = input.select.as_mut()
            {
                select.add_opt(Optbase::OptgroupClose);
            }
        }
    }
}

/// `Html_tag_open_option` (form.cc:902).
pub fn tag_open_option(h: &mut DilloHtml, tag: &[u8]) {
    if h.core.in_flags & in_flags::SELECT == 0 {
        h.bug_message("<option> outside <select>.");
        return;
    }
    if h.core.in_flags & in_flags::OPTION != 0 {
        option_finish(h);
    }
    h.core.in_flags |= in_flags::OPTION;

    if let Some(input_rc) = get_current_input(h) {
        let mut input = input_rc.borrow_mut();
        if matches!(input.ty, InputType::Select | InputType::SelList) {
            let value = h
                .get_attr_wdef(tag, b"value", None)
                .map(|v| String::from_utf8_lossy(&v).into_owned());
            let label = h
                .get_attr_wdef(tag, b"label", None)
                .map(|l| String::from_utf8_lossy(&l).into_owned());
            let selected = h.get_attr(tag, b"selected").is_some();
            let enabled = h.get_attr(tag, b"disabled").is_none();
            if let Some(select) = input.select.as_mut() {
                select.add_opt(Optbase::Option {
                    value,
                    label,
                    content: None,
                    selected,
                    enabled,
                });
            }
        }
    }
    h.stash_init();
}

/// `Html_tag_close_option` (form.cc:938).
pub fn tag_close_option(h: &mut DilloHtml) {
    if h.core.in_flags & in_flags::OPTION != 0 {
        option_finish(h);
        h.core.in_flags &= !in_flags::OPTION;
    }
}

/// `Html_tag_open_button` (form.cc:951).
pub fn tag_open_button(h: &mut DilloHtml, tag: &[u8]) {
    h.core.in_flags |= in_flags::BUTTON;
    let ty = h
        .get_attr_wdef(tag, b"type", Some(&b""[..]))
        .unwrap_or_default();

    let inp_type = if str_icmp(&ty, b"button") == 0 {
        InputType::Button
    } else if str_icmp(&ty, b"reset") == 0 {
        InputType::ButtonReset
    } else if str_icmp(&ty, b"submit") == 0 || ty.is_empty() {
        /* submit button is the default */
        InputType::ButtonSubmit
    } else {
        h.bug_message(&format!(
            "<button> type unknown: '{}'.",
            String::from_utf8_lossy(&ty)
        ));
        InputType::Unknown
    };

    if inp_type != InputType::Unknown {
        if h.prefs.show_tooltip
            && let Some(attrbuf) = h.get_attr(tag, b"title")
        {
            h.se.set_non_css_hint_str(Prop::PropertyXTooltip, CssValueType::String, &attrbuf);
        }

        let value = h.get_attr_wdef(tag, b"value", None);
        let name = h.get_attr_wdef(tag, b"name", None);

        let factory = h
            .core
            .shared
            .borrow()
            .layout
            .clone()
            .borrow_mut()
            .resource_factory();
        let Some(factory) = factory else { return };

        let mut owns_embed = true;
        let embed;
        if h.should_display() {
            /* We used to have Textblock (prefs.limit_text_width, ...) here,
             * but it caused 100% CPU usage. */
            let page = h.layout().borrow_mut().tb_new(false, true);
            if let Some(style) = h.background_style() {
                h.layout().borrow_mut().widget_set_style(page, &style);
            }
            let cbr = factory.create_complex_button_resource(page, true);
            let res: Rc<dyn Resource> = cbr;
            embed = h.layout().borrow().add_embed_widget(&res);

            if let (Some(dw), Some(style)) = (h.core.dw, h.background_style()) {
                h.layout().borrow_mut().tb_add_widget(dw, embed, &style);
            }
            h.s_top_mut().textblock = Some(page);
            h.core.dw = Some(page);
            owns_embed = false;
        } else {
            /* Use simple button if not visible */
            let label = value
                .as_deref()
                .map(|v| String::from_utf8_lossy(v).into_owned())
                .unwrap_or_default();
            let res: Rc<dyn Resource> = factory.create_label_button_resource(&label);
            embed = h.layout().borrow().add_embed_widget(&res);
        }

        add_input(h, inp_type, Some(embed), name, value, false, owns_embed);
    }
}

/// `Html_tag_close_button` (form.cc:1034).
pub fn tag_close_button(h: &mut DilloHtml) {
    h.core.in_flags &= !in_flags::BUTTON;
}

/// `Html_input_image` (form.cc:2081).
fn input_image(h: &mut DilloHtml, tag: &[u8]) -> Option<WidgetId> {
    h.se.set_pseudo_link();

    /* create new image and add it to the button */
    h.common_image_attrs(tag);
    let mut button = None;
    if let Some(image) = h.image_new(tag) {
        let dwi = image.borrow().widget;
        if let Some(style) = h.background_style() {
            h.layout().borrow_mut().widget_set_style(dwi, &style);
        }
        let factory = h
            .core
            .shared
            .borrow()
            .layout
            .clone()
            .borrow_mut()
            .resource_factory();
        if let Some(factory) = factory {
            let cbr = factory.create_complex_button_resource(dwi, false);
            let res: Rc<dyn Resource> = cbr;
            let b = h.layout().borrow().add_embed_widget(&res);
            if let (Some(dw), Some(style)) = (h.core.dw, h.style()) {
                h.layout().borrow_mut().tb_add_widget(dw, b, &style);
            }
            button = Some(b);
        }
    }
    if button.is_none() {
        tracing::info!("Html_input_image: unable to create image submit.");
    }
    button
}

/// `Html_option_finish` (form.cc:2117) — verbatim ; `getCurrentOpt`
/// `None` si aucune option (garde, ≙ UB amont).
fn option_finish(h: &mut DilloHtml) {
    if let Some(input_rc) = get_current_input(h) {
        let mut input = input_rc.borrow_mut();
        if matches!(input.ty, InputType::Select | InputType::SelList)
            && let Some(select) = input.select.as_mut()
            && let Some(opt) = select.current_opt()
        {
            opt.set_content(&h.core.stash);
        }
    }
}

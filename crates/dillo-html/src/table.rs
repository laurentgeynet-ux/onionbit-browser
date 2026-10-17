//! Port of `src/table.cc` — glue HTML ↔ `dw::Table`.
//!
//! `<table>`/`<tr>`/`<td>`/`<th>`/sections : attributs legacy →
//! non-CSS hints du `StyleEngine`, modèle de bordure
//! (separate/collapse émulé dans le modèle `separate` de dw),
//! cellules `SimpleTableCell`/`AlignedTableCell`.
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_possible_truncation)] // `int`/`float` amont.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_lossless)]

use dillo_common::dlib;
use dillo_css::css::{CssLengthType, CssPropertyName as Prop, CssValueType, css_create_length};
use dillo_dw::WidgetId;
use dillo_dw::style::{BorderCollapse, BorderStyle, Style, StyleAttrs, TextAlignType, WhiteSpace};

use crate::common::{DocType, TableBorderMode, TableMode};
use crate::html::DilloHtml;
use crate::tags;

/// `dStrAsciiCasecmp` (local — `html.rs` l'a en privé).
fn str_icmp(a: &[u8], b: &[u8]) -> i32 {
    dlib::str_ascii_casecmp(a, b)
}

/// `BUG_MSG("<t%c> ...", 'd'|'h')` — `<td>` vs `<th>` via `tag[2]`.
fn cell_letter(tag: &[u8]) -> char {
    if tag.len() >= 3 && tag[2].eq_ignore_ascii_case(&b'd') {
        'd'
    } else {
        'h'
    }
}

/// `Html_table_get_border_model` (table.cc:320) — remonte la pile
/// jusqu'au `table` parent (verbatim `while (--s_idx > 0 ...)`).
fn table_get_border_model(h: &mut DilloHtml) -> TableBorderMode {
    let i_table = tags::tag_index(b"table");
    let mut s_idx = h.core.stack.len();
    while s_idx > 0 {
        s_idx -= 1;
        if s_idx == 0 || h.core.stack[s_idx].tag_idx == i_table {
            break;
        }
    }
    h.core.stack[s_idx].table_border_mode
}

/// `Html_table_set_border_model` (table.cc:335).
fn table_set_border_model(h: &mut DilloHtml, mode: TableBorderMode) {
    let i_table = tags::tag_index(b"table");
    let mut s_idx = h.core.stack.len();
    while s_idx > 0 {
        s_idx -= 1;
        if s_idx == 0 || h.core.stack[s_idx].tag_idx == i_table {
            break;
        }
    }
    if s_idx > 0 {
        h.core.stack[s_idx].table_border_mode = mode;
    }
}

/// `Html_set_collapsing_border_model` (table.cc:344) — workaround
/// du modèle collapse dans le modèle separate de dw.
fn set_collapsing_border_model(h: &mut DilloHtml, col_tb: WidgetId) {
    let Some(table) = h.s_top().table else { return };
    let Some(style) = h.style() else { return };
    let shared = h.core.shared.borrow();
    let layout = shared.layout.clone();
    let Some(table_style) = layout.borrow().widget_style(table) else {
        return;
    };

    let border_width = style.attrs.border_width.top;
    let margin_width = table_style.attrs.margin.top;

    let mut cell_attrs: StyleAttrs = style.attrs.clone();
    cell_attrs.margin.set_val(0);
    cell_attrs.border_width.left = 0;
    cell_attrs.border_width.top = 0;
    cell_attrs.border_width.right = border_width;
    cell_attrs.border_width.bottom = border_width;
    cell_attrs.h_border_spacing = 0;
    cell_attrs.v_border_spacing = 0;
    let collapse_style = Style::create(&cell_attrs);
    drop(shared);
    let layout2 = h.core.shared.borrow().layout.clone();
    layout2
        .borrow_mut()
        .widget_set_style(col_tb, &collapse_style);

    if table_get_border_model(h) != TableBorderMode::Collapse {
        table_set_border_model(h, TableBorderMode::Collapse);
        let mut table_attrs: StyleAttrs = table_style.attrs.clone();
        table_attrs.margin.set_val(margin_width);
        table_attrs.border_width.left = border_width;
        table_attrs.border_width.top = border_width;
        table_attrs.border_width.right = 0;
        table_attrs.border_width.bottom = 0;
        table_attrs.h_border_spacing = 0;
        table_attrs.v_border_spacing = 0;
        table_attrs.border_color = cell_attrs.border_color;
        table_attrs.border_style = cell_attrs.border_style;
        // CSS2 17.6.2 : pas de padding en mode collapse.
        table_attrs.padding.set_val(0);
        let table_collapse = Style::create(&table_attrs);
        let shared2 = h.core.shared.borrow();
        shared2
            .layout
            .borrow_mut()
            .widget_set_style(table, &table_collapse);
    }
}

/// `Html_set_separate_border_model` (table.cc:390) — les éléments
/// internes de table n'ont pas de marges (CSS2 17.5).
fn set_separate_border_model(h: &mut DilloHtml, col_tb: WidgetId) {
    let Some(style) = h.style() else { return };
    let mut cell_attrs: StyleAttrs = style.attrs.clone();
    cell_attrs.margin.set_val(0);
    let separate_style = Style::create(&cell_attrs);
    let shared = h.core.shared.borrow();
    shared
        .layout
        .borrow_mut()
        .widget_set_style(col_tb, &separate_style);
}

/// `Html_tag_open_table` (table.cc:48) — attributs legacy →
/// non-CSS hints table puis cellules.
pub fn tag_open_table(h: &mut DilloHtml, tag: &[u8]) {
    let mut border: i32 = -1;
    let mut cellspacing: i32 = -1;
    let mut cellpadding: i32 = -1;

    if let Some(attrbuf) = h.get_attr(tag, b"border") {
        // non-numérique ≙ 1 (verbatim `dIsdigit ? strtol : 1`).
        border = if attrbuf.first().is_some_and(|&c| dlib::is_digit(c)) {
            dlib::strtol(&attrbuf, 10).0
        } else {
            1
        };
    }
    if let Some(attrbuf) = h.get_attr(tag, b"cellspacing") {
        cellspacing = dlib::strtol(&attrbuf, 10).0;
        if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
            h.bug_message("<table> cellspacing attribute is obsolete.");
        }
    }
    if let Some(attrbuf) = h.get_attr(tag, b"cellpadding") {
        cellpadding = dlib::strtol(&attrbuf, 10).0;
        if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
            h.bug_message("<table> cellpadding attribute is obsolete.");
        }
    }

    if border != -1 {
        let l = css_create_length(border as f32, CssLengthType::Px);
        let outset = BorderStyle::Outset as i32;
        h.se.set_non_css_hint_len(Prop::BorderTopWidth, CssValueType::LengthPercentage, l);
        h.se.set_non_css_hint_len(Prop::BorderBottomWidth, CssValueType::LengthPercentage, l);
        h.se.set_non_css_hint_len(Prop::BorderLeftWidth, CssValueType::LengthPercentage, l);
        h.se.set_non_css_hint_len(Prop::BorderRightWidth, CssValueType::LengthPercentage, l);
        h.se.set_non_css_hint_int(Prop::BorderTopStyle, CssValueType::Enum, outset);
        h.se.set_non_css_hint_int(Prop::BorderBottomStyle, CssValueType::Enum, outset);
        h.se.set_non_css_hint_int(Prop::BorderLeftStyle, CssValueType::Enum, outset);
        h.se.set_non_css_hint_int(Prop::BorderRightStyle, CssValueType::Enum, outset);
    }

    if cellspacing != -1 {
        let l = css_create_length(cellspacing as f32, CssLengthType::Px);
        h.se.set_non_css_hint_len(Prop::BorderSpacing, CssValueType::LengthPercentage, l);
    }

    if let Some(attrbuf) = h.get_attr(tag, b"width") {
        let l = h.parse_length(&attrbuf);
        h.se.set_non_css_hint_len(Prop::Width, CssValueType::LengthPercentage, l);
        if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
            h.bug_message("<table> width attribute is obsolete.");
        }
    }

    if let Some(attrbuf) = h.get_attr(tag, b"align") {
        use TextAlignType as TA;
        let ta = if str_icmp(&attrbuf, b"left") == 0 {
            Some(TA::Left)
        } else if str_icmp(&attrbuf, b"right") == 0 {
            Some(TA::Right)
        } else if str_icmp(&attrbuf, b"center") == 0 {
            Some(TA::Center)
        } else {
            None
        };
        if let Some(ta) = ta {
            h.se.set_non_css_hint_int(Prop::TextAlign, CssValueType::Enum, ta as i32);
        }
        if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
            h.bug_message("<table> align attribute is obsolete.");
        }
    }

    if let Some(attrbuf) = h.get_attr(tag, b"bgcolor") {
        let bgcolor = h.color_parse(&attrbuf, -1);
        if bgcolor != -1 {
            h.se.set_non_css_hint_int(Prop::BackgroundColor, CssValueType::Color, bgcolor);
        }
        if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
            h.bug_message("<table> bgcolor attribute is obsolete.");
        }
    }

    // évalue le style maintenant (hints des cellules à suivre).
    let _ = h.style();

    // style des cellules.
    h.se.clear_non_css_hints();
    if border > 0 {
        let l = css_create_length(1.0, CssLengthType::Px);
        let inset = BorderStyle::Inset as i32;
        h.se.set_non_css_hint_len(Prop::BorderTopWidth, CssValueType::LengthPercentage, l);
        h.se.set_non_css_hint_len(Prop::BorderBottomWidth, CssValueType::LengthPercentage, l);
        h.se.set_non_css_hint_len(Prop::BorderLeftWidth, CssValueType::LengthPercentage, l);
        h.se.set_non_css_hint_len(Prop::BorderRightWidth, CssValueType::LengthPercentage, l);
        h.se.set_non_css_hint_int(Prop::BorderTopStyle, CssValueType::Enum, inset);
        h.se.set_non_css_hint_int(Prop::BorderBottomStyle, CssValueType::Enum, inset);
        h.se.set_non_css_hint_int(Prop::BorderLeftStyle, CssValueType::Enum, inset);
        h.se.set_non_css_hint_int(Prop::BorderRightStyle, CssValueType::Enum, inset);
    }

    if cellpadding != -1 {
        let l = css_create_length(cellpadding as f32, CssLengthType::Px);
        h.se.set_non_css_hint_len(Prop::PaddingTop, CssValueType::LengthPercentage, l);
        h.se.set_non_css_hint_len(Prop::PaddingBottom, CssValueType::LengthPercentage, l);
        h.se.set_non_css_hint_len(Prop::PaddingLeft, CssValueType::LengthPercentage, l);
        h.se.set_non_css_hint_len(Prop::PaddingRight, CssValueType::LengthPercentage, l);
    }
}

/// `Html_tag_content_table` (table.cc:157) — widget `dw::Table` +
/// états `S_TOP`.
pub fn tag_content_table(h: &mut DilloHtml, _tag: &[u8]) {
    let (Some(dw), Some(ws)) = (h.core.dw, h.word_style()) else {
        return;
    };
    let Some(style) = h.style() else { return };
    let shared = h.core.shared.borrow();
    let layout = shared.layout.clone();
    let ltw = h.prefs.limit_text_width;
    {
        let mut a = layout.borrow_mut();
        a.tb_add_parbreak(dw, 0, &ws);
        let table = a.table_new(ltw);
        a.tb_add_widget(dw, table, &style);
        a.tb_add_parbreak(dw, 0, &ws);
        drop(a);
        drop(shared);
        let t = h.s_top_mut();
        t.table_mode = TableMode::Top;
        t.table_border_mode = TableBorderMode::Separate;
        t.cell_text_align_set = false;
        t.table = Some(table);
    }
}

/// `Html_tag_open_tr` (table.cc:169).
pub fn tag_open_tr(h: &mut DilloHtml, tag: &[u8]) {
    h.se.inherit_non_css_hints();

    match h.s_top().table_mode {
        TableMode::None => {
            tracing::info!("Invalid HTML syntax: <tr> outside <table>");
        }
        TableMode::Top | TableMode::Tr | TableMode::Td => {
            let mut bgcolor: i32 = -1;
            if let Some(attrbuf) = h.get_attr(tag, b"bgcolor") {
                bgcolor = h.color_parse(&attrbuf, -1);
                if bgcolor != -1 {
                    h.se.set_non_css_hint_int(Prop::BackgroundColor, CssValueType::Color, bgcolor);
                }
                if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
                    h.bug_message("<tr> bgcolor attribute is obsolete.");
                }
            }

            if h.get_attr(tag, b"align").is_some() {
                h.s_top_mut().cell_text_align_set = true;
                h.tag_set_align_attr(tag);
            }

            h.se.inherit_background_color();

            if bgcolor != -1 {
                h.se.set_non_css_hint_int(Prop::BackgroundColor, CssValueType::Color, bgcolor);
            }
            h.tag_set_valign_attr(tag);
        }
    }
}

/// `Html_tag_content_tr` (table.cc:208) — `Table::addRow`.
pub fn tag_content_tr(h: &mut DilloHtml, _tag: &[u8]) {
    match h.s_top().table_mode {
        TableMode::None => {}
        TableMode::Top | TableMode::Tr | TableMode::Td => {
            let style = h.style();
            if let Some(table) = h.s_top().table {
                let shared = h.core.shared.borrow();
                shared.layout.borrow_mut().tbl_add_row(table, style);
            }
        }
    }
    h.s_top_mut().table_mode = TableMode::Tr;
}

/// `Html_tag_open_td` (table.cc:228).
pub fn tag_open_td(h: &mut DilloHtml, tag: &[u8]) {
    tag_open_table_cell(h, tag, TextAlignType::Left);
}

/// `Html_tag_content_td` (table.cc:236).
pub fn tag_content_td(h: &mut DilloHtml, tag: &[u8]) {
    tag_content_table_cell(h, tag);
}

/// `Html_tag_open_tbody` (table.cc:245) — verbatim vide.
pub fn tag_open_tbody(_h: &mut DilloHtml, _tag: &[u8]) {}

/// `Html_tag_content_tbody` (table.cc:249) — verbatim vide.
pub fn tag_content_tbody(_h: &mut DilloHtml, _tag: &[u8]) {}

/// `Html_tag_open_tfoot` (table.cc:253) — verbatim vide.
pub fn tag_open_tfoot(_h: &mut DilloHtml, _tag: &[u8]) {}

/// `Html_tag_open_thead` (table.cc:257) — verbatim vide.
pub fn tag_open_thead(_h: &mut DilloHtml, _tag: &[u8]) {}

/// `Html_tag_open_th` (table.cc:265).
pub fn tag_open_th(h: &mut DilloHtml, tag: &[u8]) {
    tag_open_table_cell(h, tag, TextAlignType::Center);
}

/// `Html_tag_content_th` (table.cc:272).
pub fn tag_content_th(h: &mut DilloHtml, tag: &[u8]) {
    tag_content_table_cell(h, tag);
}

/// `Html_tag_open_table_cell` (table.cc:399) — utilisé par
/// `<td>` et `<th>`.
fn tag_open_table_cell(h: &mut DilloHtml, tag: &[u8], text_align: TextAlignType) {
    h.se.inherit_non_css_hints();

    match h.s_top().table_mode {
        TableMode::None => {}
        // TOP : `add_cell` gère le hors-`<tr>` (verbatim).
        TableMode::Top | TableMode::Tr | TableMode::Td => {
            if !h.s_top().cell_text_align_set {
                h.se.set_non_css_hint_int(Prop::TextAlign, CssValueType::Enum, text_align as i32);
            }
            if h.get_attr(tag, b"nowrap").is_some() {
                if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
                    h.bug_message(&format!(
                        "<t{}> nowrap attribute is obsolete.",
                        cell_letter(tag)
                    ));
                }
                h.se.set_non_css_hint_int(
                    Prop::WhiteSpace,
                    CssValueType::Enum,
                    WhiteSpace::Nowrap as i32,
                );
            }

            h.tag_set_align_attr(tag);

            if let Some(attrbuf) = h.get_attr(tag, b"width") {
                let l = h.parse_length(&attrbuf);
                h.se.set_non_css_hint_len(Prop::Width, CssValueType::LengthPercentage, l);
                if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
                    h.bug_message(&format!(
                        "<t{}> width attribute is obsolete.",
                        cell_letter(tag)
                    ));
                }
            }

            h.tag_set_valign_attr(tag);

            if let Some(attrbuf) = h.get_attr(tag, b"bgcolor") {
                let bgcolor = h.color_parse(&attrbuf, -1);
                if bgcolor != -1 {
                    h.se.set_non_css_hint_int(Prop::BackgroundColor, CssValueType::Color, bgcolor);
                }
                if h.core.doc_type == DocType::Html && h.core.doc_type_version >= 5.0 {
                    h.bug_message(&format!(
                        "<t{}> bgcolor attribute is obsolete.",
                        cell_letter(tag)
                    ));
                }
            }
        }
    }
}

/// `Html_tag_content_table_cell` (table.cc:448) — cellule +
/// `colspan`/`rowspan` + modèle de bordure.
fn tag_content_table_cell(h: &mut DilloHtml, tag: &[u8]) {
    let mut colspan: i32 = 1;
    let mut rowspan: i32 = 1;

    match h.s_top().table_mode {
        TableMode::None => {
            h.bug_message(&format!("<t{}> outside <table>.", cell_letter(tag)));
            return;
        }
        TableMode::Top => {
            h.bug_message(&format!("<t{}> outside <tr>.", cell_letter(tag)));
            // fallthrough verbatim : `add_cell` gère sans crash.
            content_cell_body(h, tag, &mut colspan, &mut rowspan);
        }
        TableMode::Tr | TableMode::Td => {
            content_cell_body(h, tag, &mut colspan, &mut rowspan);
        }
    }

    h.s_top_mut().table_mode = TableMode::Td;
}

/// Corps verbatim des branches TR/TD (et fallthrough TOP) de
/// `Html_tag_content_table_cell` (table.cc:466-488).
fn content_cell_body(h: &mut DilloHtml, tag: &[u8], colspan: &mut i32, rowspan: &mut i32) {
    if let Some(attrbuf) = h.get_attr(tag, b"colspan") {
        let (v, end) = dlib::strtol(&attrbuf, 10);
        // verbatim : négatif ou aucun chiffre → 1.
        *colspan = if v < 0 || end == 0 { 1 } else { v };
    }
    if let Some(attrbuf) = h.get_attr(tag, b"rowspan") {
        *rowspan = dlib::strtol(&attrbuf, 10).0.max(1);
    }

    let Some(style) = h.style() else { return };
    let ltw = h.prefs.limit_text_width;
    let shared = h.core.shared.borrow();
    let layout = shared.layout.clone();
    let table = h.s_top().table;
    let col_tb = if style.attrs.text_align == TextAlignType::String {
        let cell_ref = table.and_then(|t| layout.borrow().tbl_get_cell_ref(t));
        layout.borrow_mut().aligned_table_cell_new(cell_ref, ltw)
    } else {
        layout.borrow_mut().simple_table_cell_new(ltw)
    };

    if style.attrs.border_collapse == BorderCollapse::Collapse {
        drop(shared);
        set_collapsing_border_model(h, col_tb);
    } else {
        drop(shared);
        set_separate_border_model(h, col_tb);
    }

    if let Some(table) = table {
        let shared = h.core.shared.borrow();
        shared
            .layout
            .borrow_mut()
            .tbl_add_cell(table, col_tb, *colspan, *rowspan);
    }
    let t = h.s_top_mut();
    t.textblock = Some(col_tb);
    h.core.dw = Some(col_tb);
}

//! Port de `dw/textblock.hh` + `dw/textblock.cc` +
//! `dw/textblock_linebreaking.cc` + `dw/textblock_iterator.cc`.
//!
//! `Textblock` amont hérite de `RegardingBorder : OOFAwareWidget` —
//! en modèle arène, le kind `WidgetKind::Textblock { tb }` embarque
//! `OofAwareData` et `node_oof*`/`is_oof_aware_kind` couvrent les deux
//! kinds (≙ `instanceOf`).
//!
//! Accès aux données : les méthodes amont manipulent librement
//! `words`/`lines`/`paragraphs` *et* l'arène (styles, widgets enfants,
//! mgrs OOF). Le port extrait `TextblockData` du nœud le temps d'un
//! appel (`with_tb`) — pendant ce temps `node_tb(_mut)` voit un
//! dummy : tous les accès doivent passer par le `&mut TextblockData`
//! du paramètre (invariant documenté à chaque méthode).
//!
//! Découpage :
//! - `mod.rs` — structures verbatim `textblock.hh` (Word/Line/
//!   Paragraph/Anchor/Hl/`BadnessAndPenalty`), constantes, ctor,
//!   accès arène, renderers (`WordImgRenderer`/`SpaceImgRenderer`)
//! - `words.rs` — `textblock.cc` : construction des mots
//!   (`addText*`/`addSpace`/`addParbreak`/`addLinebreak`/`addWidget`/
//!   `addAnchor`/`fillWord`/`fillSpace`/`setBreakOption`/
//!   `calcTextSize*`/`calcSizeOfWidgetInFlow`/`findSizeRequestReference`
//!   + `handOverBreak`/`flush`/`changeLinkColor`/`changeWordStyle`/
//!     `removeChild`) et événements (`sendSelectionEvent` + `*Impl`)
//! - `breaking.rs` — `textblock_linebreaking.cc` entier
//! - `sizing.rs` — `textblock.cc` : `sizeRequestImpl`/
//!   `getExtremesSimpl`/`sizeAllocateImpl`/`calcExtraSpaceImpl`/
//!   `getAvail*OfChild`/`mark*Change`/`resizeDrawImpl`/méthodes
//!   générateur/OOF + `find*`/`yOffset*`/`calcVerticalBorder`/
//!   `getWordExtremes`/`queueDrawRange`/`updateReference`/
//!   `widgetRefSizeChanged`/`oofSizeChanged`
//! - `draw.rs` — `textblock.cc` : `decorateText`/`drawText`/
//!   `drawWord*`/`drawSpace`/`drawLine`/`drawLevel`/
//!   `getWidgetAtPointLevel`
//! - `iter.rs` — `textblock_iterator.cc` : `TextblockIterator`

// Casts int/size_t verbatim amont (tailles dw < INT_MAX).
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::similar_names)]
#![allow(clippy::bool_to_int_with_if)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::layout::Layout;
use crate::oof::{OofAwareData, NUM_OOFM};
use crate::tools::SizeParams;
use crate::types::{Allocation, Content, Requisition, HIGHLIGHT_NUM_LAYERS};
use crate::widget::WidgetKind;
use crate::WidgetId;

pub mod breaking;
pub mod draw;
pub mod iter;
pub mod listitem;
pub mod sizing;
pub mod words;

// ------------------------------------------------------------------
// Constantes statiques (verbatim textblock.cc:126-174).
// ------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct DivChar {
    /// `const char *s`.
    pub s: &'static [u8],
    pub char_removed: bool,
    pub can_be_hyphenated: bool,
    pub unbreakable_for_min_width: bool,
    /// -1 = pas de casse à gauche/à droite.
    pub penalty_index_left: i32,
    pub penalty_index_right: i32,
}

pub const NUM_DIV_CHARS: usize = 4;

/// `divChars` (textblock.cc:126-136).
pub const DIV_CHARS: [DivChar; NUM_DIV_CHARS] = [
    // soft hyphen (U+00AD)
    DivChar {
        s: b"\xc2\xad",
        char_removed: true,
        can_be_hyphenated: false,
        unbreakable_for_min_width: true,
        penalty_index_left: PENALTY_HYPHEN as i32,
        penalty_index_right: -1,
    },
    // hyphen-minus : mêmes pénalités que les césures auto/soft
    DivChar {
        s: b"-",
        char_removed: false,
        can_be_hyphenated: true,
        unbreakable_for_min_width: true,
        penalty_index_left: -1,
        penalty_index_right: PENALTY_HYPHEN as i32,
    },
    // hyphen inconditionnel (U+2010) : identique au hyphen-minus
    DivChar {
        s: b"\xe2\x80\x90",
        char_removed: false,
        can_be_hyphenated: true,
        unbreakable_for_min_width: true,
        penalty_index_left: -1,
        penalty_index_right: PENALTY_HYPHEN as i32,
    },
    // em dash (U+2014) : césure des deux côtés
    DivChar {
        s: b"\xe2\x80\x94",
        char_removed: false,
        can_be_hyphenated: true,
        unbreakable_for_min_width: false,
        penalty_index_left: PENALTY_EM_DASH_LEFT as i32,
        penalty_index_right: PENALTY_EM_DASH_RIGHT as i32,
    },
];

/// `hyphenDrawChar` (textblock.cc:174).
pub const HYPHEN_DRAW_CHAR: &[u8] = b"-";

thread_local! {
    /// `static int Textblock::penalties[PENALTY_NUM][2]`
    /// (textblock.cc:154-162) — ×100, fixés par prefs.
    static PENALTIES: Cell<[[i32; 2]; PENALTY_NUM]> =
        const { Cell::new([[100, 800], [800, 800], [100, 800]]) };
    /// `static int Textblock::stretchabilityFactor`
    /// (textblock.cc:164).
    static STRETCHABILITY_FACTOR: Cell<i32> = const { Cell::new(100) };
    /// `static Tooltip *hoverTooltip` (textblock.cc:38) — verbatim :
    /// tooltip sous le pointeur, **partagé** entre tous les
    /// textblocks (amont utilise une static — le motion d'un
    /// textblock à l'autre s'appuie dessus) ; `thread_local` ≙
    /// global, dillo étant mono-thread. NULLé dans `~Textblock`.
    pub(crate) static HOVER_TOOLTIP: RefCell<Option<Rc<crate::style::Tooltip>>> =
        const { RefCell::new(None) };
}

/// `setPenaltyHyphen` (textblock.cc:176).
pub fn set_penalty_hyphen(penalty_hyphen: i32) {
    PENALTIES.with(|p| p.get()[PENALTY_HYPHEN][0] = penalty_hyphen);
}
/// `setPenaltyHyphen2` (textblock.cc:181).
pub fn set_penalty_hyphen2(penalty_hyphen2: i32) {
    PENALTIES.with(|p| p.get()[PENALTY_HYPHEN][1] = penalty_hyphen2);
}
/// `setPenaltyEmDashLeft` (textblock.cc:186).
pub fn set_penalty_em_dash_left(penalty_left_em_dash: i32) {
    PENALTIES.with(|p| {
        let mut v = p.get();
        v[PENALTY_EM_DASH_LEFT][0] = penalty_left_em_dash;
        v[PENALTY_EM_DASH_LEFT][1] = penalty_left_em_dash;
        p.set(v);
    });
}
/// `setPenaltyEmDashRight` (textblock.cc:192).
pub fn set_penalty_em_dash_right(penalty_right_em_dash: i32) {
    PENALTIES.with(|p| p.get()[PENALTY_EM_DASH_RIGHT][0] = penalty_right_em_dash);
}
/// `setPenaltyEmDashRight2` (textblock.cc:197).
pub fn set_penalty_em_dash_right2(penalty_right_em_dash2: i32) {
    PENALTIES.with(|p| p.get()[PENALTY_EM_DASH_RIGHT][1] = penalty_right_em_dash2);
}
/// `setStretchabilityFactor` (textblock.cc:202).
pub fn set_stretchability_factor(factor: i32) {
    STRETCHABILITY_FACTOR.set(factor);
}
pub(crate) fn penalty(index: usize, i: usize) -> i32 {
    PENALTIES.with(|p| p.get()[index][i])
}
pub(crate) fn stretchability_factor() -> i32 {
    STRETCHABILITY_FACTOR.get()
}

// ------------------------------------------------------------------
// `BadnessAndPenalty` (textblock.hh:222-281 +
//  textblock_linebreaking.cc:37-193).
// ------------------------------------------------------------------

/// `PENALTY_FORCE_BREAK`/`PENALTY_PROHIBIT_BREAK` (`INT_MIN/MAX`).
pub const PENALTY_FORCE_BREAK: i32 = i32::MIN;
pub const PENALTY_PROHIBIT_BREAK: i32 = i32::MAX;

pub const PENALTY_HYPHEN: usize = 0;
pub const PENALTY_EM_DASH_LEFT: usize = 1;
pub const PENALTY_EM_DASH_RIGHT: usize = 2;
pub const PENALTY_NUM: usize = 3;

/// `badnessState` (textblock.hh:225).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BadnessState {
    /// `NOT_STRETCHABLE`.
    NotStretchable,
    /// `QUITE_LOOSE`.
    QuiteLoose,
    /// `BADNESS_VALUE`.
    Value,
    /// `TOO_TIGHT`.
    TooTight,
}

/// « Niveaux d'infini » (textblock.hh:246-258) — `INF_PENALTIES`
/// dernier : une pénalité infinie interdit toute césure (texte
/// pré-formaté).
const INF_VALUE: i32 = 0;
const INF_LARGE: i32 = 1;
const INF_NOT_STRETCHABLE: i32 = 2;
const INF_TOO_TIGHT: i32 = 3;
const INF_PENALTIES: i32 = 4;
const INF_MAX: i32 = INF_PENALTIES;

/// `Textblock::BadnessAndPenalty` — arithmétique entière verbatim.
#[derive(Clone, Copy, Debug)]
pub struct BadnessAndPenalty {
    badness_state: BadnessState,
    /// `ratio` — défini seulement quand `badness` l'est.
    ratio: i32,
    badness: i32,
    penalty: [i32; 2],
}

impl BadnessAndPenalty {
    /// `badnessValue (infLevel)` (linebreaking:37-56).
    fn badness_value(&self, inf_level: i32) -> i32 {
        match self.badness_state {
            BadnessState::NotStretchable => {
                if inf_level == INF_NOT_STRETCHABLE {
                    1
                } else {
                    0
                }
            }
            BadnessState::QuiteLoose => {
                if inf_level == INF_LARGE {
                    1
                } else {
                    0
                }
            }
            BadnessState::Value => {
                if inf_level == INF_VALUE {
                    self.badness
                } else {
                    0
                }
            }
            BadnessState::TooTight => {
                if inf_level == INF_TOO_TIGHT {
                    1
                } else {
                    0
                }
            }
        }
    }

    /// `penaltyValue (index, infLevel)` (linebreaking:58-66).
    fn penalty_value(&self, index: usize, inf_level: i32) -> i32 {
        if self.penalty[index] == i32::MIN {
            if inf_level == INF_PENALTIES {
                -1
            } else {
                0
            }
        } else if self.penalty[index] == i32::MAX {
            if inf_level == INF_PENALTIES {
                1
            } else {
                0
            }
        } else if inf_level == INF_VALUE {
            self.penalty[index]
        } else {
            0
        }
    }

    /// `calcBadness` (linebreaking:68-110) — ratio entier ×100,
    /// badness = ratio³ (signe préservé).
    #[allow(clippy::comparison_chain)] // verbatim amont.
    pub(crate) fn calc_badness(
        &mut self,
        total_width: i32,
        ideal_width: i32,
        total_stretchability: i32,
        total_shrinkability: i32,
    ) {
        self.ratio = 0;

        if total_width == ideal_width {
            self.badness_state = BadnessState::Value;
            self.badness = 0;
        } else if total_width < ideal_width {
            if total_stretchability == 0 {
                self.badness_state = BadnessState::NotStretchable;
            } else {
                self.ratio = 100 * (ideal_width - total_width) / total_stretchability;
                if self.ratio > 1024 {
                    self.badness_state = BadnessState::QuiteLoose;
                } else {
                    self.badness_state = BadnessState::Value;
                    self.badness = self.ratio * self.ratio * self.ratio;
                }
            }
        } else {
            // totalWidth > idealWidth
            if total_shrinkability == 0 {
                self.badness_state = BadnessState::TooTight;
            } else {
                // ratio négatif ici
                self.ratio = 100 * (ideal_width - total_width) / total_shrinkability;
                if self.ratio <= -100 {
                    self.badness_state = BadnessState::TooTight;
                } else {
                    self.badness_state = BadnessState::Value;
                    self.badness = -self.ratio * self.ratio * self.ratio;
                }
            }
        }
    }

    /// `setPenalty (penalty)` inline → `setPenalties (p, p)`.
    pub(crate) fn set_penalty(&mut self, p: i32) {
        self.set_penalties(p, p);
    }

    /// `setPenalties` (linebreaking:130-135) — ×100 → ×100³/100
    /// via `setSinglePenalty`.
    pub(crate) fn set_penalties(&mut self, penalty1: i32, penalty2: i32) {
        self.set_single_penalty(0, penalty1);
        self.set_single_penalty(1, penalty2);
    }

    /// `setSinglePenalty` (linebreaking:137-149) — facteur
    /// `100*100*100/100` ; ±∞ conservés.
    fn set_single_penalty(&mut self, index: usize, p: i32) {
        if p == i32::MAX || p == i32::MIN {
            self.penalty[index] = p;
        } else {
            self.penalty[index] = p * (100 * 100 * 100 / 100);
        }
    }

    /// `getPenalty (i)` — accès debug.
    #[must_use]
    pub fn get_penalty(&self, i: usize) -> i32 {
        self.penalty[i]
    }

    /// `lineLoose` (linebreaking:151-156).
    fn line_loose(&self) -> bool {
        self.badness_state == BadnessState::NotStretchable
            || self.badness_state == BadnessState::QuiteLoose
            || (self.badness_state == BadnessState::Value && self.ratio > 0)
    }

    /// `lineTight` (linebreaking:158-162).
    fn line_tight(&self) -> bool {
        self.badness_state == BadnessState::TooTight
            || (self.badness_state == BadnessState::Value && self.ratio < 0)
    }

    /// `lineTooTight` (linebreaking:164-167).
    pub(crate) fn line_too_tight(&self) -> bool {
        self.badness_state == BadnessState::TooTight
    }

    /// `lineMustBeBroken` (linebreaking:170-173).
    pub(crate) fn line_must_be_broken(&self, penalty_index: usize) -> bool {
        self.penalty[penalty_index] == PENALTY_FORCE_BREAK
    }

    /// `lineCanBeBroken` (linebreaking:175-178).
    pub(crate) fn line_can_be_broken(&self, penalty_index: usize) -> bool {
        self.penalty[penalty_index] != PENALTY_PROHIBIT_BREAK
    }

    /// `compareTo (penaltyIndex, other)` (linebreaking:180-193) —
    /// niveaux d'infini décroissants.
    pub(crate) fn compare_to(&self, penalty_index: usize, other: &Self) -> i32 {
        for l in (0..=INF_MAX).rev() {
            let this_value = self.badness_value(l) + self.penalty_value(penalty_index, l);
            let other_value = other.badness_value(l) + other.penalty_value(penalty_index, l);
            if this_value != other_value {
                return this_value - other_value;
            }
        }
        0
    }

    /// `lineLoose`/`lineTight` usages internes (breaking.rs).
    pub(crate) fn is_loose(&self) -> bool {
        self.line_loose()
    }
    /// `lineTight` usage interne.
    pub(crate) fn is_tight(&self) -> bool {
        self.line_tight()
    }
}

impl Default for BadnessAndPenalty {
    /// État neutre : pénalités `PROHIBIT` (≙ `fillWord` amont).
    fn default() -> Self {
        let mut r = Self {
            badness_state: BadnessState::Value,
            ratio: 0,
            badness: 0,
            penalty: [0; 2],
        };
        r.set_penalty(PENALTY_PROHIBIT_BREAK);
        r
    }
}

// ------------------------------------------------------------------
// Flags Word (verbatim textblock.hh:429-460).
// ------------------------------------------------------------------

/// `CAN_BE_HYPHENATED`.
pub const WORD_CAN_BE_HYPHENATED: i16 = 1 << 0;
/// `DIV_CHAR_AT_EOL`.
pub const WORD_DIV_CHAR_AT_EOL: i16 = 1 << 1;
/// `PERM_DIV_CHAR`.
pub const WORD_PERM_DIV_CHAR: i16 = 1 << 2;
/// `DRAW_AS_ONE_TEXT`.
pub const WORD_DRAW_AS_ONE_TEXT: i16 = 1 << 3;
/// `UNBREAKABLE_FOR_MIN_WIDTH`.
pub const WORD_UNBREAKABLE_FOR_MIN_WIDTH: i16 = 1 << 4;
/// `WORD_START`.
pub const WORD_START: i16 = 1 << 5;
/// `WORD_END`.
pub const WORD_END: i16 = 1 << 6;
/// `TOPLEFT_OF_LINE`.
pub const WORD_TOPLEFT_OF_LINE: i16 = 1 << 7;

// ------------------------------------------------------------------
// Word / Line / Paragraph / Anchor / Hl (textblock.hh:334-526).
// ------------------------------------------------------------------

/// `Textblock::Word` (textblock.hh:427-500).
#[derive(Clone)]
pub struct Word {
    /// `Requisition size`.
    pub size: Requisition,
    /// `short origSpace` — de la fonte, fixé par `addSpace`.
    pub orig_space: i16,
    /// `short effSpace` — fixé par `wordWrap`.
    pub eff_space: i16,
    /// `short hyphenWidth`.
    pub hyphen_width: i16,
    /// `short flags`.
    pub flags: i16,
    /// `Content content`.
    pub content: Content,
    /// `int totalWidth` — cumul depuis le début de la ligne.
    pub total_width: i32,
    /// `int maxAscent`, `maxDescent`.
    pub max_ascent: i32,
    pub max_descent: i32,
    /// `int totalSpaceStretchability` — inclut tout *avant* ce mot.
    pub total_space_stretchability: i32,
    /// `int totalSpaceShrinkability`.
    pub total_space_shrinkability: i32,
    /// `BadnessAndPenalty badnessAndPenalty`.
    pub badness_and_penalty: BadnessAndPenalty,
    /// `Style *style`.
    pub style: Rc<crate::style::Style>,
    /// `Style *spaceStyle`.
    pub space_style: Rc<crate::style::Style>,
    /// `WordImgRenderer *wordImgRenderer` — typé
    /// `dyn ImgRenderer` (enregistrement dans `ImgRendererDist` —
    /// modèle D : `RefCell` non-covariant) ; `setData` via
    /// `as_any_mut().downcast_mut`.
    pub word_img_renderer: Option<Rc<RefCell<dyn crate::imgrenderer::ImgRenderer>>>,
    /// `SpaceImgRenderer *spaceImgRenderer`.
    pub space_img_renderer: Option<Rc<RefCell<dyn crate::imgrenderer::ImgRenderer>>>,
}

/// `Textblock::Paragraph` (textblock.hh:334-366).
#[derive(Clone)]
pub struct Paragraph {
    /// `int firstWord`, `lastWord`.
    pub first_word: i32,
    pub last_word: i32,
    /// Somme des minima depuis la dernière césure possible.
    pub par_min: i32,
    pub par_min_intrinsic: i32,
    pub par_adjustment_width: i32,
    /// Somme des maxima du paragraphe.
    pub par_max: i32,
    pub par_max_intrinsic: i32,
    /// Maxima cumulés (ce paragraphe inclus).
    pub max_par_min: i32,
    pub max_par_min_intrinsic: i32,
    pub max_par_adjustment_width: i32,
    pub max_par_max: i32,
    pub max_par_max_intrinsic: i32,
}

/// `Textblock::Line` (textblock.hh:368-425).
#[derive(Clone)]
pub struct Line {
    /// `int firstWord`, `lastWord` — `lastWord < firstWord` = ligne
    /// vide.
    pub first_word: i32,
    pub last_word: i32,
    /// `int top` — relatif au haut de la 1re ligne (`lines[0].top == 0`).
    pub top: i32,
    /// `int marginAscent`/`marginDescent` (max marges comprises).
    pub margin_ascent: i32,
    pub margin_descent: i32,
    /// `int borderAscent`/`borderDescent` (sans marge).
    pub border_ascent: i32,
    pub border_descent: i32,
    /// `int contentAscent`/`contentDescent` (deprecated amont).
    pub content_ascent: i32,
    pub content_descent: i32,
    /// `int breakSpace` — espace sous cette ligne.
    pub break_space: i32,
    /// `int textOffset` — position horizontale du 1er mot (coords widget).
    pub text_offset: i32,
    /// `int maxLineWidth`.
    pub max_line_width: i32,
    /// `int lastOofRefPositionedBeforeThisLine` (-1 = aucun).
    pub last_oof_ref_positioned_before_this_line: i32,
    /// `int leftOffset`, `rightOffset`.
    pub left_offset: i32,
    pub right_offset: i32,
    /// `enum { LEFT, RIGHT, CENTER } alignment`.
    pub alignment: LineAlignment,
}

/// `Line::{LEFT,RIGHT,CENTER}` (char amont).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i8)]
pub enum LineAlignment {
    /// `LEFT`.
    Left = 0,
    /// `RIGHT`.
    Right = 1,
    /// `CENTER`.
    Center = 2,
}

impl Line {
    /// `totalHeight (marginNextLine)` inline (textblock.hh:401-405) —
    /// collapse des marges de lignes adjacentes.
    fn total_height(&self, margin_next_line: i32) -> i32 {
        self.border_ascent
            + self.border_descent
            + (self.margin_descent - self.border_descent)
                .max(margin_next_line)
                .max(self.break_space)
    }
}

/// `Textblock::Anchor` (textblock.hh:502-506) — `name` est la
/// copie rendue par `Widget::addAnchor`.
#[derive(Clone)]
pub struct Anchor {
    /// `char *name` (copie possédée).
    pub name: Vec<u8>,
    /// `int wordIndex`.
    pub word_index: i32,
}

/// `struct { int index, nChar; }` (textblock.hh:614-615).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hl {
    /// `index`.
    pub index: i32,
    /// `nChar`.
    pub n_char: i32,
}

// ------------------------------------------------------------------
// TextblockData — état verbatim de la classe.
// ------------------------------------------------------------------

/// Données de `WidgetKind::Textblock` — champs verbatim
/// `textblock.hh` (privés + protected).
#[allow(clippy::struct_excessive_bools)] // bools verbatim amont.
pub struct TextblockData {
    /// La partie `OOFAwareWidget` (héritée amont).
    pub oof: OofAwareData,
    /// `bool hasListitemValue`.
    pub has_listitem_value: bool,
    /// `int leftInnerPadding`.
    pub left_inner_padding: i32,
    /// `int line1Offset` — offset additionnel de la 1re ligne
    /// (signe quelconque).
    pub line1_offset: i32,
    /// `int line1OffsetEff` — valeur « effective » (≙ offset
    /// ignoré quand `ignoreLine1OffsetSometimes`).
    pub line1_offset_eff: i32,
    /// `bool ignoreLine1OffsetSometimes` (hack `DwTableCell`).
    pub ignore_line1_offset_sometimes: bool,
    /// État `AlignedTextblock` (`list`/`listPos`,
    /// alignedtextblock.hh:38-39) — `Some` ≙ le nœud est un
    /// `ListItem` (`AlignedTextblock` est abstraite amont :
    /// `getValue`/`setMaxValue` purs ; `usesMaxGeneratorWidth`
    /// retourne alors `true` — listitem.cc:42-44).
    pub aligned: Option<listitem::AlignedData>,
    /// Sous-classe cellule de table — `Some` ≙ le nœud est un
    /// `SimpleTableCell` ou `AlignedTableCell` (sous-classes
    /// `Textblock` amont ; les virtuels deviennent les branches
    /// `cell.is_some()` des méthodes — tablecell.cc).
    pub cell: Option<crate::table::TableCellKind>,
    /// `bool mustQueueResize`.
    pub must_queue_resize: bool,
    /// `bool limitTextWidth` (prefs).
    pub limit_text_width: bool,
    /// `bool treatAsInline`.
    pub treat_as_inline: bool,
    /// `int redrawY`.
    pub redraw_y: i32,
    /// `int lastWordDrawn`.
    pub last_word_drawn: i32,
    /// `SizeParams sizeRequestParams`.
    pub size_request_params: SizeParams,
    /// `int lineBreakWidth` — valeur de `getAvailWidth` (-1 =
    /// indéfini).
    pub line_break_width: i32,
    /// `int wrapRefLines`, `wrapRefParagraphs` — indices de
    /// *lignes* 0-based (-1 = pas de rewrap).
    pub wrap_ref_lines: i32,
    pub wrap_ref_paragraphs: i32,
    /// `int wrapRefLinesFCX`, `wrapRefLinesFCY` — position du
    /// container de floats au dernier wrap (-1 = indéfini).
    pub wrap_ref_lines_fcx: i32,
    pub wrap_ref_lines_fcy: i32,
    /// `bool newLineHasFloatLeft`/`Right`, bords et hauteurs de
    /// float de la ligne en construction (remplis par
    /// `calcBorders`).
    pub new_line_has_float_left: bool,
    pub new_line_has_float_right: bool,
    pub new_line_left_border: i32,
    pub new_line_right_border: i32,
    pub new_line_left_float_height: i32,
    pub new_line_right_float_height: i32,
    /// `int newLineAscent`, `newLineDescent` — max des
    /// ascents/descents depuis la fin de la dernière ligne.
    pub new_line_ascent: i32,
    pub new_line_descent: i32,
    /// `SimpleVector<Line> *lines`.
    pub lines: Vec<Line>,
    /// `SimpleVector<Paragraph> *paragraphs`.
    pub paragraphs: Vec<Paragraph>,
    /// `int nonTemporaryLines`.
    pub non_temporary_lines: i32,
    /// `NotSoSimpleVector<Word> *words`.
    pub words: Vec<Word>,
    /// `SimpleVector<Anchor> *anchors`.
    pub anchors: Vec<Anchor>,
    /// `Hl hlStart[2]`, `hlEnd[2]` —
    /// `hlStart.index > hlEnd.index` = pas de surbrillance.
    pub hl_start: [Hl; HIGHLIGHT_NUM_LAYERS],
    pub hl_end: [Hl; HIGHLIGHT_NUM_LAYERS],
    /// `int hoverLink` — lien sous le pointeur.
    pub hover_link: i32,
    /// `int numSizeReferences`.
    pub num_size_references: i32,
    /// `Widget *sizeReferences[NUM_OOFM]`.
    pub size_references: [Option<WidgetId>; NUM_OOFM],
}

impl std::fmt::Debug for TextblockData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextblockData")
            .field("words", &self.words.len())
            .field("lines", &self.lines.len())
            .field("paragraphs", &self.paragraphs.len())
            .finish_non_exhaustive()
    }
}

impl TextblockData {
    /// `Textblock (bool limitTextWidth, bool treatAsInline)`
    /// (textblock.cc:207-276). `setButtonSensitive (true)` est fait
    /// par le créateur du nœud.
    #[must_use]
    pub fn new(limit_text_width: bool, treat_as_inline: bool) -> Self {
        TextblockData {
            oof: OofAwareData::new(),
            has_listitem_value: false,
            left_inner_padding: 0,
            line1_offset: 0,
            line1_offset_eff: 0,
            ignore_line1_offset_sometimes: false,
            aligned: None,
            cell: None,
            must_queue_resize: false,
            limit_text_width,
            treat_as_inline,
            redraw_y: 0,
            last_word_drawn: -1,
            size_request_params: SizeParams::default(),
            line_break_width: -1,
            wrap_ref_lines: -1,
            wrap_ref_paragraphs: -1,
            wrap_ref_lines_fcx: -1,
            wrap_ref_lines_fcy: -1,
            new_line_has_float_left: false,
            new_line_has_float_right: false,
            new_line_left_border: 0,
            new_line_right_border: 0,
            new_line_left_float_height: 0,
            new_line_right_float_height: 0,
            new_line_ascent: 0,
            new_line_descent: 0,
            lines: Vec::new(),
            paragraphs: Vec::new(),
            non_temporary_lines: 0,
            words: Vec::new(),
            anchors: Vec::new(),
            // `hlStart.index > hlEnd.index` = rien de surligné.
            hl_start: [Hl {
                index: 1,
                n_char: 0,
            }; HIGHLIGHT_NUM_LAYERS],
            hl_end: [Hl {
                index: 0,
                n_char: 0,
            }; HIGHLIGHT_NUM_LAYERS],
            hover_link: -1,
            num_size_references: 0,
            size_references: [None; NUM_OOFM],
        }
        // `initNewLine()` est appelé par `tb_init` (besoin de
        // `&mut Layout` pour `calcBorders`).
    }
}

// ------------------------------------------------------------------
// Accès arène — `node_tb`, `with_tb`, predicats.
// ------------------------------------------------------------------

impl Layout {
    /// Lecture des données textblock (`None` si kind différent).
    /// Pendant `with_tb`, le nœud contient un dummy — ne pas
    /// utiliser (invariant du modèle).
    pub(crate) fn node_tb<T>(&self, w: WidgetId, f: impl FnOnce(&TextblockData) -> T) -> Option<T> {
        self.node(w, |n| match &n.kind {
            WidgetKind::Textblock { tb } => Some(f(tb)),
            _ => None,
        })
    }

    /// Mutation des données textblock — même réserve que `node_tb`.
    pub(crate) fn node_tb_mut<T>(
        &self,
        w: WidgetId,
        f: impl FnOnce(&mut TextblockData) -> T,
    ) -> Option<T> {
        self.node_mut(w, |n| match &mut n.kind {
            WidgetKind::Textblock { tb } => Some(f(tb)),
            _ => None,
        })
    }

    /// Extrait `TextblockData` du nœud pendant `f` — `&mut Layout`
    /// et `&mut TextblockData` simultanés (modèle D). Pendant `f`,
    /// `node_tb*` voit le dummy : tout accès doit passer par le
    /// paramètre `tb`.
    pub(crate) fn with_tb<R>(
        &mut self,
        w: WidgetId,
        f: impl FnOnce(&mut Self, &mut TextblockData) -> R,
    ) -> Option<R> {
        let mut tb = self.node_mut(w, |n| match &mut n.kind {
            WidgetKind::Textblock { tb } => Some(std::mem::replace(
                tb.as_mut(),
                TextblockData::new(false, false),
            )),
            _ => None,
        })?;
        let r = f(self, &mut tb);
        self.node_mut(w, |n| {
            if let WidgetKind::Textblock { tb: slot } = &mut n.kind {
                **slot = tb;
            }
        });
        Some(r)
    }
}

/// `instanceOf (Textblock)` — predicat kind.
#[must_use]
pub fn is_textblock_kind(kind: &WidgetKind) -> bool {
    matches!(kind, WidgetKind::Textblock { .. })
}

// ------------------------------------------------------------------
// Renderers internes (verbatim textblock.hh:303-332 +
//  textblock.cc:49-122) — `StyleImage::ExternalWidgetImgRenderer`
//  amont ≙ trait `style::ExternalWidgetImgRenderer` + helpers
//  `external_img_renderer_*`.
// ------------------------------------------------------------------

/// `Textblock::WordImgRenderer` (textblock.hh:303-322) — tient
/// `Textblock*` amont ≙ `Weak<Arena>` + `WidgetId` (modèle D :
/// callbacks sans `&mut Layout`, draws différés).
pub struct WordImgRenderer {
    /// Arène faible (≙ `textblock`).
    arena: std::rc::Weak<RefCell<crate::layout::Arena>>,
    /// `Layout::id` émetteur (tag `pending_draws`).
    layout_id: usize,
    /// Le textblock.
    widget: WidgetId,
    /// `wordNo`.
    word_no: i32,
    /// `xWordWidget` — coords widget, fixé par `setData`.
    x_word_widget: i32,
    /// `lineNo`.
    line_no: i32,
    /// `dataSet`.
    data_set: bool,
}

impl WordImgRenderer {
    /// `WordImgRenderer (textblock, wordNo)` — `dataSet = false`.
    pub(crate) fn new(
        arena: &Rc<RefCell<crate::layout::Arena>>,
        layout_id: usize,
        widget: WidgetId,
        word_no: i32,
    ) -> Self {
        Self {
            arena: Rc::downgrade(arena),
            layout_id,
            widget,
            word_no,
            x_word_widget: 0,
            line_no: 0,
            data_set: false,
        }
    }

    /// `setData (xWordWidget, lineNo)` (textblock.cc:64-69).
    pub(crate) fn set_data(&mut self, x_word_widget: i32, line_no: i32) {
        self.data_set = true;
        self.x_word_widget = x_word_widget;
        self.line_no = line_no;
    }

    /// `getBgArea` (textblock.cc:81-90) — coords canvas :
    /// `x = alloc.x + xWordWidget`, `y = lineYOffsetCanvas (line)`,
    /// `w = word.size.width`, `h = borderAscent + borderDescent`.
    fn bg_area(&self) -> (i32, i32, i32, i32) {
        self.arena.upgrade().map_or((0, 0, 0, 0), |a| {
            let arena = a.borrow();
            let n = &arena.nodes[self.widget];
            let alloc = n.allocation;
            match &n.kind {
                WidgetKind::Textblock { tb } => {
                    let line = tb.lines.get(self.line_no as usize);
                    let word = tb.words.get(self.word_no as usize);
                    match (line, word) {
                        (Some(line), Some(word)) => (
                            alloc.x + self.x_word_widget,
                            alloc.y
                                + line.top
                                + (alloc.ascent - tb.lines.first().map_or(0, |l| l.border_ascent)),
                            word.size.width,
                            line.border_ascent + line.border_descent,
                        ),
                        _ => (0, 0, 0, 0),
                    }
                }
                _ => (0, 0, 0, 0),
            }
        })
    }

    /// `getRefArea` (textblock.cc:92-97) — padding area du
    /// textblock (commentaire `Widget::drawBox`).
    fn ref_area(&self) -> (i32, i32, i32, i32) {
        self.arena.upgrade().map_or((0, 0, 0, 0), |a| {
            a.borrow().nodes[self.widget].get_padding_area()
        })
    }

    /// `getStyle` — `words[wordNo]->style` (peut changer à
    /// `spaceStyle` pour `SpaceImgRenderer` — paramétré).
    fn style(&self, space: bool) -> Option<Rc<crate::style::Style>> {
        self.arena.upgrade().and_then(|a| {
            let arena = a.borrow();
            match &arena.nodes[self.widget].kind {
                WidgetKind::Textblock { tb } => tb.words.get(self.word_no as usize).map(|w| {
                    if space {
                        w.space_style.clone()
                    } else {
                        w.style.clone()
                    }
                }),
                _ => None,
            }
        })
    }
}

impl crate::imgrenderer::ImgRenderer for WordImgRenderer {
    /// `setBuffer` — rien (≙ `external_img_renderer_set_buffer`).
    fn set_buffer(&mut self, _b: &Rc<dyn crate::imgbuf::Imgbuf>, _resize: bool) {}
    /// `drawRow` — verbatim via helper.
    fn draw_row(&mut self, row: i32) {
        crate::style::external_img_renderer_draw_row(self, row);
    }
    /// `finish` — verbatim via helper.
    fn finish(&mut self) {
        crate::style::external_img_renderer_finish(self);
    }
    /// `fatal` — rien.
    fn fatal(&mut self) {}
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl crate::style::ExternalImgRenderer for WordImgRenderer {
    /// `readyToDraw` (textblock.cc:71-79) — `dataSet &&
    /// wasAllocated && wordNo < words.size && lineNo < lines.size`.
    fn ready_to_draw(&self) -> bool {
        self.data_set
            && self.arena.upgrade().is_some_and(|a| {
                let arena = a.borrow();
                arena.nodes[self.widget].was_allocated()
                    && match &arena.nodes[self.widget].kind {
                        WidgetKind::Textblock { tb } => {
                            self.word_no < tb.words.len() as i32
                                && self.line_no < tb.lines.len() as i32
                        }
                        _ => false,
                    }
            })
    }

    /// `getBgArea` (textblock.cc:81-90).
    fn get_bg_area(&self) -> (i32, i32, i32, i32) {
        self.bg_area()
    }

    /// `getRefArea` (textblock.cc:92-97).
    fn get_ref_area(&self) -> (i32, i32, i32, i32) {
        self.ref_area()
    }

    /// `getBackgroundImage` — via `getStyle` (trait
    /// `ExternalWidgetImgRenderer` par défaut amont).
    fn get_background_image(&self) -> Option<Rc<RefCell<crate::style::StyleImage>>> {
        crate::style::ExternalWidgetImgRenderer::get_widget_background_image(self)
    }
    /// `getBackgroundRepeat` — fallback amont.
    fn get_background_repeat(&self) -> crate::style::BackgroundRepeat {
        crate::style::ExternalWidgetImgRenderer::get_widget_background_repeat(self)
    }
    /// `getBackgroundAttachment` — fallback amont.
    fn get_background_attachment(&self) -> crate::style::BackgroundAttachment {
        crate::style::ExternalWidgetImgRenderer::get_widget_background_attachment(self)
    }
    /// `getBackgroundPositionX` — fallback amont.
    fn get_background_position_x(&self) -> crate::style::Length {
        crate::style::ExternalWidgetImgRenderer::get_widget_background_position_x(self)
    }
    /// `getBackgroundPositionY` — fallback amont.
    fn get_background_position_y(&self) -> crate::style::Length {
        crate::style::ExternalWidgetImgRenderer::get_widget_background_position_y(self)
    }

    /// `draw` (textblock.cc:104-108) —
    /// `queueDrawArea (x - alloc.x, y - alloc.y, w, h)` ≙
    /// `pending_draws` (modèle D, différé).
    fn draw(&mut self, x: i32, y: i32, width: i32, height: i32) {
        if let Some(a) = self.arena.upgrade() {
            let alloc = a.borrow().nodes[self.widget].allocation;
            a.borrow_mut().pending_draws.push((
                self.layout_id,
                Some(self.widget),
                crate::types::Rectangle {
                    x: x - alloc.x,
                    y: y - alloc.y,
                    width,
                    height,
                },
            ));
        }
    }
}

impl crate::style::ExternalWidgetImgRenderer for WordImgRenderer {
    /// `getStyle` — `words[wordNo]->style` (textblock.cc:99-102).
    fn get_style(&self) -> Option<Rc<crate::style::Style>> {
        self.style(false)
    }
}

/// `Textblock::SpaceImgRenderer` (textblock.hh:324-332) —
/// hérite `WordImgRenderer` amont ≙ `Deref` vers
/// `WordImgRenderer` + overrides de `getBgArea`/`getStyle`.
pub struct SpaceImgRenderer {
    /// Base (≙ héritage amont).
    word: WordImgRenderer,
}

impl SpaceImgRenderer {
    /// `SpaceImgRenderer (textblock, wordNo)` — ctor inline
    /// délégué (textblock.hh:327).
    pub(crate) fn new(
        arena: &Rc<RefCell<crate::layout::Arena>>,
        layout_id: usize,
        widget: WidgetId,
        word_no: i32,
    ) -> Self {
        Self {
            word: WordImgRenderer::new(arena, layout_id, widget, word_no),
        }
    }
    /// `setData` hérité (appelé depuis `addLine`).
    pub(crate) fn set_data(&mut self, x_word_widget: i32, line_no: i32) {
        self.word.set_data(x_word_widget, line_no);
    }
}

impl crate::imgrenderer::ImgRenderer for SpaceImgRenderer {
    fn set_buffer(&mut self, _b: &Rc<dyn crate::imgbuf::Imgbuf>, _resize: bool) {}
    /// `drawRow`/`finish`/`fatal` — délégués verbatim à la base.
    fn draw_row(&mut self, row: i32) {
        crate::style::external_img_renderer_draw_row(self, row);
    }
    fn finish(&mut self) {
        crate::style::external_img_renderer_finish(self);
    }
    fn fatal(&mut self) {}
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl crate::style::ExternalImgRenderer for SpaceImgRenderer {
    /// `readyToDraw` — hérité de `WordImgRenderer`.
    fn ready_to_draw(&self) -> bool {
        crate::style::ExternalImgRenderer::ready_to_draw(&self.word)
    }
    /// `getBgArea` (textblock.cc:111-117) — aire du mot puis
    /// `x += width`, `width = effSpace`.
    fn get_bg_area(&self) -> (i32, i32, i32, i32) {
        let (x, y, width, height) = self.word.bg_area();
        self.word
            .arena
            .upgrade()
            .map_or((x + width, y, 0, height), |a| {
                let arena = a.borrow();
                match &arena.nodes[self.word.widget].kind {
                    WidgetKind::Textblock { tb } => tb
                        .words
                        .get(self.word.word_no as usize)
                        .map_or((x + width, y, 0, height), |w| {
                            (x + width, y, i32::from(w.eff_space), height)
                        }),
                    _ => (x + width, y, 0, height),
                }
            })
    }
    /// `getRefArea` — hérité.
    fn get_ref_area(&self) -> (i32, i32, i32, i32) {
        self.word.ref_area()
    }
    fn get_background_image(&self) -> Option<Rc<RefCell<crate::style::StyleImage>>> {
        crate::style::ExternalWidgetImgRenderer::get_widget_background_image(self)
    }
    fn get_background_repeat(&self) -> crate::style::BackgroundRepeat {
        crate::style::ExternalWidgetImgRenderer::get_widget_background_repeat(self)
    }
    fn get_background_attachment(&self) -> crate::style::BackgroundAttachment {
        crate::style::ExternalWidgetImgRenderer::get_widget_background_attachment(self)
    }
    fn get_background_position_x(&self) -> crate::style::Length {
        crate::style::ExternalWidgetImgRenderer::get_widget_background_position_x(self)
    }
    fn get_background_position_y(&self) -> crate::style::Length {
        crate::style::ExternalWidgetImgRenderer::get_widget_background_position_y(self)
    }
    /// `draw` — hérité (queueDrawArea).
    fn draw(&mut self, x: i32, y: i32, width: i32, height: i32) {
        crate::style::ExternalImgRenderer::draw(&mut self.word, x, y, width, height);
    }
}

impl crate::style::ExternalWidgetImgRenderer for SpaceImgRenderer {
    /// `getStyle` — `words[wordNo]->spaceStyle` (textblock.cc:119-122).
    fn get_style(&self) -> Option<Rc<crate::style::Style>> {
        self.word.style(true)
    }
}

// ------------------------------------------------------------------
// Helpers verbatim divers (textblock.hh:712-744).
// ------------------------------------------------------------------

impl TextblockData {
    /// `lineYOffsetWidget (line, allocation)` (textblock.hh:712).
    pub(crate) fn line_y_offset_widget(&self, line: &Line, allocation: &Allocation) -> i32 {
        line.top + (allocation.ascent - self.lines.first().map_or(0, |l| l.border_ascent))
    }

    /// `lineYOffsetCanvas (line, allocation)` (textblock.hh:722).
    pub(crate) fn line_y_offset_canvas(&self, line: &Line, allocation: &Allocation) -> i32 {
        allocation.y + self.line_y_offset_widget(line, allocation)
    }

    /// `lineYOffsetWidgetI (lineIndex, allocation)` (textblock.hh:737).
    /// Panique-free : `None` si l'index est hors bornes (amont
    /// déréférencerait ; en Rust on borne — appelants contractuels).
    pub(crate) fn line_y_offset_widget_i(&self, line_index: i32, allocation: &Allocation) -> i32 {
        self.lines
            .get(line_index as usize)
            .map_or(0, |l| self.line_y_offset_widget(l, allocation))
    }

    /// `calcPenaltyIndexForNewLine` (textblock.hh:747-757).
    pub(crate) fn calc_penalty_index_for_new_line(&self) -> usize {
        if self.lines.is_empty() {
            0
        } else {
            let line = self.lines.last();
            match line {
                Some(l) if l.first_word <= l.last_word => 1,
                _ => 0,
            }
        }
    }
}

//! Port de `dw/events.hh` — événements souris platform-
//! indépendants. Les `Widget*` amont deviennent des `WidgetId`.

use crate::WidgetId;

/// `dw::core::ButtonState` — masque de bits (≙ enum amont).
pub mod button_state {
    /// `SHIFT_MASK`.
    pub const SHIFT_MASK: i32 = 1 << 0;
    /// `CONTROL_MASK`.
    pub const CONTROL_MASK: i32 = 1 << 1;
    /// `META_MASK`.
    pub const META_MASK: i32 = 1 << 2;
    /// `BUTTON1_MASK`.
    pub const BUTTON1_MASK: i32 = 1 << 3;
    /// `BUTTON2_MASK`.
    pub const BUTTON2_MASK: i32 = 1 << 4;
    /// `BUTTON3_MASK`.
    pub const BUTTON3_MASK: i32 = 1 << 5;
}

/// `dw::core::MouseEvent` — base de tout événement souris.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    /// `state` — combinaison de `button_state::*`.
    pub state: i32,
}

/// `dw::core::MousePositionEvent` — événement souris lié à une
/// position (coordonnées canvas + widget).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MousePositionEvent {
    /// `state`.
    pub state: i32,
    /// `xCanvas`.
    pub x_canvas: i32,
    /// `yCanvas`.
    pub y_canvas: i32,
    /// `xWidget`.
    pub x_widget: i32,
    /// `yWidget`.
    pub y_widget: i32,
}

/// `dw::core::EventButton` — pression/relâchement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventButton {
    /// `state`.
    pub state: i32,
    /// `xCanvas`.
    pub x_canvas: i32,
    /// `yCanvas`.
    pub y_canvas: i32,
    /// `xWidget`.
    pub x_widget: i32,
    /// `yWidget`.
    pub y_widget: i32,
    /// `numPressed` — 1 simple clic, 2 double, etc.
    pub num_pressed: i32,
    /// `button`.
    pub button: i32,
}

/// `dw::core::EventMotion` — mouvement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventMotion {
    /// `state`.
    pub state: i32,
    /// `xCanvas`.
    pub x_canvas: i32,
    /// `yCanvas`.
    pub y_canvas: i32,
    /// `xWidget`.
    pub x_widget: i32,
    /// `yWidget`.
    pub y_widget: i32,
}

/// `dw::core::EventCrossing` — enter/leave notify.
///
/// `lastWidget`/`currentWidget` peuvent être `NULL` amont →
/// `Option<WidgetId>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventCrossing {
    /// `state`.
    pub state: i32,
    /// `lastWidget`.
    pub last_widget: Option<WidgetId>,
    /// `currentWidget`.
    pub current_widget: Option<WidgetId>,
}

/// `dw::core::Layout::ButtonEventType` — type d'événement
/// bouton traité par `processMouseEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonEventType {
    /// `BUTTON_PRESS`.
    ButtonPress,
    /// `BUTTON_RELEASE`.
    ButtonRelease,
    /// `MOTION_NOTIFY`.
    MotionNotify,
}

//! dillo-browser — ossature applicative et chrome.
//!
//! Cible du port : `src/{cache,capi,bw,web,nav,history,bookmark,
//! cookies,domain,auth,digest,hsts,prefs,prefsparser,paths,actions,
//! keys,menu,dialog,findbar,tipwin,timeout,ui,dpiapi}.*`.
#![forbid(unsafe_code)]

pub mod actions;
pub mod bookmark;
pub mod bw;
pub mod dialog;
pub mod dpiapi;
pub mod findbar;
pub mod keys;
pub mod menu;
pub mod nav;
pub mod prefs;
pub mod tipwin;
pub mod ui;
pub mod uicmd;

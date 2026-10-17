//! Dioxus Components vendored from <https://dioxuslabs.com/components>.
//!
//! Each module holds the styled source the Dioxus registry ships for that component,
//! copied in as-is and built on `dioxus-primitives`. Styling lives beside each component
//! in its `style.css`, scoped by `#[css_module]`; the shared theme variables come from
//! `assets/dx-components-theme.css`.

// Upstream style, kept verbatim so it stays easy to diff against the registry.
#![allow(
    dead_code,
    clippy::all,
    clippy::pedantic,
    reason = "vendored from dioxuslabs/components; the project's pedantic lints are for our code"
)]

pub mod badge;
pub mod button;
pub mod progress;

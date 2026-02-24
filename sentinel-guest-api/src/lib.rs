#![allow(clippy::all)]
#![allow(warnings)]

//! # sentinel-guest-api
//!
//! Type-safe Rust bindings for guest-side agent code running inside
//! the SENTINEL Wasm sandbox.

wit_bindgen::generate!({
    path: "../wit/sentinel.wit",
    world: "sentinel-guest",
});

/// Convenience re-exports for guest authors.
pub mod prelude {
    pub use super::sentinel::agent::capabilities::*;
    pub use super::sentinel::agent::hitl::*;
    pub use super::sentinel::agent::logging::*;
    pub use super::sentinel::agent::reasoning::*;
    pub use super::Guest;
}

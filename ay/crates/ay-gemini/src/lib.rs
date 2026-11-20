
extern crate self as ay_gemini;

#[cfg(feature = "boq")]
pub mod boq;
#[cfg(feature = "web")]
pub mod web;
// The runtime drives the Gemini CLI as a child process: native targets only.
#[cfg(all(feature = "runtime", not(target_family = "wasm")))]
pub mod runtime;

#[cfg(feature = "google")]
pub mod google;

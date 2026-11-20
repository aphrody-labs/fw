
extern crate self as ay_model;

#[cfg(feature = "store")]
pub mod store;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "antigravity")]
pub mod antigravity;
#[cfg(feature = "antigravity_model")]
pub mod antigravity_model;

#[cfg(feature = "infer")]
pub mod infer;
#[cfg(feature = "embed")]
pub mod embed;
#[cfg(feature = "gateway")]
pub mod gateway;

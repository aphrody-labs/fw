
extern crate self as fw_model;

#[cfg(feature = "store")]
pub mod store;
#[cfg(feature = "client")]
pub mod client;

#[cfg(feature = "infer")]
pub mod infer;
#[cfg(feature = "embed")]
pub mod embed;
#[cfg(feature = "gateway")]
pub mod gateway;

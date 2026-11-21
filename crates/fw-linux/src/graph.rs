//! The code graph of a kernel tree (stub).

use std::path::Path;

use serde_json::Value;

use crate::error::{Error, Result};

/// `fw kernel graph <args>`: build or query the graph store at `default_db`.
pub fn cli(_arguments: &[String], _tree_root: &Path, _default_db: &Path) -> Result<Value> {
    Err(Error::message("graph: not implemented"))
}

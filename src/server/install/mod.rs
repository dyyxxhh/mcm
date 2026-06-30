//! Install route handlers — bootstrap (task 17) and package install (task 18).
//!
//! This module is a facade that re-exports from sibling submodules:
//! - `bootstrap` — `/install` bootstrap script
//! - `pkg` — `/install/pkg/{slug}` package install script

mod bootstrap;
mod pkg;

pub(crate) use bootstrap::install_script;
pub(crate) use pkg::pkg_install_script;

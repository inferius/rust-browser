//! Testing infrastructure - WPT harness, reftests, fuzz targets.
//!
//! - `wpt` - Web Platform Tests harness (testharness.js subset + result tracking)
//! - `reftest` - Reference rendering comparison (visual diff)
//! - `fuzz` - Fuzz testing targets (HTML/CSS/JS parsers)

pub mod wpt;
pub mod reftest;
pub mod test262;

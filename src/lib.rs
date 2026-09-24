//! disktree: scan a drive, see what fills it as a treemap, and delete safely.
//!
//! The library holds everything that can be tested without a window: the
//! scanner, the size-aggregating tree, the treemap layout and the delete
//! safety rules. The GUI lives in the binary.

pub mod category;
pub mod format;
pub mod fsread;
pub mod ops;
pub mod safety;
pub mod scan;
pub mod tree;
pub mod treemap;

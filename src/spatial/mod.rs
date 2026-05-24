//! Spatial index (rstar wrapper).
//!
//! Wraps `rstar::RTree<SpatialEntry>` for nearest-neighbor queries
//! (snap engine) and range queries (window selection). Uses lazy
//! rebuild with a `dirty` flag.

pub mod index;
pub mod query;

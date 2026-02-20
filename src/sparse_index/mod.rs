pub mod client;
pub mod types;

pub use client::{fetch_index, parse_ndjson};
pub use types::{IndexLine, DepEntry, DepKind, compute_path, find_latest_stable};

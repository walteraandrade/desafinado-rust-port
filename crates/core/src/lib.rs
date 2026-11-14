//! All offsets are byte offsets into UTF-8 `&str` (the TypeScript original used UTF-16 code units).

pub mod blocks;
pub mod change_set;
pub mod diff;

pub mod sample;
pub mod suggestions;
pub mod text;

//! All offsets are byte offsets into UTF-8 `&str` (the TypeScript original used UTF-16 code units).

pub mod blocks;
pub mod change_set;
pub mod diff;

pub mod suggestions;
#[cfg(test)]
mod test_support;
pub mod text;

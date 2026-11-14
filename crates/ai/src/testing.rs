pub use desafinado_core::blocks::{extract_blocks, get_neighbors};
pub use desafinado_core::sample::SAMPLE_DOCUMENT;

use crate::model::{Block, Target};

pub fn resolve_target(blocks: &[Block], from: usize, to: usize, text: &str) -> Target {
    desafinado_core::blocks::resolve_target(
        blocks,
        text,
        desafinado_core::blocks::Selection { from, to },
    )
    .expect("target")
}

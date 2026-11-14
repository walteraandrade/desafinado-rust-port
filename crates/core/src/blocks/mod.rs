mod extract_blocks;
mod frontmatter;
mod target;

pub use extract_blocks::{Block, BlockKind, extract_blocks, extract_blocks_from_blanked};
pub use frontmatter::{
    Frontmatter, Range, blank_frontmatter, find_frontmatter, touches_frontmatter,
};
pub use target::{
    Neighbors, Selection, Target, TargetSource, find_block_at, get_neighbors, heading_text,
    resolve_target, target_from_range,
};

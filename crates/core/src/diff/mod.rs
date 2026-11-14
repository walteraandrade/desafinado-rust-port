mod adaptive_diff;
mod count_words;
mod word_diff;

pub use adaptive_diff::{
    AdaptiveDiff, BLOCK_LEVEL_THRESHOLD, DiffHunk, DiffMode, adaptive_diff, changed_share,
};
pub use count_words::count_words;

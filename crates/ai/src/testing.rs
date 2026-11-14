use crate::model::{Block, BlockKind, Neighbors, Target, TargetSource};

pub const SAMPLE_DOCUMENT: &str = "# Release process update: weekday deploys\n\n## Why we are changing\n\nFor the past two years our team has frozen all production deployments on Fridays, a rule introduced after a painful weekend incident in 2022 that was entirely reasonable at the time but has since hardened into a habit nobody questions, even though it slows down every sprint, pushes risky changes into the early part of the week when the team is already stretched thin by planning and support duties, and makes it harder to ship the small fixes that customers are actually waiting for.\n\nOur pipeline has changed a lot since then. Every deploy now runs automated smoke tests, rolls out to ten percent of traffic first, and reverts itself if the error rate rises. The Friday freeze no longer protects us from much.\n\nHonestly, the old freeze was a total pain and everyone kinda hated it. So we're ditching it, no big deal, but please don't be that person who pushes a huge change at 5pm on Friday and then vanishes for the weekend lol.\n\n## What changes\n\nStarting on the first of next month, you may deploy on any weekday, as long as the following conditions are met:\n\n- Tests are passing on the main branch\n- Get a review from a second engineer\n- Rollback plan, you must write one in the pull request.\n- No deploys during the on-call handover\n\nAll deploys need a reviewed pull request. Nothing may be deployed without a review. Every deployment has to be approved by a reviewer first.\n\nTo start a canary rollout, run the deploy script from the repository root:\n\n```bash\n./scripts/deploy --env production --canary 10\n./scripts/deploy --status\n```\n\nOnce the canary looks healthy, you will recieve a prompt to promote it to all traffic. If anything looks wrong, the rollback happens automatically and the on-call engineer is notified.\n\nIf you have questions about the new process, ask in the #releases channel or contact Priya on the platform team.\n";

fn kind_of(text: &str) -> (BlockKind, Option<u8>) {
    if text.starts_with("```") {
        return (BlockKind::Code, None);
    }
    let hashes = text.chars().take_while(|c| *c == '#').count();
    if hashes > 0 && text[hashes..].starts_with(' ') {
        return (BlockKind::Heading, Some(hashes as u8));
    }
    if text.starts_with("- ") {
        return (BlockKind::List, None);
    }
    (BlockKind::Paragraph, None)
}

pub fn extract_blocks(text: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut start: Option<usize> = None;
    let mut fenced = false;
    let mut offset = 0;
    let mut end = 0;
    let flush = |blocks: &mut Vec<Block>, start: usize, end: usize| {
        let slice = &text[start..end];
        let (kind, level) = kind_of(slice);
        blocks.push(Block {
            id: format!("p{}", blocks.len() + 1),
            from: start,
            to: end,
            text: slice.to_owned(),
            kind,
            level,
        });
    };
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches('\n');
        if content.starts_with("```") {
            fenced = !fenced;
        }
        if content.is_empty() && !fenced {
            if let Some(s) = start.take() {
                flush(&mut blocks, s, end);
            }
        } else {
            start.get_or_insert(offset);
            end = offset + content.len();
        }
        offset += line.len();
    }
    if let Some(s) = start {
        flush(&mut blocks, s, end);
    }
    blocks
}

fn heading_text(block: &Block) -> String {
    block.text.trim_start_matches('#').trim().to_owned()
}

pub fn resolve_target(blocks: &[Block], from: usize, to: usize, text: &str) -> Target {
    if from == to {
        let block = blocks
            .iter()
            .find(|block| block.from <= from && from <= block.to)
            .expect("block at position");
        return Target {
            source: TargetSource::Block,
            from: block.from,
            to: block.to,
            text: block.text.clone(),
            blocks: vec![block.clone()],
        };
    }
    Target {
        source: TargetSource::Selection,
        from,
        to,
        text: text[from..to].to_owned(),
        blocks: blocks
            .iter()
            .filter(|block| block.from < to && block.to > from)
            .cloned()
            .collect(),
    }
}

pub fn get_neighbors(blocks: &[Block], target: &Target) -> Neighbors {
    let from = target
        .blocks
        .iter()
        .map(|block| block.from)
        .chain([target.from])
        .min()
        .unwrap_or(target.from);
    let to = target
        .blocks
        .iter()
        .map(|block| block.to)
        .chain([target.to])
        .max()
        .unwrap_or(target.to);
    let preceding: Vec<&Block> = blocks.iter().filter(|block| block.to <= from).collect();
    let following: Vec<&Block> = blocks.iter().filter(|block| block.from >= to).collect();
    Neighbors {
        title: blocks
            .iter()
            .find(|block| block.kind == BlockKind::Heading && block.level == Some(1))
            .map(heading_text),
        section: preceding
            .iter()
            .rfind(|block| block.kind == BlockKind::Heading)
            .map(|block| heading_text(block)),
        before: preceding
            .iter()
            .rev()
            .take(2)
            .rev()
            .map(|block| (*block).clone())
            .collect(),
        after: following
            .iter()
            .take(2)
            .map(|block| (*block).clone())
            .collect(),
    }
}

use std::{sync::Arc, thread, time::Duration};

use desafinado_ai::{
    edits::ReviewValidationContext,
    notes::{CommentPromptInput, build_comment_prompt},
    prompts::{ReviewPromptInput, TargetPromptInput, build_review_prompt, build_target_prompt},
    records::{format_records_table, summarize_costs},
};
use desafinado_core::{
    blocks::{Selection, extract_blocks, get_neighbors, resolve_target},
    sample::SAMPLE_DOCUMENT,
};
use desafinado_runtime::{
    ai_backend::{AiBackend, CommentCall, ModeCall, ReviewCall, ReviewEvent, TargetCall},
    openrouter_ai::OpenRouterAi,
    services::Services,
};

fn main() {
    let services = Services::load();
    let before = services.log.read_all().map_or(0, |records| records.len());
    println!("model: {}", services.model());
    let ai = OpenRouterAi::new(Arc::clone(&services));
    let blocks = extract_blocks(SAMPLE_DOCUMENT);
    let paragraph = blocks
        .iter()
        .find(|block| block.kind.as_str() == "paragraph")
        .expect("a paragraph");
    let target = resolve_target(
        &blocks,
        SAMPLE_DOCUMENT,
        Selection {
            from: paragraph.from,
            to: paragraph.from,
        },
    )
    .expect("a target");
    let parts = build_target_prompt(&TargetPromptInput {
        target: target.clone(),
        neighbors: get_neighbors(&blocks, &target),
        instruction:
            "Make the text shorter. Remove redundancy and filler, and keep every key point.".into(),
    });
    match ai.target(TargetCall {
        task: "ai-check: cursor in p1 → Shorter".into(),
        parts,
        original: target.text.clone(),
    }) {
        Ok(result) => println!(
            "target ok\n  original: {}\n  replacement: {}\n  rationale: {}",
            target.text, result.replacement, result.rationale
        ),
        Err(error) => println!("target failed: {error:?}"),
    }
    check_comment_review(&ai, &blocks);
    let parts = build_review_prompt(&ReviewPromptInput {
        blocks: blocks.clone(),
        focus: None,
    });
    let mut edits = 0;
    let mut failure = None;
    ai.review(
        ReviewCall {
            task: "ai-check: review document → default criteria".into(),
            parts,
            context: ReviewValidationContext {
                blocks,
                occupied: std::collections::BTreeSet::new(),
            },
        },
        &mut |event| {
            match event {
                ReviewEvent::Edit(edit) => {
                    edits += 1;
                    println!("review edit {}: {}", edit.block_id, edit.rationale);
                }
                ReviewEvent::Dropped(_) => {}
                ReviewEvent::Done { error } => failure = error,
                ReviewEvent::Failed(error) => failure = Some(error),
            }
            true
        },
    );
    match failure {
        None => println!("review ok: {edits} edits"),
        Some(error) => println!("review failed after {edits} edits: {error:?}"),
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(280);
    let records = loop {
        let records = services.log.read_all().unwrap_or_default();
        let done = records.len().saturating_sub(before) >= 5;
        if done || std::time::Instant::now() >= deadline {
            break records;
        }
        thread::sleep(Duration::from_secs(2));
    };
    let new = &records[before.min(records.len())..];
    println!("\n{}", format_records_table(new));
    let summary = summarize_costs(new);
    println!(
        "calls: {}, priced: {}, total cost: US${:.6}",
        summary.total.calls, summary.total.priced_calls, summary.total.cost_usd
    );
}

fn check_comment_review(ai: &OpenRouterAi, blocks: &[desafinado_core::blocks::Block]) {
    for focus in ["what do you think of the tone?", "fix the spelling"] {
        match ai.review_mode(ModeCall {
            task: format!("ai-check: choose review mode → {focus}"),
            focus: focus.into(),
        }) {
            Ok(choice) => println!("review mode for {focus:?}: {}", choice.chip_label()),
            Err(error) => println!("review mode failed for {focus:?}: {error:?}"),
        }
    }
    match ai.comment(CommentCall {
        task: "ai-check: comment on document → default criteria".into(),
        parts: build_comment_prompt(&CommentPromptInput {
            blocks: blocks.to_vec(),
            focus: None,
        }),
        blocks: blocks.to_vec(),
    }) {
        Ok(board) => {
            println!(
                "comment ok: {} general notes, {} notes, {} dropped",
                board.general.len(),
                board.notes.len(),
                board.dropped
            );
            for note in board.shown() {
                println!(
                    "  {} {} [{}]: {}",
                    note.number,
                    note.block_id,
                    note.kind.label(),
                    note.title
                );
            }
        }
        Err(error) => println!("comment failed: {error:?}"),
    }
}

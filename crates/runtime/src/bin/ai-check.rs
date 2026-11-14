use std::{thread, time::Duration};

use desafinado_ai::{
    edits::ReviewValidationContext,
    prompts::{ReviewPromptInput, TargetPromptInput, build_review_prompt, build_target_prompt},
    records::{format_records_table, summarize_costs},
    review_stream::{ReviewStream, ReviewStreamEvent},
};
use desafinado_core::{
    blocks::{Selection, extract_blocks, get_neighbors, resolve_target},
    sample::SAMPLE_DOCUMENT,
};
use desafinado_runtime::{
    ai_backend::{AiBackend, ReviewCall, ReviewEvent, TargetCall},
    openrouter_ai::OpenRouterAi,
    services::Services,
};

fn main() {
    let services = Services::load();
    let before = services.log.read_all().map(|records| records.len()).unwrap_or(0);
    println!("model: {}", services.model());
    let ai = OpenRouterAi::new(services.clone());
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
        instruction: "Make the text shorter. Remove redundancy and filler, and keep every key point.".into(),
    });
    match ai.target(TargetCall {
        task: "ai-check: cursor in p1 → Shorter".into(),
        parts,
        original: target.text.clone(),
    }) {
        Ok(result) => println!("target ok\n  original: {}\n  replacement: {}\n  rationale: {}", target.text, result.replacement, result.rationale),
        Err(error) => println!("target failed: {error:?}"),
    }
    let parts = build_review_prompt(&ReviewPromptInput {
        blocks: blocks.clone(),
        focus: None,
    });
    let mut stream = ReviewStream::new(ReviewValidationContext {
        blocks,
        occupied: Default::default(),
    });
    let mut edits = 0;
    let mut failure = None;
    ai.review(
        ReviewCall {
            task: "ai-check: review document → default criteria".into(),
            parts,
        },
        &mut |event| {
            match event {
                ReviewEvent::Chunk(chunk) => {
                    for event in stream.feed(&chunk) {
                        if let ReviewStreamEvent::Edit(edit) = event {
                            edits += 1;
                            println!("review edit {}: {}", edit.block_id, edit.rationale);
                        }
                    }
                }
                ReviewEvent::Done => failure = stream.finish().err(),
                ReviewEvent::Failed(error) => failure = Some(error),
            }
            true
        },
    );
    match failure {
        None => println!("review ok: {edits} edits"),
        Some(error) => println!("review failed after {edits} edits: {error:?}"),
    }
    thread::sleep(Duration::from_secs(14));
    let records = services.log.read_all().unwrap_or_default();
    let new = &records[before.min(records.len())..];
    println!("\n{}", format_records_table(new));
    let summary = summarize_costs(new);
    println!(
        "calls: {}, priced: {}, total cost: US${:.6}",
        summary.total.calls, summary.total.priced_calls, summary.total.cost_usd
    );
}

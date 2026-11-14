use std::sync::Arc;

use desafinado_ai::prompts::PromptParts;
use desafinado_core::suggestions::{ClientError, SuggestionResult};

#[derive(Clone, Debug)]
pub struct TargetCall {
    pub task: String,
    pub parts: PromptParts,
    pub original: String,
}

#[derive(Clone, Debug)]
pub struct ReviewCall {
    pub task: String,
    pub parts: PromptParts,
}

pub enum ReviewEvent {
    Chunk(String),
    Done,
    Failed(ClientError),
}

pub trait AiBackend: Send + Sync {
    fn target(&self, call: TargetCall) -> Result<SuggestionResult, ClientError>;
    fn review(&self, call: ReviewCall, emit: &mut dyn FnMut(ReviewEvent) -> bool);
}

pub struct FakeAi;

fn fake_rewrite(original: &str) -> String {
    let words: Vec<&str> = original.split_whitespace().collect();
    let keep = (words.len() * 2 / 3).max(1);
    let mut text = words[..keep.min(words.len())].join(" ");
    if !text.ends_with('.') {
        text.push('.');
    }
    text
}

impl AiBackend for FakeAi {
    fn target(&self, call: TargetCall) -> Result<SuggestionResult, ClientError> {
        std::thread::sleep(std::time::Duration::from_millis(600));
        let replacement = fake_rewrite(&call.original);
        Ok(SuggestionResult {
            unchanged: replacement == call.original,
            replacement,
            rationale: "Cuts the last third to keep the point short.".into(),
        })
    }

    fn review(&self, call: ReviewCall, emit: &mut dyn FnMut(ReviewEvent) -> bool) {
        let ids: Vec<String> = call
            .parts
            .prompt
            .lines()
            .filter_map(|line| {
                line.strip_prefix("<block id=\"")
                    .and_then(|rest| rest.split('"').next())
                    .map(String::from)
            })
            .take(2)
            .collect();
        let body = ids
            .iter()
            .map(|id| {
                serde_json::json!({
                    "blockId": id,
                    "replacement": "A tighter version of this block.",
                    "rationale": "Says the same in fewer words."
                })
                .to_string()
            })
            .collect::<Vec<_>>()
            .join(",");
        let text = format!("{{\"elements\":[{body}]}}");
        for chunk in text.as_bytes().chunks(16) {
            std::thread::sleep(std::time::Duration::from_millis(40));
            if !emit(ReviewEvent::Chunk(
                String::from_utf8_lossy(chunk).to_string(),
            )) {
                return;
            }
        }
        emit(ReviewEvent::Done);
    }
}

pub fn backend() -> Arc<dyn AiBackend> {
    Arc::new(FakeAi)
}

use std::sync::Arc;

use desafinado_ai::prompts::{DescriptionPromptInput, PromptParts, build_description_prompt};
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

pub const DESCRIPTION_TASK: &str = "Improve skill description";

#[derive(Clone, Debug)]
pub struct DescriptionCall {
    pub task: String,
    pub input: DescriptionPromptInput,
}

impl DescriptionCall {
    pub fn new(input: DescriptionPromptInput) -> Self {
        Self {
            task: DESCRIPTION_TASK.into(),
            input,
        }
    }

    pub fn target_call(&self) -> TargetCall {
        TargetCall {
            task: self.task.clone(),
            parts: build_description_prompt(&self.input),
            original: self.input.description.clone(),
        }
    }
}

pub enum ReviewEvent {
    Chunk(String),
    Done,
    Failed(ClientError),
}

pub trait AiBackend: Send + Sync {
    fn target(&self, call: TargetCall) -> Result<SuggestionResult, ClientError>;
    fn review(&self, call: ReviewCall, emit: &mut dyn FnMut(ReviewEvent) -> bool);
    fn description(&self, call: DescriptionCall) -> Result<SuggestionResult, ClientError>;
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

fn fake_description(name: &str, description: &str) -> String {
    let topic = name.replace('-', " ");
    let current = description.trim().trim_end_matches('.');
    format!("Use when the writer asks for {topic}, or when a task matches: {current}.")
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

    fn description(&self, call: DescriptionCall) -> Result<SuggestionResult, ClientError> {
        std::thread::sleep(std::time::Duration::from_millis(600));
        Ok(SuggestionResult {
            replacement: fake_description(&call.input.name, &call.input.description),
            rationale: "Names when to invoke the skill instead of what it does.".into(),
            unchanged: false,
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

pub fn backend(services: Arc<crate::services::Services>) -> Arc<dyn AiBackend> {
    if std::env::var_os("DESAFINADO_FAKE_AI").is_some() {
        Arc::new(FakeAi)
    } else {
        Arc::new(crate::openrouter_ai::OpenRouterAi::new(services))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> DescriptionPromptInput {
        DescriptionPromptInput {
            name: "pdf-tools".into(),
            description: "Helps with PDFs.".into(),
            body: "# PDF tools".into(),
        }
    }

    #[test]
    fn a_description_call_is_a_target_call_on_the_current_description() {
        let call = DescriptionCall::new(input()).target_call();
        assert_eq!(call.task, "Improve skill description");
        assert_eq!(call.original, "Helps with PDFs.");
        assert_eq!(call.parts, build_description_prompt(&input()));
    }

    #[test]
    fn the_fake_description_differs_from_the_current_one() {
        let result = FakeAi.description(DescriptionCall::new(input())).unwrap();
        assert_ne!(result.replacement, "Helps with PDFs.");
        assert!(result.replacement.starts_with("Use when"));
    }
}

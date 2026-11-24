use desafinado_core::log_err::LogErr;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use desafinado_ai::{
    edits::{DroppedEdit, ReviewValidationContext},
    notes::{CommentOutput, GeneralNote, NoteBoard, NoteKind, NoteOutput, place_notes},
    prompts::{DescriptionPromptInput, PromptParts, build_description_prompt},
    records::{CallObservation, CallOutcome, CallTiming, ShapeRecordInput, shape_ai_call_record},
    review_mode::{ModeChoice, ModeDecision, ModeSource, ReviewMode},
    review_stream::{ReviewStream, ReviewStreamEvent, should_retry_review},
};
use desafinado_core::suggestions::{ClientError, SuggestionResult, ValidatedEdit};
use desafinado_platform::records::AiCallLog;

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
    pub context: ReviewValidationContext,
}

#[derive(Clone, Debug)]
pub struct CommentCall {
    pub task: String,
    pub parts: PromptParts,
    pub blocks: Vec<desafinado_core::blocks::Block>,
}

#[derive(Clone, Debug)]
pub struct ModeCall {
    pub task: String,
    pub focus: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewEvent {
    Edit(ValidatedEdit),
    Dropped(DroppedEdit),
    Done { error: Option<ClientError> },
    Failed(ClientError),
}

pub trait AiBackend: Send + Sync {
    fn target(&self, call: TargetCall) -> Result<SuggestionResult, ClientError>;
    fn review(&self, call: ReviewCall, emit: &mut dyn FnMut(ReviewEvent) -> bool);
    fn description(&self, call: DescriptionCall) -> Result<SuggestionResult, ClientError>;
    /// Asks the decision model whether a Focus asks for comments or for changes.
    fn review_mode(&self, call: ModeCall) -> Result<ModeChoice, ClientError>;
    /// A Comment review: Notes on the Document, placed on the Blocks that were read.
    fn comment(&self, call: CommentCall) -> Result<NoteBoard, ClientError>;
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn outcome_of(failure: Option<&ClientError>) -> CallOutcome {
    match failure {
        None => CallOutcome::Valid,
        Some(ClientError::InvalidOutput { .. }) => CallOutcome::Invalid,
        Some(_) => CallOutcome::Error,
    }
}

pub fn stopped_error() -> ClientError {
    ClientError::Aborted {
        message: "Stopped by the caller".into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAttempt {
    pub validated: usize,
    pub failure: Option<ClientError>,
    pub stopped: bool,
}

pub type Emit<'a> = dyn FnMut(ReviewEvent) -> bool + 'a;

pub fn drive_review(
    attempt: &mut dyn FnMut(u32, &mut Emit) -> ReviewAttempt,
    emit: &mut dyn FnMut(ReviewEvent) -> bool,
) {
    let first = attempt(1, emit);
    if first.stopped {
        return;
    }
    let last = if should_retry_review(first.failure.as_ref(), first.validated) {
        attempt(2, emit)
    } else {
        first
    };
    if last.stopped {
        return;
    }
    match last.failure {
        Some(error) if last.validated == 0 => {
            emit(ReviewEvent::Failed(error));
        }
        error => {
            emit(ReviewEvent::Done { error });
        }
    }
}

pub fn feed_review(
    stream: &mut ReviewStream,
    chunk: &str,
    emit: &mut dyn FnMut(ReviewEvent) -> bool,
    on_edit: &mut dyn FnMut(),
) -> bool {
    for event in stream.feed(chunk) {
        let event = match event {
            ReviewStreamEvent::Edit(edit) => {
                on_edit();
                ReviewEvent::Edit(edit)
            }
            ReviewStreamEvent::Dropped(dropped) => ReviewEvent::Dropped(dropped),
            ReviewStreamEvent::InvalidElement { .. } => continue,
        };
        if !emit(event) {
            return false;
        }
    }
    true
}

pub struct FakeAi {
    log: Option<AiCallLog>,
}

const FAKE_MODEL: &str = "fake/desafinado";
const FAKE_DECISION_MODEL: &str = "fake/jev";
const REVISE_WORDS: [&str; 9] = [
    "fix",
    "shorten",
    "rewrite",
    "cut",
    "translate",
    "correct",
    "corrig",
    "enxug",
    "reescrev",
];

fn fake_mode(focus: &str) -> ModeDecision {
    let lower = focus.to_lowercase();
    let revise = REVISE_WORDS.iter().any(|word| lower.contains(word));
    ModeDecision {
        choice: ModeChoice {
            mode: if revise {
                ReviewMode::Revise
            } else {
                ReviewMode::Comment
            },
            source: ModeSource::Decided {
                probability: Some(if revise { 0.91 } else { 0.96 }),
            },
        },
        observation: desafinado_ai::records::CallObservation {
            requested_model: FAKE_DECISION_MODEL.into(),
            served_model: Some(FAKE_DECISION_MODEL.into()),
            provider: Some("Fake".into()),
            input_tokens: Some(210.0),
            cost_usd: Some(0.000_008_8),
            ..Default::default()
        },
    }
}

const FAKE_NOTES: [(NoteKind, &str, &str); 3] = [
    (
        NoteKind::Structure,
        "Repeats the opening",
        "Says again what the first paragraph said. Cut it, or carry the idea forward with an example.",
    ),
    (
        NoteKind::Clarity,
        "The point arrives late",
        "A long sentence hides its main idea at the end. Split it in two.",
    ),
    (
        NoteKind::Fact,
        "Claim without a source",
        "Say where this comes from, or mark it as an anecdote.",
    ),
];

fn fake_comment_output(blocks: &[desafinado_core::blocks::Block]) -> CommentOutput {
    let paragraphs = blocks
        .iter()
        .filter(|block| block.kind == desafinado_core::blocks::BlockKind::Paragraph);
    CommentOutput {
        general: vec![
            GeneralNote {
                topic: "Structure".into(),
                text: "The second half returns to the start; the ending repeats the opening instead of closing the argument.".into(),
            },
            GeneralNote {
                topic: "What works".into(),
                text: "The central image holds the piece together. Come back to it at the end.".into(),
            },
        ],
        notes: paragraphs
            .skip(1)
            .zip(FAKE_NOTES)
            .map(|(block, (kind, title, comment))| NoteOutput {
                block_id: block.id.clone(),
                kind,
                title: title.into(),
                comment: comment.into(),
            })
            .collect(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FakeScript {
    Normal,
    Slow,
    Network,
    Invalid,
    Credit,
    Partial,
}

fn fake_script(prompt: &str) -> FakeScript {
    [
        ("fake:slow", FakeScript::Slow),
        ("fake:network", FakeScript::Network),
        ("fake:invalid", FakeScript::Invalid),
        ("fake:credit", FakeScript::Credit),
        ("fake:partial", FakeScript::Partial),
    ]
    .into_iter()
    .find(|(marker, _)| prompt.contains(marker))
    .map_or(FakeScript::Normal, |(_, script)| script)
}

/// Two thirds of each line's words, so a request over many paragraphs keeps its paragraphs.
fn fake_rewrite(original: &str) -> String {
    original
        .split('\n')
        .map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            if words.is_empty() {
                return String::new();
            }
            let keep = (words.len() * 2 / 3).max(1);
            let mut text = words[..keep].join(" ");
            if !text.ends_with('.') {
                text.push('.');
            }
            text
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fake_refine(original: &str) -> String {
    let words: Vec<&str> = original.split_whitespace().collect();
    let keep = (words.len() / 2).max(1);
    format!(
        "{}, gently.",
        words[..keep.min(words.len())]
            .join(" ")
            .trim_end_matches(['.', ','])
    )
}

fn fake_description(name: &str, description: &str) -> String {
    let topic = name.replace('-', " ");
    let current = description.trim().trim_end_matches('.');
    format!("Use when the writer asks for {topic}, or when a task matches: {current}.")
}

fn network_error() -> ClientError {
    ClientError::Network {
        message: "fake: connection refused".into(),
    }
}

impl FakeAi {
    pub fn new(log: Option<AiCallLog>) -> Self {
        Self { log }
    }

    fn record(
        &self,
        task: &str,
        attempt: u32,
        failure: Option<&ClientError>,
        clock: Instant,
        first_suggestion_ms: Option<f64>,
    ) {
        let Some(log) = self.log.as_ref() else {
            return;
        };
        let record = shape_ai_call_record(&ShapeRecordInput {
            task: task.to_string(),
            attempt,
            outcome: outcome_of(failure),
            started_at: now_iso(),
            observation: CallObservation {
                requested_model: FAKE_MODEL.into(),
                served_model: failure.is_none().then(|| FAKE_MODEL.into()),
                provider: failure.is_none().then(|| "Fake".into()),
                finish_reason: failure.is_none().then(|| "stop".into()),
                error: failure.map(|error| error.kind().to_string()),
                ..Default::default()
            },
            generation: None,
            timing: CallTiming {
                client_total_ms: clock.elapsed().as_millis() as u64,
                first_element_ms: first_suggestion_ms,
                first_suggestion_ms,
            },
        });
        log.append(&record).log_err();
    }

    fn answer(
        &self,
        call: &TargetCall,
        result: impl Fn() -> SuggestionResult,
    ) -> Result<SuggestionResult, ClientError> {
        let script = fake_script(&call.parts.prompt);
        let wait = if script == FakeScript::Slow {
            4000
        } else {
            600
        };
        let failure = match script {
            FakeScript::Network => Some(network_error()),
            FakeScript::Credit => Some(ClientError::OutOfCredit {
                message: "fake: insufficient credits".into(),
            }),
            FakeScript::Invalid => Some(ClientError::InvalidOutput {
                message: "The replacement was empty".into(),
            }),
            _ => None,
        };
        let attempts = if script == FakeScript::Invalid { 2 } else { 1 };
        for attempt in 1..=attempts {
            let clock = Instant::now();
            std::thread::sleep(Duration::from_millis(wait));
            self.record(&call.task, attempt, failure.as_ref(), clock, None);
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(result()),
        }
    }
}

impl AiBackend for FakeAi {
    fn review_mode(&self, call: ModeCall) -> Result<ModeChoice, ClientError> {
        let clock = Instant::now();
        std::thread::sleep(Duration::from_millis(150));
        let decision = fake_mode(&call.focus);
        if let Some(log) = self.log.as_ref() {
            let record = shape_ai_call_record(&ShapeRecordInput {
                task: call.task,
                attempt: 1,
                outcome: CallOutcome::Valid,
                started_at: now_iso(),
                observation: decision.observation,
                generation: None,
                timing: CallTiming {
                    client_total_ms: clock.elapsed().as_millis() as u64,
                    ..Default::default()
                },
            });
            log.append(&record).log_err();
        }
        Ok(decision.choice)
    }

    fn comment(&self, call: CommentCall) -> Result<NoteBoard, ClientError> {
        let target = TargetCall {
            task: call.task.clone(),
            parts: call.parts.clone(),
            original: String::new(),
        };
        self.answer(&target, || SuggestionResult {
            replacement: String::new(),
            rationale: String::new(),
            unchanged: true,
        })?;
        Ok(place_notes(fake_comment_output(&call.blocks), &call.blocks))
    }

    fn target(&self, call: TargetCall) -> Result<SuggestionResult, ClientError> {
        self.answer(&call, || {
            let refining = call.parts.prompt.contains("<latest-replacement>");
            let replacement = if refining {
                fake_refine(&call.original)
            } else {
                fake_rewrite(&call.original)
            };
            SuggestionResult {
                unchanged: replacement == call.original,
                replacement,
                rationale: if refining {
                    "Halves the text and softens the ending.".into()
                } else {
                    "Cuts the last third to keep the point short.".into()
                },
            }
        })
    }

    fn description(&self, call: DescriptionCall) -> Result<SuggestionResult, ClientError> {
        let target = call.target_call();
        self.answer(&target, || SuggestionResult {
            replacement: fake_description(&call.input.name, &call.input.description),
            rationale: "Names when to invoke the skill instead of what it does.".into(),
            unchanged: false,
        })
    }

    fn review(&self, call: ReviewCall, emit: &mut dyn FnMut(ReviewEvent) -> bool) {
        let script = fake_script(&call.parts.prompt);
        let ids: Vec<String> = call
            .context
            .blocks
            .iter()
            .filter(|block| block.kind == desafinado_core::blocks::BlockKind::Paragraph)
            .map(|block| block.id.clone())
            .filter(|id| !call.context.occupied.contains(id))
            .take(if script == FakeScript::Partial { 1 } else { 2 })
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
        let text = match script {
            FakeScript::Network => String::new(),
            FakeScript::Partial => text[..text.len() - 2].to_string(),
            _ => text,
        };
        let chunk_ms = if script == FakeScript::Slow { 160 } else { 40 };
        drive_review(
            &mut |attempt, emit| {
                let clock = Instant::now();
                let mut stream = ReviewStream::new(call.context.clone());
                let mut validated = 0;
                let mut first: Option<f64> = None;
                for chunk in text.as_bytes().chunks(16) {
                    std::thread::sleep(Duration::from_millis(chunk_ms));
                    let alive = feed_review(
                        &mut stream,
                        &String::from_utf8_lossy(chunk),
                        emit,
                        &mut || {
                            validated += 1;
                            first.get_or_insert(clock.elapsed().as_millis() as f64);
                        },
                    );
                    if !alive {
                        self.record(&call.task, attempt, Some(&stopped_error()), clock, first);
                        return ReviewAttempt {
                            validated,
                            failure: None,
                            stopped: true,
                        };
                    }
                }
                let failure = match script {
                    FakeScript::Network | FakeScript::Partial => Some(network_error()),
                    _ => stream.finish().err(),
                };
                self.record(&call.task, attempt, failure.as_ref(), clock, first);
                ReviewAttempt {
                    validated,
                    failure,
                    stopped: false,
                }
            },
            emit,
        );
    }
}

pub fn backend(services: Arc<crate::services::Services>) -> Arc<dyn AiBackend> {
    if std::env::var_os("DESAFINADO_FAKE_AI").is_some() {
        Arc::new(FakeAi::new(Some(AiCallLog::new(&services.user_data))))
    } else {
        Arc::new(crate::openrouter_ai::OpenRouterAi::new(services))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desafinado_ai::prompts::{ReviewPromptInput, build_review_prompt};
    use desafinado_core::blocks::extract_blocks;

    fn input() -> DescriptionPromptInput {
        DescriptionPromptInput {
            name: "pdf-tools".into(),
            description: "Helps with PDFs.".into(),
            body: "# PDF tools".into(),
        }
    }

    #[test]
    fn the_fake_rewrite_shortens_each_paragraph_and_keeps_the_blank_lines() {
        assert_eq!(
            fake_rewrite("One two three four five six.\n\nSeven eight nine\n"),
            "One two three four.\n\nSeven eight.\n"
        );
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
        let result = FakeAi::new(None)
            .description(DescriptionCall::new(input()))
            .unwrap();
        assert_ne!(result.replacement, "Helps with PDFs.");
        assert!(result.replacement.starts_with("Use when"));
    }

    #[test]
    fn names_the_outcome_of_a_call_by_its_failure() {
        assert_eq!(outcome_of(None), CallOutcome::Valid);
        assert_eq!(
            outcome_of(Some(&ClientError::InvalidOutput {
                message: "x".into()
            })),
            CallOutcome::Invalid
        );
        assert_eq!(outcome_of(Some(&network_error())), CallOutcome::Error);
        assert_eq!(outcome_of(Some(&stopped_error())), CallOutcome::Error);
    }

    fn invalid() -> ClientError {
        ClientError::InvalidOutput {
            message: "bad".into(),
        }
    }

    fn run(attempts: Vec<ReviewAttempt>) -> (Vec<u32>, Vec<ReviewEvent>) {
        let mut queue = attempts.into_iter();
        let mut made = Vec::new();
        let mut events = Vec::new();
        drive_review(
            &mut |number, _| {
                made.push(number);
                queue.next().unwrap()
            },
            &mut |event| {
                events.push(event);
                true
            },
        );
        (made, events)
    }

    #[test]
    fn a_clean_review_pass_is_done_after_one_attempt() {
        let (made, events) = run(vec![ReviewAttempt {
            validated: 2,
            failure: None,
            stopped: false,
        }]);
        assert_eq!(made, vec![1]);
        assert_eq!(events, vec![ReviewEvent::Done { error: None }]);
    }

    #[test]
    fn retries_a_review_pass_whose_output_was_invalid_before_any_suggestion() {
        let (made, events) = run(vec![
            ReviewAttempt {
                validated: 0,
                failure: Some(invalid()),
                stopped: false,
            },
            ReviewAttempt {
                validated: 1,
                failure: None,
                stopped: false,
            },
        ]);
        assert_eq!(made, vec![1, 2]);
        assert_eq!(events, vec![ReviewEvent::Done { error: None }]);
    }

    #[test]
    fn a_network_failure_before_any_suggestion_fails_without_a_retry() {
        let (made, events) = run(vec![ReviewAttempt {
            validated: 0,
            failure: Some(network_error()),
            stopped: false,
        }]);
        assert_eq!(made, vec![1]);
        assert_eq!(events, vec![ReviewEvent::Failed(network_error())]);
    }

    #[test]
    fn a_failure_after_some_suggestions_ends_the_pass_with_the_error() {
        let (made, events) = run(vec![ReviewAttempt {
            validated: 1,
            failure: Some(invalid()),
            stopped: false,
        }]);
        assert_eq!(made, vec![1]);
        assert_eq!(
            events,
            vec![ReviewEvent::Done {
                error: Some(invalid())
            }]
        );
    }

    #[test]
    fn a_stopped_pass_emits_nothing_more() {
        let (_, events) = run(vec![ReviewAttempt {
            validated: 0,
            failure: None,
            stopped: true,
        }]);
        assert!(events.is_empty());
    }

    fn review_call(document: &str) -> ReviewCall {
        let blocks = extract_blocks(document);
        ReviewCall {
            task: "review document → default criteria".into(),
            parts: build_review_prompt(&ReviewPromptInput {
                blocks: blocks.clone(),
                focus: None,
            }),
            context: ReviewValidationContext {
                blocks,
                occupied: std::collections::BTreeSet::new(),
            },
        }
    }

    #[test]
    fn the_fake_review_streams_validated_edits_and_ends() {
        let mut events = Vec::new();
        FakeAi::new(None).review(
            review_call("One two three.\n\nFour five six.\n"),
            &mut |event| {
                events.push(event);
                true
            },
        );
        let edits = events
            .iter()
            .filter(|event| matches!(event, ReviewEvent::Edit(_)))
            .count();
        assert_eq!(edits, 2);
        assert_eq!(events.last(), Some(&ReviewEvent::Done { error: None }));
    }

    #[test]
    fn the_fake_decision_reads_revision_words_in_the_focus() {
        let ai = FakeAi::new(None);
        let ask = |focus: &str| {
            ai.review_mode(ModeCall {
                task: "mode".into(),
                focus: focus.into(),
            })
            .unwrap()
            .mode
        };
        assert_eq!(ask("corrige a ortografia"), ReviewMode::Revise);
        assert_eq!(ask("what do you think of the tone?"), ReviewMode::Comment);
    }

    #[test]
    fn the_fake_comment_review_leaves_notes_on_paragraphs() {
        let blocks = extract_blocks("# T\n\nOne.\n\nTwo.\n\nThree.\n");
        let board = FakeAi::new(None)
            .comment(CommentCall {
                task: "comment".into(),
                parts: PromptParts {
                    instructions: String::new(),
                    prompt: String::new(),
                },
                blocks,
            })
            .unwrap();
        assert_eq!(board.general.len(), 2);
        let ids: Vec<&str> = board
            .notes
            .iter()
            .map(|note| note.block_id.as_str())
            .collect();
        assert_eq!(ids, ["p3", "p4"]);
    }

    #[test]
    fn the_fake_scripts_failures_from_markers_in_the_prompt() {
        assert_eq!(
            fake_script("Instruction: fake:network"),
            FakeScript::Network
        );
        assert_eq!(fake_script("Instruction: Shorter"), FakeScript::Normal);
        let call = TargetCall {
            task: "cursor in p1 → free text: fake:invalid".into(),
            parts: PromptParts {
                instructions: String::new(),
                prompt: "Instruction: fake:credit".into(),
            },
            original: "Text.".into(),
        };
        assert!(matches!(
            FakeAi::new(None).target(call),
            Err(ClientError::OutOfCredit { .. })
        ));
    }
}

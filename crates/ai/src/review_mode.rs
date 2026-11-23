//! Which kind of Review the writer asked for: comments from an editor, or changes to the text.
//! A decision model (Jev, on OpenRouter's Decisions API) reads the Focus and picks one.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::errors::ClientError;
use crate::records::CallObservation;
use crate::text::js_trim;

pub const DECISION_MODEL: &str = "typesafe/jev-1.14";
pub const DECISION_PROVIDER: &str = "TypeSafe";
const QUESTION: &str = "mode";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewMode {
    Comment,
    Revise,
}

impl ReviewMode {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::Revise => "revise",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Comment => "Comment",
            Self::Revise => "Revise",
        }
    }

    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Comment => Self::Revise,
            Self::Revise => Self::Comment,
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        [Self::Comment, Self::Revise]
            .into_iter()
            .find(|mode| mode.key() == key)
    }
}

/// Who picked the mode: no Focus means Comment without asking, the decision model reads a
/// Focus, and the writer can switch it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ModeSource {
    Default,
    Decided { probability: Option<f64> },
    Writer,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModeChoice {
    pub mode: ReviewMode,
    pub source: ModeSource,
}

impl ModeChoice {
    pub const fn by_writer(mode: ReviewMode) -> Self {
        Self {
            mode,
            source: ModeSource::Writer,
        }
    }

    /// The chip's text, such as "Comment · Jev 96%".
    pub fn chip_label(&self) -> String {
        match self.source {
            ModeSource::Default => format!("{} · default", self.mode.label()),
            ModeSource::Decided {
                probability: Some(probability),
            } => format!(
                "{} · Jev {}%",
                self.mode.label(),
                (probability * 100.0).round() as i64
            ),
            ModeSource::Decided { probability: None } => format!("{} · Jev", self.mode.label()),
            ModeSource::Writer => format!("{} · your choice", self.mode.label()),
        }
    }
}

/// A Review without a Focus is a request for an editor's comments; only a Focus can ask for
/// changes, so only a Focus needs a decision.
pub fn mode_without_decision(focus: &str) -> Option<ModeChoice> {
    js_trim(focus).is_empty().then_some(ModeChoice {
        mode: ReviewMode::Comment,
        source: ModeSource::Default,
    })
}

pub fn mode_decision_body(focus: &str) -> Value {
    json!({
        "model": DECISION_MODEL,
        "state": format!("A writer asked for a review of their whole document. Their request: {}", js_trim(focus)),
        "questions": {
            QUESTION: {
                "type": "choice",
                "instructions": "Does the writer want an editor's comments on the document, or changes made to its text?",
                "criteria": {
                    ReviewMode::Comment.key(): "Comments: feedback, critique or an opinion on what works and what to improve, without rewriting. Includes vague requests such as review, read, check or look at the document, the tone or the structure.",
                    ReviewMode::Revise.key(): "Changes: the request names edits to make, such as fix the spelling or grammar, shorten, cut, rewrite, simplify, translate, or change the tone of the text."
                }
            }
        }
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModeDecision {
    pub choice: ModeChoice,
    pub observation: CallObservation,
}

fn invalid(message: &str) -> ClientError {
    ClientError::InvalidOutput {
        message: message.to_owned(),
    }
}

pub fn parse_mode_decision(body: &Value) -> Result<ModeDecision, ClientError> {
    let answer = body
        .pointer(&format!("/answers/{QUESTION}"))
        .ok_or_else(|| invalid("The decision had no answer"))?;
    let mode = answer
        .get("choice")
        .and_then(Value::as_str)
        .and_then(ReviewMode::from_key)
        .ok_or_else(|| invalid("The decision chose an unknown mode"))?;
    let probability = answer
        .pointer(&format!("/probabilities/{}", mode.key()))
        .and_then(Value::as_f64)
        .map(|probability| probability.clamp(0.0, 1.0));
    Ok(ModeDecision {
        choice: ModeChoice {
            mode,
            source: ModeSource::Decided { probability },
        },
        observation: decision_observation(body),
    })
}

pub fn decision_observation(body: &Value) -> CallObservation {
    let text = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_owned);
    let usage = |key: &str| {
        body.pointer(&format!("/usage/{key}"))
            .and_then(Value::as_f64)
    };
    CallObservation {
        requested_model: DECISION_MODEL.to_owned(),
        served_model: text("model"),
        provider: text("provider"),
        finish_reason: body.get("answers").map(|_| "stop".to_owned()),
        input_tokens: usage("input_tokens"),
        output_tokens: usage("output_tokens"),
        cost_usd: usage("cost"),
        ..CallObservation::default()
    }
}

pub fn mode_task_label(focus: &str) -> String {
    format!("choose review mode → {}", js_trim(focus))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(choice: &str, probability: f64) -> Value {
        json!({
            "id": "dec-1",
            "model": "typesafe/jev-1.14-20261014",
            "provider": "TypeSafe",
            "answers": { "mode": {
                "type": "choice",
                "choice": choice,
                "probabilities": { choice: probability, "other": 1.0 - probability },
                "confidence": 0.8
            }},
            "usage": { "input_tokens": 210, "output_tokens": 0, "cost": 0.000_008_82 }
        })
    }

    #[test]
    fn no_focus_means_comments_without_a_decision() {
        let choice = mode_without_decision("  \n").unwrap();
        assert_eq!(choice.mode, ReviewMode::Comment);
        assert_eq!(choice.source, ModeSource::Default);
        assert_eq!(mode_without_decision("fix the spelling"), None);
    }

    #[test]
    fn asks_one_choice_question_between_comment_and_revise() {
        let body = mode_decision_body("  fix the spelling ");
        assert_eq!(body["model"], DECISION_MODEL);
        assert!(
            body["state"]
                .as_str()
                .unwrap()
                .ends_with("request: fix the spelling")
        );
        let question = &body["questions"]["mode"];
        assert_eq!(question["type"], "choice");
        let mut keys: Vec<&String> = question["criteria"].as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(keys, ["comment", "revise"]);
    }

    #[test]
    fn reads_the_chosen_mode_its_probability_and_the_cost() {
        let decision = parse_mode_decision(&answer("revise", 0.93)).unwrap();
        assert_eq!(decision.choice.mode, ReviewMode::Revise);
        assert_eq!(
            decision.choice.source,
            ModeSource::Decided {
                probability: Some(0.93)
            }
        );
        let observation = decision.observation;
        assert_eq!(observation.requested_model, DECISION_MODEL);
        assert_eq!(
            observation.served_model.as_deref(),
            Some("typesafe/jev-1.14-20261014")
        );
        assert_eq!(observation.provider.as_deref(), Some("TypeSafe"));
        assert_eq!(observation.input_tokens, Some(210.0));
        assert_eq!(observation.cost_usd, Some(0.000_008_82));
    }

    #[test]
    fn an_unknown_choice_or_a_missing_answer_is_invalid_output() {
        assert!(matches!(
            parse_mode_decision(&answer("translate", 0.9)),
            Err(ClientError::InvalidOutput { .. })
        ));
        assert!(matches!(
            parse_mode_decision(&json!({ "answers": {} })),
            Err(ClientError::InvalidOutput { .. })
        ));
    }

    #[test]
    fn the_chip_names_the_mode_and_who_chose_it() {
        let decided = parse_mode_decision(&answer("comment", 0.956))
            .unwrap()
            .choice;
        assert_eq!(decided.chip_label(), "Comment · Jev 96%");
        let unsure = ModeChoice {
            mode: ReviewMode::Revise,
            source: ModeSource::Decided { probability: None },
        };
        assert_eq!(unsure.chip_label(), "Revise · Jev");
        assert_eq!(
            ModeChoice::by_writer(ReviewMode::Revise).chip_label(),
            "Revise · your choice"
        );
        assert_eq!(
            mode_without_decision("").unwrap().chip_label(),
            "Comment · default"
        );
        assert_eq!(ReviewMode::Comment.other(), ReviewMode::Revise);
    }
}

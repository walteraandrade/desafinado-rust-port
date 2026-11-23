use std::ops::Deref;

use serde::{Deserialize, Serialize};

pub use crate::blocks::TargetSource;
use crate::diff::AdaptiveDiff;

pub type SuggestionId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionOrigin {
    Ask,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClientError {
    OutOfCredit { message: String },
    RateLimited { message: String },
    Timeout { message: String },
    Aborted { message: String },
    Network { message: String },
    InvalidOutput { message: String },
    Api { status: u16, message: String },
    Unknown { message: String },
}

impl ClientError {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::OutOfCredit { .. } => "out-of-credit",
            Self::RateLimited { .. } => "rate-limited",
            Self::Timeout { .. } => "timeout",
            Self::Aborted { .. } => "aborted",
            Self::Network { .. } => "network",
            Self::InvalidOutput { .. } => "invalid-output",
            Self::Api { .. } => "api",
            Self::Unknown { .. } => "unknown",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::OutOfCredit { message }
            | Self::RateLimited { message }
            | Self::Timeout { message }
            | Self::Aborted { message }
            | Self::Network { message }
            | Self::InvalidOutput { message }
            | Self::Api { message, .. }
            | Self::Unknown { message } => message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestionResult {
    pub replacement: String,
    pub rationale: String,
    pub unchanged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedEdit {
    pub block_id: String,
    pub replacement: String,
    pub rationale: String,
    pub block: crate::blocks::Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Instruction {
    QuickAction {
        id: String,
        label: String,
        text: String,
    },
    FreeText {
        text: String,
    },
}

impl Instruction {
    pub fn text(&self) -> &str {
        match self {
            Self::QuickAction { text, .. } | Self::FreeText { text } => text,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestionBase {
    pub id: SuggestionId,
    pub from: usize,
    pub to: usize,
    pub source: TargetSource,
    pub original: String,
    pub instruction: Instruction,
    pub sequence: usize,
    pub origin: SuggestionOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefineStep {
    pub instruction: Instruction,
    pub replacement: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefineState {
    Idle,
    Working {
        instruction: Instruction,
    },
    Failed {
        instruction: Instruction,
        error: ClientError,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReadySuggestion {
    pub base: SuggestionBase,
    pub replacement: String,
    pub rationale: String,
    pub diff: AdaptiveDiff,
    pub history: Vec<RefineStep>,
    pub shown: usize,
    pub refine: RefineState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedSuggestion {
    pub base: SuggestionBase,
    pub error: ClientError,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Suggestion {
    Pending(SuggestionBase),
    Ready(ReadySuggestion),
    Failed(FailedSuggestion),
}

impl Deref for ReadySuggestion {
    type Target = SuggestionBase;

    fn deref(&self) -> &SuggestionBase {
        &self.base
    }
}

impl Deref for FailedSuggestion {
    type Target = SuggestionBase;

    fn deref(&self) -> &SuggestionBase {
        &self.base
    }
}

impl Deref for Suggestion {
    type Target = SuggestionBase;

    fn deref(&self) -> &SuggestionBase {
        match self {
            Self::Pending(base) => base,
            Self::Ready(ready) => &ready.base,
            Self::Failed(failed) => &failed.base,
        }
    }
}

impl Suggestion {
    pub fn as_ready(&self) -> Option<&ReadySuggestion> {
        match self {
            Self::Ready(ready) => Some(ready),
            _ => None,
        }
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending(_))
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoticeKind {
    TextChanged,
    Replaced,
    NoChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuggestionNotice {
    pub serial: usize,
    pub kind: NoticeKind,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SuggestionsState {
    pub items: Vec<Suggestion>,
    pub next_sequence: usize,
    pub notice: Option<SuggestionNotice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestionDraft {
    pub id: SuggestionId,
    pub from: usize,
    pub to: usize,
    pub source: TargetSource,
    pub original: String,
    pub instruction: Instruction,
    pub origin: Option<SuggestionOrigin>,
}

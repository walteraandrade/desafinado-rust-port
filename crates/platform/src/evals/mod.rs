mod find_claude;
mod handlers;
mod results;
mod runner;
mod stage;

pub use find_claude::{
    CLAUDE_CLI_VARIABLE, candidate_paths, find_claude, find_claude_with_override,
    is_executable_file,
};
pub use handlers::{EvalDeps, EvalService, signal_name};
pub use results::{LastRun, NextRun, ResultsStore, skill_key_of};
pub use runner::{
    EvalEvent, EvalExit, EvalJob, EvalRunner, OutputStream, RunnerEnv, child_env, parse_version,
};
pub use stage::{STAGE_EXCLUDED, Stage, count_evals, stage_skill};

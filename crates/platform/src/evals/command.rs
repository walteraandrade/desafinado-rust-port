#[derive(Debug, Clone, PartialEq)]
pub struct EvalCommandInput {
    pub runs: u32,
    pub model: String,
    pub max_cost_usd: f64,
    pub json_file: String,
    pub output_dir: String,
    pub ablation_none: bool,
}

fn number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

pub fn build_eval_args(input: &EvalCommandInput) -> Vec<String> {
    let mut args: Vec<String> = [
        "plugin",
        "eval",
        ".",
        "--trust-plugin",
        "--no-publish",
        "--threshold",
        "0",
        "--max-cost-usd",
    ]
    .map(str::to_owned)
    .to_vec();
    args.push(number(input.max_cost_usd));
    args.push("--runs".into());
    args.push(input.runs.to_string());
    args.push(format!("--model={}", input.model));
    args.push("--json".into());
    args.push(input.json_file.clone());
    args.push("--output-dir".into());
    args.push(input.output_dir.clone());
    if input.ablation_none {
        args.push("--ablation".into());
        args.push("none".into());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(ablation: bool, cost: f64) -> EvalCommandInput {
        EvalCommandInput {
            runs: 3,
            model: "haiku".into(),
            max_cost_usd: cost,
            json_file: "/r/result.json".into(),
            output_dir: "/r/report".into(),
            ablation_none: ablation,
        }
    }

    #[test]
    fn builds_the_plugin_eval_command() {
        assert_eq!(
            build_eval_args(&input(false, 1.0)),
            [
                "plugin",
                "eval",
                ".",
                "--trust-plugin",
                "--no-publish",
                "--threshold",
                "0",
                "--max-cost-usd",
                "1",
                "--runs",
                "3",
                "--model=haiku",
                "--json",
                "/r/result.json",
                "--output-dir",
                "/r/report"
            ]
        );
    }

    #[test]
    fn adds_the_ablation_for_a_version_run_and_keeps_fractions() {
        let args = build_eval_args(&input(true, 0.25));
        assert_eq!(&args[args.len() - 2..], ["--ablation", "none"]);
        assert!(args.contains(&"0.25".to_owned()));
    }
}

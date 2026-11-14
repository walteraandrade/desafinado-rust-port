#[derive(Clone, Debug, PartialEq)]
pub struct EvalCommandInput {
    pub runs: f64,
    pub model: String,
    pub max_cost_usd: f64,
    pub json_file: String,
    pub output_dir: String,
    pub ablation_none: bool,
}

fn js_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
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
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    args.push(js_number(input.max_cost_usd));
    args.push("--runs".into());
    args.push(js_number(input.runs));
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

    fn input() -> EvalCommandInput {
        EvalCommandInput {
            runs: 2.0,
            model: "haiku".into(),
            max_cost_usd: 0.5,
            json_file: "/data/next/result.json".into(),
            output_dir: "/data/next/report".into(),
            ablation_none: false,
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn builds_the_fixed_command_line() {
        assert_eq!(
            build_eval_args(&input()),
            strings(&[
                "plugin",
                "eval",
                ".",
                "--trust-plugin",
                "--no-publish",
                "--threshold",
                "0",
                "--max-cost-usd",
                "0.5",
                "--runs",
                "2",
                "--model=haiku",
                "--json",
                "/data/next/result.json",
                "--output-dir",
                "/data/next/report",
            ])
        );
    }

    #[test]
    fn adds_ablation_none_last_for_a_version_baseline_and_nothing_else() {
        let mut expected = build_eval_args(&input());
        expected.extend(strings(&["--ablation", "none"]));
        assert_eq!(
            build_eval_args(&EvalCommandInput {
                ablation_none: true,
                ..input()
            }),
            expected
        );
    }

    #[test]
    fn keeps_the_model_in_one_argument() {
        let args = build_eval_args(&EvalCommandInput {
            model: "claude-haiku-4.5".into(),
            ..input()
        });
        assert!(args.contains(&"--model=claude-haiku-4.5".to_string()));
    }

    #[test]
    fn never_grants_tools_scaffolds_real_servers_or_turns_mocks_off() {
        let args = build_eval_args(&input()).join(" ");
        for flag in [
            "--allow-tools",
            "--scaffold",
            "--allow-real-servers",
            "--mocks",
        ] {
            assert!(!args.contains(flag), "{flag}");
        }
    }
}

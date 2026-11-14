const NAME_MAX_LENGTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillProblem {
    pub rule: &'static str,
    pub message: String,
}

fn problem(rule: &'static str, message: impl Into<String>) -> SkillProblem {
    SkillProblem {
        rule,
        message: message.into(),
    }
}

pub fn check_skill_name(name: &str) -> Vec<SkillProblem> {
    if name.trim().is_empty() {
        return vec![problem("name-empty", "name must not be empty")];
    }
    let normalized = name.trim();
    let length = normalized.chars().count();
    let mut problems = Vec::new();
    if length > NAME_MAX_LENGTH {
        problems.push(problem(
            "name-too-long",
            format!("name has {length} characters; the limit is {NAME_MAX_LENGTH}"),
        ));
    }
    if normalized != normalized.to_lowercase() {
        problems.push(problem("name-not-lowercase", "name must be lowercase"));
    }
    if normalized.starts_with('-') || normalized.ends_with('-') {
        problems.push(problem(
            "name-edge-hyphen",
            "name must not start or end with a hyphen",
        ));
    }
    if normalized.contains("--") {
        problems.push(problem(
            "name-double-hyphen",
            "name must not contain consecutive hyphens",
        ));
    }
    if normalized
        .chars()
        .any(|c| !(c.is_alphanumeric() || c == '-'))
    {
        problems.push(problem(
            "name-invalid-characters",
            "name may only contain letters, digits and hyphens",
        ));
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(name: &str) -> Vec<&'static str> {
        check_skill_name(name).iter().map(|p| p.rule).collect()
    }

    #[test]
    fn accepts_a_good_name_and_accents() {
        assert!(rules("pdf-processing").is_empty());
        assert!(rules("organização").is_empty());
    }

    #[test]
    fn says_which_rule_it_broke() {
        assert_eq!(rules(""), ["name-empty"]);
        assert_eq!(rules("Pdf"), ["name-not-lowercase"]);
        assert_eq!(rules("-pdf"), ["name-edge-hyphen"]);
        assert_eq!(rules("a--b"), ["name-double-hyphen"]);
        assert_eq!(rules("a b"), ["name-invalid-characters"]);
        assert_eq!(rules("a/b"), ["name-invalid-characters"]);
        assert_eq!(rules(&"a".repeat(65)), ["name-too-long"]);
    }
}

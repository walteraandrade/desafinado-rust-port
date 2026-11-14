const MAX_KEY_LENGTH: usize = 512;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("Invalid key")]
pub struct KeyInputError;

pub fn parse_key_input(value: &str) -> Result<String, KeyInputError> {
    let key = value.trim();
    let valid = !key.is_empty()
        && key.chars().count() <= MAX_KEY_LENGTH
        && key.bytes().all(|b| (0x21..=0x7e).contains(&b));
    if valid {
        Ok(key.to_owned())
    } else {
        Err(KeyInputError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_the_key_trimmed() {
        assert_eq!(parse_key_input("  sk-or-123 \n").unwrap(), "sk-or-123");
    }

    #[test]
    fn rejects_an_empty_key() {
        assert!(parse_key_input("").is_err());
        assert!(parse_key_input("   ").is_err());
    }

    #[test]
    fn rejects_whitespace_control_and_non_ascii_characters_inside_the_key() {
        for bad in ["sk or", "sk\tor", "sk\u{7}or", "sk-ór", "sk\nor"] {
            assert!(parse_key_input(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn rejects_a_key_over_512_characters_and_accepts_one_of_512() {
        assert!(parse_key_input(&"a".repeat(512)).is_ok());
        assert!(parse_key_input(&"a".repeat(513)).is_err());
    }

    #[test]
    fn does_not_echo_the_rejected_value_in_the_error() {
        let message = parse_key_input("secret value").unwrap_err().to_string();
        assert!(!message.contains("secret"));
    }
}

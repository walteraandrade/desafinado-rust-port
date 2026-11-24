use std::io::Read;

use flate2::read::GzDecoder;
use spellbook::Dictionary;

use super::{Issue, IssueKind, SpellerError, push_unique, words};

const AFF: &[u8] = include_bytes!("../../dictionaries/pt_BR.aff.gz");
const DIC: &[u8] = include_bytes!("../../dictionaries/pt_BR.dic.gz");
const MESSAGE: &str = "Not in the Portuguese dictionary.";

pub struct Portuguese {
    dictionary: Dictionary,
}

impl Portuguese {
    pub fn new() -> Result<Self, SpellerError> {
        let aff = unzip(AFF)?;
        let dic = unzip(DIC)?;
        let dictionary = Dictionary::new(strip_bom(&aff), strip_bom(&dic))
            .map_err(|error| SpellerError::Dictionary(error.to_string()))?;
        Ok(Self { dictionary })
    }

    pub fn knows(&self, word: &str) -> bool {
        self.dictionary.check(word)
            || (word.contains('-') && word.split('-').all(|part| self.dictionary.check(part)))
    }

    /// Spellbook also offers the word cut in two at odd places ("e xcessão"); a cut is kept
    /// only when each part is a word of two letters or more ("com certeza").
    fn sound_split(&self, suggestion: &str) -> bool {
        !suggestion.contains(' ')
            || suggestion
                .split(' ')
                .all(|part| part.chars().count() >= 2 && self.dictionary.check(part))
    }

    pub fn check(&self, text: &str) -> Vec<Issue> {
        words(text)
            .into_iter()
            .filter(|range| !self.knows(&text[range.clone()]))
            .map(|range| {
                let mut found = Vec::new();
                self.dictionary.suggest(&text[range.clone()], &mut found);
                let mut replacements = Vec::new();
                for suggestion in found.into_iter().filter(|s| self.sound_split(s)) {
                    push_unique(&mut replacements, suggestion);
                }
                Issue {
                    range,
                    kind: IssueKind::Spelling,
                    message: MESSAGE.to_string(),
                    replacements,
                }
            })
            .collect()
    }
}

fn unzip(bytes: &[u8]) -> Result<String, SpellerError> {
    let mut text = String::new();
    GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .map_err(|error| SpellerError::Dictionary(error.to_string()))?;
    Ok(text)
}

fn strip_bom(text: &str) -> &str {
    text.trim_start_matches('\u{feff}')
}

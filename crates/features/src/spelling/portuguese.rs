use std::io::Read;

use flate2::read::GzDecoder;
use spellbook::Dictionary;

use std::collections::HashMap;

use super::{Issue, IssueKind, SpellerError, offered, words};

const AFF: &[u8] = include_bytes!("../../dictionaries/pt_BR.aff.gz");
const DIC: &[u8] = include_bytes!("../../dictionaries/pt_BR.dic.gz");
const MESSAGE: &str = "Not in the Portuguese dictionary.";

pub struct Portuguese {
    dictionary: Dictionary,
    /// Replacements found so far, by word: finding them takes about 20 ms a word.
    replacements: HashMap<String, Vec<String>>,
}

impl Portuguese {
    pub fn new() -> Result<Self, SpellerError> {
        let aff = unzip(AFF)?;
        let dic = unzip(DIC)?;
        let dictionary = Dictionary::new(strip_bom(&aff), strip_bom(&dic))
            .map_err(|error| SpellerError::Dictionary(error.to_string()))?;
        Ok(Self {
            dictionary,
            replacements: HashMap::new(),
        })
    }

    /// The dictionary writes the apostrophe straight ("d'água"); text often has the curly one.
    pub fn knows(&self, word: &str) -> bool {
        let word = straight(word);
        self.dictionary.check(&word)
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

    /// The misspelled words of `text`. Words `borrowed` says belong to English ("deploy",
    /// "app") are taken as loanwords, not mistakes.
    pub fn check(&mut self, text: &str, borrowed: impl Fn(&str) -> bool) -> Vec<Issue> {
        let misspelled: Vec<_> = words(text)
            .into_iter()
            .filter(|range| {
                let word = &text[range.clone()];
                !self.knows(word) && !borrowed(word)
            })
            .collect();
        misspelled
            .into_iter()
            .map(|range| Issue {
                replacements: self.replacements_for(&text[range.clone()]),
                range,
                kind: IssueKind::Spelling,
                message: MESSAGE.to_string(),
            })
            .collect()
    }

    fn replacements_for(&mut self, word: &str) -> Vec<String> {
        if let Some(known) = self.replacements.get(word) {
            return known.clone();
        }
        let mut found = Vec::new();
        self.dictionary.suggest(&straight(word), &mut found);
        let list = offered(found.into_iter().filter(|s| self.sound_split(s)));
        self.replacements.insert(word.to_string(), list.clone());
        list
    }
}

fn straight(word: &str) -> String {
    word.replace('’', "'")
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

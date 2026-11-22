#[derive(Debug, Clone, Default)]
pub struct ElementScanner {
    depth: usize,
    in_string: bool,
    escaped: bool,
    array_depth: Option<usize>,
    closed: bool,
    key: Vec<u8>,
    last_key: Vec<u8>,
    capturing: bool,
    bare: bool,
    current: Vec<u8>,
}

impl ElementScanner {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn feed(&mut self, chunk: &str) -> Vec<String> {
        let mut elements = Vec::new();
        for byte in chunk.bytes() {
            self.push(byte, &mut elements);
        }
        elements
    }

    fn finish_element(&mut self, elements: &mut Vec<String>) {
        elements.push(String::from_utf8_lossy(&self.current).into_owned());
        self.current.clear();
        self.capturing = false;
        self.bare = false;
    }

    fn push_in_string(&mut self, byte: u8, elements: &mut Vec<String>) {
        if self.capturing {
            self.current.push(byte);
        }
        if self.escaped {
            self.escaped = false;
            if !self.capturing && self.array_depth.is_none() && self.depth == 1 {
                self.key.push(byte);
            }
            return;
        }
        match byte {
            b'\\' => {
                self.escaped = true;
                if !self.capturing && self.array_depth.is_none() && self.depth == 1 {
                    self.key.push(byte);
                }
            }
            b'"' => {
                self.in_string = false;
                if self.capturing {
                    if self.current.first() == Some(&b'"') {
                        self.finish_element(elements);
                    }
                } else if self.depth == 1 {
                    self.last_key = std::mem::take(&mut self.key);
                }
            }
            _ => {
                if !self.capturing && self.array_depth.is_none() && self.depth == 1 {
                    self.key.push(byte);
                }
            }
        }
    }

    fn push_capturing(&mut self, byte: u8, elements: &mut Vec<String>) -> bool {
        if self.bare {
            if byte == b',' || byte == b']' || byte.is_ascii_whitespace() {
                self.finish_element(elements);
                return false;
            }
            self.current.push(byte);
            return true;
        }
        self.current.push(byte);
        match byte {
            b'"' => self.in_string = true,
            b'{' | b'[' => self.depth += 1,
            b'}' | b']' => {
                self.depth = self.depth.saturating_sub(1);
                if Some(self.depth) == self.array_depth {
                    self.finish_element(elements);
                }
            }
            _ => {}
        }
        true
    }

    fn push(&mut self, byte: u8, elements: &mut Vec<String>) {
        if self.closed {
            return;
        }
        if self.in_string {
            self.push_in_string(byte, elements);
            return;
        }
        if self.capturing && self.push_capturing(byte, elements) {
            return;
        }
        let in_array = self.array_depth == Some(self.depth);
        match byte {
            b'"' => {
                self.in_string = true;
                self.key.clear();
                if in_array {
                    self.capturing = true;
                    self.current = vec![b'"'];
                }
            }
            b'{' => {
                self.depth += 1;
                if in_array {
                    self.capturing = true;
                    self.current = vec![b'{'];
                }
            }
            b'[' => {
                if self.array_depth.is_none()
                    && (self.depth == 0 || (self.depth == 1 && self.last_key == b"elements"))
                {
                    self.depth += 1;
                    self.array_depth = Some(self.depth);
                } else {
                    self.depth += 1;
                    if in_array {
                        self.capturing = true;
                        self.current = vec![b'['];
                    }
                }
            }
            b']' => {
                if in_array {
                    self.closed = true;
                }
                self.depth = self.depth.saturating_sub(1);
            }
            b'}' => self.depth = self.depth.saturating_sub(1),
            b',' | b':' => {}
            byte if byte.is_ascii_whitespace() => {}
            byte => {
                if in_array {
                    self.capturing = true;
                    self.bare = true;
                    self.current = vec![byte];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(chunks: &[&str]) -> Vec<String> {
        let mut scanner = ElementScanner::new();
        chunks
            .iter()
            .flat_map(|chunk| scanner.feed(chunk))
            .collect()
    }

    #[test]
    fn yields_each_element_as_soon_as_it_closes() {
        let mut scanner = ElementScanner::new();
        assert!(scanner.feed(r#"{"elements":[{"a":1"#).is_empty());
        assert_eq!(scanner.feed(r#"},{"a":"#), [r#"{"a":1}"#]);
        assert_eq!(scanner.feed(r"2}]}"), [r#"{"a":2}"#]);
        assert!(scanner.is_closed());
    }

    #[test]
    fn reads_a_bare_top_level_array() {
        assert_eq!(
            feed_all(&[r#"[{"a":1}, {"b":[1,2]}]"#]),
            [r#"{"a":1}"#, r#"{"b":[1,2]}"#]
        );
    }

    #[test]
    fn ignores_braces_and_quotes_inside_strings() {
        let element = r#"{"t":"a } ] \" , {"}"#;
        let text = format!(r#"{{"elements":[{element}]}}"#);
        assert_eq!(feed_all(&[&text]), [element]);
    }

    #[test]
    fn survives_chunks_split_at_every_byte() {
        let text = r#"{"elements":[{"blockId":"p1","replacement":"é \"x\" }","rationale":"r"},{"blockId":"p2","replacement":"b","rationale":"s"}]}"#;
        let bytes: Vec<String> = text
            .char_indices()
            .map(|(i, c)| text[i..i + c.len_utf8()].to_owned())
            .collect();
        let refs: Vec<&str> = bytes.iter().map(String::as_str).collect();
        assert_eq!(feed_all(&refs).len(), 2);
    }

    #[test]
    fn handles_scalar_elements_and_an_empty_list() {
        assert_eq!(
            feed_all(&[r#"{"elements":["a",12,true,null]}"#]),
            [r#""a""#, "12", "true", "null"]
        );
        assert!(feed_all(&[r#"{"elements":[]}"#]).is_empty());
    }

    #[test]
    fn ignores_other_keys_before_the_elements_array() {
        assert_eq!(
            feed_all(&[r#"{"note":"x","elements":[{"a":1}]}"#]),
            [r#"{"a":1}"#]
        );
    }

    #[test]
    fn ignores_text_after_the_array_closes() {
        assert_eq!(feed_all(&[r#"[{"a":1}] [{"b":2}]"#]), [r#"{"a":1}"#]);
    }
}

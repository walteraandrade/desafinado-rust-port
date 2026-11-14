#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyPress {
    pub key: String,
    pub meta: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl KeyPress {
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            ..Self::default()
        }
    }

    pub fn ctrl(key: &str) -> Self {
        Self {
            ctrl: true,
            ..Self::new(key)
        }
    }

    pub fn meta(key: &str) -> Self {
        Self {
            meta: true,
            ..Self::new(key)
        }
    }

    pub fn with_shift(self) -> Self {
        Self {
            shift: true,
            ..self
        }
    }

    pub fn with_alt(self) -> Self {
        Self { alt: true, ..self }
    }

    pub fn with_ctrl(self) -> Self {
        Self { ctrl: true, ..self }
    }

    pub fn with_meta(self) -> Self {
        Self { meta: true, ..self }
    }

    pub fn lower_key(&self) -> String {
        self.key.to_lowercase()
    }

    pub const fn platform_modifier(&self, apple: bool) -> bool {
        if apple {
            self.meta && !self.ctrl
        } else {
            self.ctrl && !self.meta
        }
    }
}

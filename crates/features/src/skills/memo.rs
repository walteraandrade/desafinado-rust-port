pub struct Memo<K, V> {
    slot: Option<(K, V)>,
}

impl<K: PartialEq, V> Memo<K, V> {
    pub const fn new() -> Self {
        Self { slot: None }
    }

    pub fn get_or_compute(&mut self, key: K, compute: impl FnOnce(&K) -> V) -> &V {
        let fresh = matches!(&self.slot, Some((held, _)) if *held == key);
        if !fresh {
            let value = compute(&key);
            self.slot = Some((key, value));
        }
        &self.slot.as_ref().expect("the slot is filled above").1
    }
}

impl<K: PartialEq, V> Default for Memo<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Memo;
    use std::cell::Cell;

    #[test]
    fn computes_once_while_the_key_stays_the_same() {
        let calls = Cell::new(0);
        let mut memo = Memo::new();
        let mut read = |key: u32| {
            *memo.get_or_compute(key, |key| {
                calls.set(calls.get() + 1);
                key * 10
            })
        };
        assert_eq!(read(1), 10);
        assert_eq!(read(1), 10);
        assert_eq!(calls.get(), 1);
        assert_eq!(read(2), 20);
        assert_eq!(calls.get(), 2);
        assert_eq!(read(1), 10);
        assert_eq!(calls.get(), 3);
    }
}

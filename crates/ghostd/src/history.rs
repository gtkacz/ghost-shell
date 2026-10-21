use std::collections::{BTreeMap, VecDeque};

const SECONDS_PER_MINUTE: usize = 60;

/// Samples to keep so that `history_minutes` of history fit at the 1 Hz sampling rate.
pub fn history_len(history_minutes: u32) -> usize {
    history_minutes as usize * SECONDS_PER_MINUTE
}

/// Fixed-capacity sample buffers keyed by series name, newest samples winning once full.
pub struct History {
    len: usize,
    series: BTreeMap<String, VecDeque<f32>>,
}

impl History {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            series: BTreeMap::new(),
        }
    }

    pub fn push(&mut self, key: &str, v: f32) {
        let len = self.len;
        let buffer = self
            .series
            .entry(key.to_owned())
            .or_insert_with(|| VecDeque::with_capacity(len));
        // Evict before pushing so a full buffer never grows past its allocation.
        if buffer.len() >= len {
            buffer.pop_front();
        }
        buffer.push_back(v);
    }

    /// Every series, oldest sample first.
    pub fn series(&self) -> BTreeMap<String, Vec<f32>> {
        self.series
            .iter()
            .map(|(key, buffer)| (key.clone(), buffer.iter().copied().collect()))
            .collect()
    }

    /// Changes the capacity; shrinking drops the oldest samples.
    pub fn resize(&mut self, len: usize) {
        self.len = len;
        for buffer in self.series.values_mut() {
            let excess = buffer.len().saturating_sub(len);
            buffer.drain(..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(len: usize, values: impl IntoIterator<Item = f32>) -> History {
        let mut history = History::new(len);
        for value in values {
            history.push("load", value);
        }
        history
    }

    #[test]
    fn resize_keeps_newest() {
        let mut history = filled(5, [1.0, 2.0, 3.0, 4.0, 5.0]);

        history.resize(2);
        assert_eq!(history.series()["load"], [4.0, 5.0]);

        history.push("load", 6.0);
        assert_eq!(history.series()["load"], [5.0, 6.0]);
    }

    #[test]
    fn resize_larger_keeps_everything_and_grows() {
        let mut history = filled(2, [1.0, 2.0]);

        history.resize(4);
        history.push("load", 3.0);
        history.push("load", 4.0);
        history.push("load", 5.0);

        assert_eq!(history.series()["load"], [2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn history_len_is_one_sample_per_second() {
        assert_eq!(history_len(5), 300);
    }
}

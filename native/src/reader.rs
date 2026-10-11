use std::{ops::Range, time::Duration};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug)]
pub struct Word {
    pub text: String,
    pub focal: Range<usize>,
    dwell_quarters: u64,
}

impl Word {
    fn new(text: &str, paragraph_end: bool) -> Self {
        let letters: Vec<_> = text.grapheme_indices(true).collect();
        let first = letters
            .iter()
            .position(|(_, g)| g.chars().any(char::is_alphanumeric))
            .unwrap_or(0);
        let last = letters
            .iter()
            .rposition(|(_, g)| g.chars().any(char::is_alphanumeric))
            .unwrap_or(letters.len().saturating_sub(1));
        let count = last + 1 - first;
        let offset = match count {
            0..=1 => 0,
            2..=5 => 1,
            6..=9 => 2,
            10..=13 => 3,
            _ => 4,
        };
        // Prefer the first vowel near the word's beginning; retain the length-based
        // anchor for consonant-only words and scripts without these Latin vowels.
        let offset = letters[first..=last]
            .iter()
            .take(4)
            .position(|(_, g)| {
                g.chars()
                    .flat_map(char::to_lowercase)
                    .any(|c| "aeiouyàáâãäåæèéêëìíîïòóôõöøœùúûüýÿ".contains(c))
            })
            .unwrap_or(offset);
        let (start, letter) = letters[first + offset];
        let end = text.trim_end_matches(['"', '\'', '”', '’', ')', ']']);
        let dwell_quarters = if paragraph_end {
            10
        } else if end.ends_with(['.', '!', '?', '…']) {
            8
        } else if end.ends_with([',', ';', ':', '—']) {
            6
        } else if letters.len() > 10 {
            5
        } else {
            4
        };
        Self {
            text: text.into(),
            focal: start..start + letter.len(),
            dwell_quarters,
        }
    }

    pub fn duration(&self, wpm: u32, natural: bool) -> Duration {
        let quarters = if natural { self.dwell_quarters } else { 4 };
        Duration::from_secs_f64(15. * quarters as f64 / wpm.clamp(100, 1200) as f64)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Paused,
    Playing,
    Finished,
}

pub struct Reader {
    pub words: Vec<Word>,
    pub index: usize,
    pub status: Status,
}

impl Reader {
    pub fn new(text: &str, position: usize) -> Self {
        let mut words = Vec::new();
        for paragraph in text.split('\n').filter(|p| !p.trim().is_empty()) {
            let tokens: Vec<_> = paragraph.split_whitespace().collect();
            for (index, token) in tokens.iter().enumerate() {
                words.push(Word::new(token, index + 1 == tokens.len()));
            }
        }
        let finished = !words.is_empty() && position >= words.len();
        let index = position.min(words.len().saturating_sub(1));
        Self {
            words,
            index,
            status: if finished {
                Status::Finished
            } else {
                Status::Paused
            },
        }
    }

    pub fn current(&self) -> Option<&Word> {
        self.words.get(self.index)
    }

    /// The persisted cursor uses len as the completed sentinel.
    pub fn position(&self) -> usize {
        if self.status == Status::Finished {
            self.words.len()
        } else {
            self.index
        }
    }

    pub fn toggle(&mut self) {
        if self.words.is_empty() {
            return;
        }
        match self.status {
            Status::Playing => self.status = Status::Paused,
            Status::Paused => self.status = Status::Playing,
            Status::Finished => {
                self.index = 0;
                self.status = Status::Playing;
            }
        }
    }

    /// Advance only after the displayed word has received its full dwell time.
    pub fn tick(&mut self) {
        if self.status != Status::Playing {
            return;
        }
        if self.index + 1 < self.words.len() {
            self.index += 1;
        } else {
            self.status = Status::Finished;
        }
    }

    pub fn seek(&mut self, index: usize) {
        self.index = index.min(self.words.len().saturating_sub(1));
        self.status = Status::Paused;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_holds_first_and_last_words_and_can_resume_or_replay() {
        let mut r = Reader::new("First middle last.", 0);
        r.toggle();
        assert_eq!(r.current().unwrap().text, "First");
        r.tick();
        r.toggle();
        r.tick();
        assert_eq!(r.current().unwrap().text, "middle");
        r.toggle();
        r.tick();
        assert_eq!(r.status, Status::Playing);
        assert_eq!(r.current().unwrap().text, "last.");
        r.tick();
        assert_eq!(r.status, Status::Finished);
        assert_eq!(r.position(), 3);
        let mut resumed = Reader::new("First middle last.", r.position());
        assert_eq!(resumed.status, Status::Finished);
        resumed.toggle();
        assert_eq!(resumed.index, 0);
        resumed.seek(usize::MAX);
        assert_eq!(resumed.index, 2);
        assert_eq!(resumed.status, Status::Paused);
        let mut empty = Reader::new(" \n\t", 200);
        empty.toggle();
        assert_eq!(empty.status, Status::Paused);
        assert!(empty.current().is_none());
    }

    #[test]
    fn unicode_focal_letters_skip_quotes_and_keep_graphemes_intact() {
        let r = Reader::new(
            "“café” A e\u{301}lan 👩🏽‍💻 Aufmerksamkeit „Überraschung!“ strengths nth 東京",
            0,
        );
        let focal: Vec<_> = r.words.iter().map(|w| &w.text[w.focal.clone()]).collect();
        assert_eq!(
            focal,
            ["a", "A", "e\u{301}", "👩🏽‍💻", "A", "Ü", "e", "t", "京"]
        );
        assert_eq!(r.words.len(), 9);
    }

    #[test]
    fn natural_pacing_includes_punctuation_and_paragraphs() {
        let r = Reader::new("Short, sentence. extraordinarily end\nNew paragraph!", 0);
        let milliseconds: Vec<_> = r
            .words
            .iter()
            .map(|w| w.duration(300, true).as_millis())
            .collect();
        assert_eq!(milliseconds, [300, 400, 250, 500, 200, 500]);
        assert_eq!(milliseconds.iter().sum::<u128>(), 2150);
        assert_eq!(
            r.words
                .iter()
                .map(|w| w.duration(300, false).as_millis())
                .sum::<u128>(),
            1200
        );
        assert_eq!(r.words[0].duration(1200, false).as_millis(), 50);
        assert_eq!(r.words[0].duration(0, false).as_millis(), 600);
    }
}

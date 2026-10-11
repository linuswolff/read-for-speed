use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const SAMPLE: &str = "Attention is a small act of generosity. When we give it to a sentence, a conversation, or a passing thought, we make room for something to become clear.\n\nReading is not a race against the page. It is a way of meeting an idea at just the right pace. Some words ask us to linger. Others carry us forward. The rhythm matters as much as the speed.\n\nTry resting your eyes on the colored letter. Let each word arrive in the same place. There is no need to chase it across the screen. Start slowly, find a comfortable rhythm, and then turn the pace up a little.\n\nPause whenever you like. Move back to revisit a thought. The best pace is the one that leaves you with something worth remembering.";

#[derive(Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: u64,
    pub title: String,
    pub text: String,
    pub source: String,
    pub position: usize,
    #[serde(skip)]
    pub word_count: usize,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub documents: Vec<Document>,
    pub active: Option<u64>,
    pub wpm: u32,
    pub font_size: f32,
    pub natural: bool,
    pub dark: bool,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            documents: vec![Document {
                id: 1,
                title: "The art of paying attention".into(),
                text: SAMPLE.into(),
                source: "Welcome to SPEED".into(),
                position: 0,
                word_count: SAMPLE.split_whitespace().count(),
            }],
            active: Some(1),
            wpm: 300,
            font_size: 76.,
            natural: true,
            dark: true,
        }
    }
}

impl Library {
    pub fn path() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("com.linuswolff.speed-native")
            .join("library.json")
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("Could not read the library: {e}")),
        };
        let mut library: Self = serde_json::from_str(&text).map_err(|e| {
            format!("Could not read the library: {e}. The original file has been kept.")
        })?;
        for document in &mut library.documents {
            document.word_count = document.text.split_whitespace().count();
        }
        library.wpm = library.wpm.clamp(100, 1200);
        library.font_size = if library.font_size.is_finite() {
            library.font_size.clamp(40., 112.)
        } else {
            76.
        };
        if !library
            .documents
            .iter()
            .any(|d| Some(d.id) == library.active)
        {
            library.active = library.documents.first().map(|d| d.id);
        }
        Ok(library)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            fs::create_dir_all(path.parent().ok_or("Missing storage directory")?)?;
            let temporary = path.with_extension("json.tmp");
            let file = fs::File::create(&temporary)?;
            serde_json::to_writer(&file, self)?;
            file.sync_all()?;
            fs::rename(temporary, path)?;
            Ok(())
        };
        write().map_err(|e| format!("Your changes could not be saved: {e}"))
    }

    pub fn active_document(&self) -> Option<&Document> {
        self.documents.iter().find(|d| Some(d.id) == self.active)
    }

    pub fn add(&mut self, text: &str, title: Option<String>, source: &str) -> Option<u64> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        if let Some(existing) = self.documents.iter().find(|d| d.text == text) {
            self.active = Some(existing.id);
            return self.active;
        }
        let id = self.documents.iter().map(|d| d.id).max().unwrap_or(0) + 1;
        let word_count = text.split_whitespace().count();
        let title = title.filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
            let words: Vec<_> = text.split_whitespace().take(7).collect();
            format!(
                "{}{}",
                words.join(" "),
                if word_count > 7 { "…" } else { "" }
            )
        });
        self.documents.insert(
            0,
            Document {
                id,
                title,
                text: text.into(),
                source: source.into(),
                position: 0,
                word_count,
            },
        );
        self.active = Some(id);
        Some(id)
    }
}

pub fn import_text(path: &Path) -> Result<String, String> {
    // Read bounded UTF-8 text, never binary PDF/EPUB data or an unbounded file.
    use std::io::Read;
    const MAX: u64 = 2 * 1024 * 1024;
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf") || e.eq_ignore_ascii_case("epub"))
    {
        return Err("PDF and EPUB are not supported yet. Choose a text or Markdown file.".into());
    }
    let file = fs::File::open(path).map_err(|e| format!("Could not open this file: {e}"))?;
    let mut bytes = Vec::new();
    file.take(MAX + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX {
        return Err("Choose a text file smaller than 2 MB.".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        "Choose a UTF-8 text or Markdown file. PDF and EPUB are not supported yet.".to_string()
    })?;
    if text.contains('\0') {
        return Err("This looks like a binary file. Choose a text or Markdown file.".into());
    }
    let text = text.trim_start_matches('\u{feff}').trim().to_owned();
    if text.is_empty() {
        return Err("This file does not contain any text.".into());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_round_trip_preserves_resume_preferences_and_deduplicates() {
        let path = std::env::temp_dir()
            .join(format!("speed-native-test-{}", std::process::id()))
            .join("library.json");
        let mut l = Library::default();
        assert!(l.dark);
        let id = l
            .add("one two three", Some("My reading".into()), "Clipboard")
            .unwrap();
        assert_eq!(l.add("  one two three  ", None, "Clipboard"), Some(id));
        assert_eq!(l.documents.len(), 2);
        l.documents[0].position = 2;
        l.wpm = 460;
        l.dark = false;
        l.natural = false;
        l.save(&path).unwrap();
        let saved = Library::load(&path).unwrap();
        assert_eq!(saved.active_document().unwrap().position, 2);
        assert_eq!(saved.active_document().unwrap().title, "My reading");
        assert_eq!(saved.wpm, 460);
        assert!(!saved.dark);
        assert!(!saved.natural);
        fs::write(&path, b"not json").unwrap();
        assert!(Library::load(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "not json");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_import_accepts_utf8_and_rejects_empty_binary_or_oversize_files() {
        let path =
            std::env::temp_dir().join(format!("speed-native-import-{}.txt", std::process::id()));
        fs::write(&path, "\u{feff}  Café\n\nÜber den Wolken.  ").unwrap();
        assert_eq!(import_text(&path).unwrap(), "Café\n\nÜber den Wolken.");
        for invalid in [b" \n ".as_slice(), b"text\0binary", &[0xff, 0xfe]] {
            fs::write(&path, invalid).unwrap();
            assert!(import_text(&path).is_err());
        }
        fs::write(&path, "a".repeat(2 * 1024 * 1024)).unwrap();
        assert_eq!(import_text(&path).unwrap().len(), 2 * 1024 * 1024);
        fs::write(&path, "a".repeat(2 * 1024 * 1024 + 1)).unwrap();
        assert!(import_text(&path).is_err());
        assert!(
            import_text(&path.with_extension("PDF"))
                .unwrap_err()
                .contains("PDF")
        );
        fs::remove_file(path).unwrap();
    }
}

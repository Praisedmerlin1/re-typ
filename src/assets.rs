use std::path::{Path, PathBuf};

/// Assigns stable, document-relative SVG names: `<stem>-1.svg`, `<stem>-2.svg`, ...
#[derive(Debug)]
pub struct AssetNamer {
    dir: PathBuf,
    stem: String,
    counter: usize,
}

impl AssetNamer {
    pub fn new(dir: PathBuf, stem: impl Into<String>) -> Self {
        Self {
            dir,
            stem: stem.into(),
            counter: 0,
        }
    }

    /// Preview the next asset path without consuming the number.
    pub fn peek(&self) -> PathBuf {
        self.path_for(self.counter + 1)
    }

    /// Reserve and return the next asset path in document order.
    pub fn commit(&mut self) -> PathBuf {
        self.counter += 1;
        self.path_for(self.counter)
    }

    fn path_for(&self, number: usize) -> PathBuf {
        self.dir.join(format!("{}-{}.svg", self.stem, number))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

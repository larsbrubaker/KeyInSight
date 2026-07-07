//! The storage seam: load/save one serialized document. Platform shells
//! provide the real backends (a file under the OS app-data directory on
//! native; localStorage on WASM); tests use [`MemoryStorage`].

use std::cell::RefCell;

pub trait Storage {
    /// The previously saved document, if any.
    fn load(&self) -> Option<String>;
    /// Persist the document. Implementations own their error reporting
    /// (the Swift app logged persistence failures and kept training).
    fn save(&self, contents: &str);
}

/// In-memory storage — the port's equivalent of `AppDatabase.inMemory()`.
#[derive(Default)]
pub struct MemoryStorage {
    contents: RefCell<Option<String>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }

    /// The current saved document (lets tests simulate reopening).
    pub fn snapshot(&self) -> Option<String> {
        self.contents.borrow().clone()
    }

    /// Pre-seed with a previously saved document (simulates reopen).
    pub fn with_contents(contents: String) -> Self {
        Self {
            contents: RefCell::new(Some(contents)),
        }
    }
}

impl Storage for MemoryStorage {
    fn load(&self) -> Option<String> {
        self.contents.borrow().clone()
    }

    fn save(&self, contents: &str) {
        *self.contents.borrow_mut() = Some(contents.to_string());
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Event {
    pub id: Uuid,
    pub timestamp: Instant,
    pub kind: EventKind,
    pub source: String,
    pub metadata: HashMap<String, String>,
}

impl Event {
    pub fn new(kind: EventKind, source: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Instant::now(),
            kind,
            source: source.into(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    // File System Events
    FileCreated {
        path: PathBuf,
    },
    FileModified {
        path: PathBuf,
    },
    FileDeleted {
        path: PathBuf,
    },
    FileRenamed {
        old_path: PathBuf,
        new_path: PathBuf,
    },

    // Window Events
    WindowCreated {
        hwnd: isize,
        title: String,
        process_id: u32,
    },
    WindowDestroyed {
        hwnd: isize,
    },
    WindowFocused {
        hwnd: isize,
        title: String,
    },

    // Process Events
    ProcessStarted {
        pid: u32,
        name: String,
        command_line: String,
    },
    ProcessStopped {
        pid: u32,
        name: String,
    },

    // Registry Events
    RegistryChanged {
        root: String,
        key: String,
        value_name: Option<String>,
        change_type: RegistryChangeType,
    },

    // Timer (for testing/scheduled tasks)
    TimerTick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryChangeType {
    Created,
    Modified,
    Deleted,
}

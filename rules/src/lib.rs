use engine_core::event::{Event, EventKind};

pub trait RuleMatcher: Send + Sync {
    fn matches(&self, event: &Event) -> bool;
    fn description(&self) -> String;
    fn clone_box(&self) -> Box<dyn RuleMatcher>;
}

impl Clone for Box<dyn RuleMatcher> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl std::fmt::Debug for Box<dyn RuleMatcher> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RuleMatcher({})", self.description())
    }
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub description: Option<String>,
    pub matcher: Box<dyn RuleMatcher>,
    pub enabled: bool,
}

impl Rule {
    pub fn new(name: impl Into<String>, matcher: Box<dyn RuleMatcher>) -> Self {
        Self {
            name: name.into(),
            description: None,
            matcher,
            enabled: true,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn matches(&self, event: &Event) -> bool {
        if !self.enabled {
            return false;
        }
        self.matcher.matches(event)
    }
}

#[derive(Debug, Clone)]
pub struct EventKindMatcher {
    pub kind: EventKind,
}

impl RuleMatcher for EventKindMatcher {
    fn matches(&self, event: &Event) -> bool {
        matches_event_kind(&self.kind, &event.kind)
    }

    fn description(&self) -> String {
        format!("Event kind matches {:?}", self.kind)
    }

    fn clone_box(&self) -> Box<dyn RuleMatcher> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct WindowMatcher {
    pub event_type: WindowEventType,
    pub title_contains: Option<String>,
    pub process_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowEventType {
    Focused,
    Unfocused,
    Created,
    Destroyed,
}

impl RuleMatcher for WindowMatcher {
    fn matches(&self, event: &Event) -> bool {
        let (event_type, title, process_name) = match &event.kind {
            EventKind::WindowFocused { hwnd: _, title } => {
                let process_name = event
                    .metadata
                    .get("process_name")
                    .cloned()
                    .unwrap_or_default();
                (WindowEventType::Focused, title.clone(), process_name)
            }
            EventKind::WindowUnfocused { hwnd: _, title } => {
                let process_name = event
                    .metadata
                    .get("process_name")
                    .cloned()
                    .unwrap_or_default();
                (WindowEventType::Unfocused, title.clone(), process_name)
            }
            EventKind::WindowCreated {
                hwnd: _,
                title,
                process_id: _,
            } => (WindowEventType::Created, title.clone(), String::new()),
            EventKind::WindowDestroyed { hwnd: _ } => {
                (WindowEventType::Destroyed, String::new(), String::new())
            }
            _ => return false,
        };

        if event_type != self.event_type {
            return false;
        }

        if let Some(ref title_filter) = self.title_contains {
            if !title.to_lowercase().contains(&title_filter.to_lowercase()) {
                return false;
            }
        }

        if let Some(ref process_filter) = self.process_name {
            if !process_name
                .to_lowercase()
                .contains(&process_filter.to_lowercase())
            {
                return false;
            }
        }

        true
    }

    fn description(&self) -> String {
        let mut desc = format!("Window {:?} event", self.event_type);
        if let Some(ref title) = self.title_contains {
            desc.push_str(&format!(" with title containing '{}'", title));
        }
        if let Some(ref process) = self.process_name {
            desc.push_str(&format!(" from process '{}'", process));
        }
        desc
    }

    fn clone_box(&self) -> Box<dyn RuleMatcher> {
        Box::new(self.clone())
    }
}

#[derive(Debug)]
pub struct FilePatternMatcher {
    pub event_type: FileEventType,
    pub path_pattern: Option<glob::Pattern>,
    pub file_pattern: Option<glob::Pattern>,
}

impl Clone for FilePatternMatcher {
    fn clone(&self) -> Self {
        Self {
            event_type: self.event_type,
            path_pattern: self
                .path_pattern
                .as_ref()
                .map(|p| glob::Pattern::new(p.as_str()).unwrap()),
            file_pattern: self
                .file_pattern
                .as_ref()
                .map(|p| glob::Pattern::new(p.as_str()).unwrap()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEventType {
    Created,
    Modified,
    Deleted,
    Any,
}

impl RuleMatcher for FilePatternMatcher {
    fn matches(&self, event: &Event) -> bool {
        let (event_type, path) = match &event.kind {
            EventKind::FileCreated { path } => (FileEventType::Created, path),
            EventKind::FileModified { path } => (FileEventType::Modified, path),
            EventKind::FileDeleted { path } => (FileEventType::Deleted, path),
            _ => return false,
        };

        if self.event_type != FileEventType::Any && self.event_type != event_type {
            return false;
        }

        if let Some(ref pattern) = self.path_pattern {
            if let Some(path_str) = path.to_str() {
                if !pattern.matches(path_str) {
                    return false;
                }
            }
        }

        if let Some(ref pattern) = self.file_pattern {
            if let Some(filename) = path.file_name() {
                if let Some(name) = filename.to_str() {
                    if !pattern.matches(name) {
                        return false;
                    }
                }
            }
        }

        true
    }

    fn description(&self) -> String {
        let mut desc = format!("File {:?} event", self.event_type);
        if let Some(ref pattern) = self.file_pattern {
            desc.push_str(&format!(" matching '{}'", pattern.as_str()));
        }
        desc
    }

    fn clone_box(&self) -> Box<dyn RuleMatcher> {
        Box::new(self.clone())
    }
}

impl FilePatternMatcher {
    pub fn created() -> Self {
        Self {
            event_type: FileEventType::Created,
            path_pattern: None,
            file_pattern: None,
        }
    }

    pub fn modified() -> Self {
        Self {
            event_type: FileEventType::Modified,
            path_pattern: None,
            file_pattern: None,
        }
    }

    pub fn deleted() -> Self {
        Self {
            event_type: FileEventType::Deleted,
            path_pattern: None,
            file_pattern: None,
        }
    }

    pub fn any() -> Self {
        Self {
            event_type: FileEventType::Any,
            path_pattern: None,
            file_pattern: None,
        }
    }

    pub fn with_path_pattern(mut self, pattern: &str) -> Result<Self, glob::PatternError> {
        self.path_pattern = Some(glob::Pattern::new(pattern)?);
        Ok(self)
    }

    pub fn with_file_pattern(mut self, pattern: &str) -> Result<Self, glob::PatternError> {
        self.file_pattern = Some(glob::Pattern::new(pattern)?);
        Ok(self)
    }
}

#[derive(Debug)]
pub struct CompositeMatcher {
    pub matchers: Vec<Box<dyn RuleMatcher>>,
    pub operator: MatchOperator,
}

impl Clone for CompositeMatcher {
    fn clone(&self) -> Self {
        Self {
            matchers: self.matchers.iter().map(|m| m.clone_box()).collect(),
            operator: self.operator,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MatchOperator {
    And,
    Or,
}

impl RuleMatcher for CompositeMatcher {
    fn matches(&self, event: &Event) -> bool {
        match self.operator {
            MatchOperator::And => self.matchers.iter().all(|m| m.matches(event)),
            MatchOperator::Or => self.matchers.iter().any(|m| m.matches(event)),
        }
    }

    fn description(&self) -> String {
        let op_str = match self.operator {
            MatchOperator::And => "AND",
            MatchOperator::Or => "OR",
        };
        format!(
            "({})",
            self.matchers
                .iter()
                .map(|m| m.description())
                .collect::<Vec<_>>()
                .join(&format!(" {} ", op_str))
        )
    }

    fn clone_box(&self) -> Box<dyn RuleMatcher> {
        Box::new(self.clone())
    }
}

fn matches_event_kind(expected: &EventKind, actual: &EventKind) -> bool {
    match (expected, actual) {
        (EventKind::TimerTick, EventKind::TimerTick) => true,
        (EventKind::FileCreated { path: p1 }, EventKind::FileCreated { path: p2 }) => p1 == p2,
        (EventKind::FileModified { path: p1 }, EventKind::FileModified { path: p2 }) => p1 == p2,
        (EventKind::FileDeleted { path: p1 }, EventKind::FileDeleted { path: p2 }) => p1 == p2,
        (EventKind::WindowFocused { .. }, EventKind::WindowFocused { .. }) => true,
        (EventKind::WindowUnfocused { .. }, EventKind::WindowUnfocused { .. }) => true,
        (EventKind::WindowCreated { .. }, EventKind::WindowCreated { .. }) => true,
        (EventKind::WindowDestroyed { .. }, EventKind::WindowDestroyed { .. }) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::event::Event;
    use std::path::PathBuf;

    #[test]
    fn test_simple_event_kind_matcher() {
        let matcher = EventKindMatcher {
            kind: EventKind::TimerTick,
        };
        let event = Event::new(EventKind::TimerTick, "test");
        assert!(matcher.matches(&event));

        let event2 = Event::new(
            EventKind::FileCreated {
                path: PathBuf::from("test.txt"),
            },
            "test",
        );
        assert!(!matcher.matches(&event2));
    }

    #[test]
    fn test_file_pattern_matcher() {
        let matcher = FilePatternMatcher::created()
            .with_file_pattern("*.txt")
            .unwrap();

        let event = Event::new(
            EventKind::FileCreated {
                path: PathBuf::from("/tmp/test.txt"),
            },
            "test",
        );
        assert!(matcher.matches(&event));

        let event2 = Event::new(
            EventKind::FileCreated {
                path: PathBuf::from("/tmp/test.log"),
            },
            "test",
        );
        assert!(!matcher.matches(&event2));

        let event3 = Event::new(
            EventKind::FileModified {
                path: PathBuf::from("/tmp/test.txt"),
            },
            "test",
        );
        assert!(!matcher.matches(&event3));
    }

    #[test]
    fn test_composite_matcher_and() {
        let matcher1 = Box::new(FilePatternMatcher::created()) as Box<dyn RuleMatcher>;
        let matcher2 = Box::new(
            FilePatternMatcher::created()
                .with_file_pattern("*.log")
                .unwrap(),
        );

        let composite = CompositeMatcher {
            matchers: vec![matcher1, matcher2],
            operator: MatchOperator::And,
        };

        let event = Event::new(
            EventKind::FileCreated {
                path: PathBuf::from("/tmp/app.log"),
            },
            "test",
        );
        assert!(composite.matches(&event));

        let event2 = Event::new(
            EventKind::FileCreated {
                path: PathBuf::from("/tmp/app.txt"),
            },
            "test",
        );
        assert!(!composite.matches(&event2));
    }

    #[test]
    fn test_rule_with_disabled() {
        let matcher = Box::new(EventKindMatcher {
            kind: EventKind::TimerTick,
        });
        let rule = Rule::new("test_rule", matcher).with_enabled(false);

        let event = Event::new(EventKind::TimerTick, "test");
        assert!(!rule.matches(&event));
    }

    #[test]
    fn test_rule_with_description() {
        let matcher = Box::new(EventKindMatcher {
            kind: EventKind::TimerTick,
        });
        let rule = Rule::new("test_rule", matcher).with_description("A test rule");

        assert_eq!(rule.description, Some("A test rule".to_string()));
    }
}

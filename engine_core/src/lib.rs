pub mod event;
pub mod plugin;

#[cfg(test)]
mod tests {
    use super::event::*;
    use std::path::PathBuf;

    #[test]
    fn test_event_creation() {
        let event = Event::new(
            EventKind::FileCreated {
                path: PathBuf::from("test.txt"),
            },
            "test_plugin",
        );

        assert_eq!(event.source, "test_plugin");
        match event.kind {
            EventKind::FileCreated { path } => {
                assert_eq!(path, PathBuf::from("test.txt"));
            }
            _ => panic!("Wrong event kind"),
        }
    }

    #[test]
    fn test_event_with_metadata() {
        let event = Event::new(EventKind::TimerTick, "test_plugin")
            .with_metadata("key1", "value1")
            .with_metadata("key2", "value2");

        assert_eq!(event.metadata.get("key1"), Some(&"value1".to_string()));
        assert_eq!(event.metadata.get("key2"), Some(&"value2".to_string()));
    }
}

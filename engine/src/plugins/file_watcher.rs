use async_trait::async_trait;
use engine_core::event::{Event, EventKind};
use engine_core::plugin::{EventEmitter, EventSourcePlugin, PluginError};
use notify::{Config, Event as NotifyEvent, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

pub struct FileWatcherPlugin {
    name: String,
    paths: Vec<PathBuf>,
    pattern: Option<String>,
    recursive: bool,
    watcher: Option<RecommendedWatcher>,
    is_running: bool,
}

impl FileWatcherPlugin {
    pub fn new(name: impl Into<String>, paths: Vec<PathBuf>) -> Self {
        Self {
            name: name.into(),
            paths,
            pattern: None,
            recursive: true,
            watcher: None,
            is_running: false,
        }
    }

    pub fn with_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = Some(pattern.into());
        self
    }

    pub fn with_recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    #[allow(dead_code)]
    fn should_emit_event(&self, path: &PathBuf) -> bool {
        if let Some(ref pattern) = self.pattern {
            if let Some(filename) = path.file_name() {
                if let Some(name) = filename.to_str() {
                    return glob::Pattern::new(pattern)
                        .map(|p| p.matches(name))
                        .unwrap_or(true);
                }
            }
            false
        } else {
            true
        }
    }

    #[allow(dead_code)]
    fn convert_notify_event(&self, event: NotifyEvent) -> Vec<Event> {
        let mut events = Vec::new();
        let source = self.name.clone();

        for path in event.paths {
            if !self.should_emit_event(&path) {
                continue;
            }

            let kind = match event.kind {
                notify::EventKind::Create(_) => {
                    EventKind::FileCreated { path: path.clone() }
                }
                notify::EventKind::Modify(_) => {
                    EventKind::FileModified { path: path.clone() }
                }
                notify::EventKind::Remove(_) => {
                    EventKind::FileDeleted { path: path.clone() }
                }
                _ => continue,
            };

            let metadata_event = Event::new(kind, &source)
                .with_metadata("watcher_path", path.to_string_lossy());
            
            events.push(metadata_event);
        }

        events
    }
}

#[async_trait]
impl EventSourcePlugin for FileWatcherPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&mut self, emitter: EventEmitter) -> Result<(), PluginError> {
        if self.is_running {
            return Ok(());
        }

        if self.paths.is_empty() {
            return Err(PluginError::Configuration(
                "No paths specified for file watcher".to_string(),
            ));
        }

        info!("Starting file watcher plugin: {}", self.name);

        let recursive_mode = if self.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        let plugin_name = self.name.clone();
        let pattern = self.pattern.clone();
        
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<NotifyEvent, notify::Error>| {
                match res {
                    Ok(event) => {
                        debug!("File system event: {:?}", event);
                        
                        for path in &event.paths {
                            if let Some(ref pat) = pattern {
                                if let Some(filename) = path.file_name() {
                                    if let Some(name) = filename.to_str() {
                                        if let Ok(glob_pattern) = glob::Pattern::new(pat) {
                                            if !glob_pattern.matches(name) {
                                                continue;
                                            }
                                        }
                                    }
                                }
                            }
                            
                            let kind = match event.kind {
                                notify::EventKind::Create(_) => {
                                    EventKind::FileCreated { path: path.clone() }
                                }
                                notify::EventKind::Modify(_) => {
                                    EventKind::FileModified { path: path.clone() }
                                }
                                notify::EventKind::Remove(_) => {
                                    EventKind::FileDeleted { path: path.clone() }
                                }
                                _ => continue,
                            };
                            
                            let event = Event::new(kind, &plugin_name)
                                .with_metadata("watcher_path", path.to_string_lossy());
                            
                            if let Err(e) = emitter.try_send(event) {
                                error!("Failed to send event: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("File watcher error: {}", e);
                    }
                }
            },
            Config::default(),
        )
        .map_err(|e| PluginError::Initialization(format!("Failed to create watcher: {}", e)))?;

        for path in &self.paths {
            if !path.exists() {
                warn!("Watch path does not exist: {:?}", path);
                continue;
            }

            watcher
                .watch(path, recursive_mode)
                .map_err(|e| PluginError::Initialization(format!("Failed to watch {:?}: {}", path, e)))?;
            
            info!("Watching path: {:?} (recursive: {})", path, self.recursive);
        }

        self.watcher = Some(watcher);
        self.is_running = true;
        
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        if let Some(mut watcher) = self.watcher.take() {
            info!("Stopping file watcher plugin: {}", self.name);
            
            for path in &self.paths {
                if let Err(e) = watcher.unwatch(path) {
                    warn!("Failed to unwatch {:?}: {}", path, e);
                }
            }
        }
        
        self.is_running = false;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.is_running
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_file_watcher_detects_create() {
        let temp_dir = TempDir::new().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        
        let mut plugin = FileWatcherPlugin::new("test_watcher", vec![temp_dir.path().to_path_buf()])
            .with_recursive(false);
        
        plugin.start(tx).await.expect("Failed to start plugin");
        
        // Give watcher time to initialize
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Create a file
        let test_file = temp_dir.path().join("test.txt");
        let mut file = File::create(&test_file).await.expect("Failed to create file");
        file.write_all(b"test").await.expect("Failed to write");
        drop(file);
        
        // Wait for event
        let result = timeout(Duration::from_secs(5), rx.recv()).await;
        
        plugin.stop().await.expect("Failed to stop plugin");
        
        assert!(result.is_ok(), "Should receive event within timeout");
        let event = result.unwrap().expect("Should receive event");
        
        match event.kind {
            EventKind::FileCreated { path } => {
                assert!(path.ends_with("test.txt"));
            }
            _ => panic!("Expected FileCreated event, got {:?}", event.kind),
        }
    }

    #[test]
    fn test_pattern_matching() {
        let temp_dir = TempDir::new().unwrap();
        let plugin = FileWatcherPlugin::new("test", vec![temp_dir.path().to_path_buf()])
            .with_pattern("*.txt");
        
        assert!(plugin.should_emit_event(&PathBuf::from("file.txt")));
        assert!(plugin.should_emit_event(&PathBuf::from("/path/to/file.txt")));
        assert!(!plugin.should_emit_event(&PathBuf::from("file.log")));
    }

    #[test]
    fn test_no_pattern_matches_all() {
        let temp_dir = TempDir::new().unwrap();
        let plugin = FileWatcherPlugin::new("test", vec![temp_dir.path().to_path_buf()]);
        
        assert!(plugin.should_emit_event(&PathBuf::from("file.txt")));
        assert!(plugin.should_emit_event(&PathBuf::from("file.log")));
        assert!(plugin.should_emit_event(&PathBuf::from("anything")));
    }
}

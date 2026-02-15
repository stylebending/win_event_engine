#[cfg(test)]
mod integration_tests {
    use crate::plugins::file_watcher::FileWatcherPlugin;
    use actions::{ActionExecutor, LogAction, LogLevel};
    use bus::create_event_bus;
    use engine_core::plugin::EventSourcePlugin;
    use rules::{FilePatternMatcher, Rule};
    use tokio::time::{Duration, sleep};

    #[tokio::test]
    async fn test_full_event_flow() {
        // Setup
        let (sender, mut receiver) = create_event_bus(100);
        let temp_dir = std::env::temp_dir().join("win_event_test");

        // Clean up and create test directory
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        // Start file watcher
        let mut watcher =
            FileWatcherPlugin::new("test", vec![temp_dir.clone()]).with_pattern("*.txt");
        watcher
            .start(sender)
            .await
            .expect("Failed to start watcher");

        // Setup rule
        let rule = Rule::new(
            "test_rule",
            Box::new(
                FilePatternMatcher::created()
                    .with_file_pattern("*.txt")
                    .unwrap(),
            ),
        );

        // Setup action executor
        let mut executor = ActionExecutor::new();
        executor.register("test_log", Box::new(LogAction::new("File created!")));

        // Give watcher time to initialize
        sleep(Duration::from_millis(200)).await;

        // Create test file
        let test_file = temp_dir.join("test.txt");
        std::fs::write(&test_file, "test content").expect("Failed to write test file");

        // Wait for event
        let timeout = tokio::time::timeout(Duration::from_secs(5), receiver.recv());
        let event = timeout
            .await
            .expect("Timeout waiting for event")
            .expect("Should receive event");

        // Verify
        assert!(rule.matches(&event), "Rule should match the event");

        // Execute action
        let result = executor.execute("test_log", &event);
        assert!(result.is_ok(), "Action should execute successfully");

        // Cleanup
        watcher.stop().await.ok();
        std::fs::remove_dir_all(&temp_dir).ok();

        println!("✓ Integration test passed!");
    }
}

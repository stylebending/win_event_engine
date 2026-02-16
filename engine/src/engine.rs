use crate::config::{ActionConfig, Config, RuleConfig, SourceConfig, SourceType, TriggerConfig};
use crate::plugins::file_watcher::FileWatcherPlugin;
use crate::plugins::process_monitor::ProcessMonitorPlugin;
use crate::plugins::registry_monitor::{RegistryMonitorPlugin, RegistryRoot};
use crate::plugins::window_watcher::WindowEventPlugin;
use actions::{Action, ActionExecutor, ExecuteAction, LogAction, LogLevel, PowerShellAction};
use bus::create_event_bus;
use engine_core::event::EventKind;
use engine_core::plugin::EventSourcePlugin;
use rules::{EventKindMatcher, FilePatternMatcher, Rule, RuleMatcher, WindowMatcher, WindowEventType};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};
use tracing::{error, info, warn};

pub struct Engine {
    config: Config,
    config_path: Option<PathBuf>,
    plugins: Vec<Box<dyn EventSourcePlugin>>,
    rules: Vec<Rule>,
    action_executor: ActionExecutor,
    event_sender: Option<mpsc::Sender<engine_core::event::Event>>,
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
    config_reload_rx: Option<mpsc::Receiver<()>>,
}

impl Engine {
    pub fn new(config: Config, config_path: Option<PathBuf>) -> Self {
        Self {
            config,
            config_path,
            plugins: Vec::new(),
            rules: Vec::new(),
            action_executor: ActionExecutor::new(),
            event_sender: None,
            shutdown_flag: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            config_reload_rx: None,
        }
    }

    pub fn take_config_reload_rx(&mut self) -> Option<mpsc::Receiver<()>> {
        self.config_reload_rx.take()
    }

    pub async fn initialize(&mut self) -> Result<(), EngineError> {
        info!("Initializing Windows Event Automation Engine");

        // Create event bus
        let (sender, mut receiver) = create_event_bus(self.config.engine.event_buffer_size);
        self.event_sender = Some(sender.clone());

        // Initialize plugins from configuration
        self.initialize_plugins(sender.clone()).await?;

        // Initialize rules from configuration
        self.initialize_rules();

        // Initialize actions from configuration
        self.initialize_actions();

        // Start event processing loop
        let rules = self.rules.clone();
        let action_executor = self.action_executor.clone();

        tokio::spawn(async move {
            info!("Event processing loop started");

            while let Some(event) = receiver.recv().await {
                tracing::debug!("Processing event: {:?} from {}", event.kind, event.source);

                for (idx, rule) in rules.iter().enumerate() {
                    if !rule.enabled {
                        continue;
                    }

                    if rule.matches(&event) {
                        info!("Rule '{}' matched event from {}", rule.name, event.source);

                        let action_name = format!("rule_{}_action", idx);
                        match action_executor.execute(&action_name, &event) {
                            Ok(result) => info!("Action executed successfully: {:?}", result),
                            Err(e) => error!("Action execution failed: {}", e),
                        }
                    }
                }
            }

            info!("Event processing loop stopped");
        });

        info!("Engine initialized successfully");
        Ok(())
    }

    async fn initialize_plugins(
        &mut self,
        sender: mpsc::Sender<engine_core::event::Event>,
    ) -> Result<(), EngineError> {
        for source_config in &self.config.sources {
            if !source_config.enabled {
                info!("Skipping disabled source: {}", source_config.name);
                continue;
            }

            match self.create_plugin(source_config, sender.clone()).await {
                Ok(plugin) => {
                    info!("Initialized plugin: {}", source_config.name);
                    self.plugins.push(plugin);
                }
                Err(e) => {
                    error!("Failed to initialize plugin {}: {}", source_config.name, e);
                }
            }
        }

        Ok(())
    }

    async fn create_plugin(
        &self,
        config: &SourceConfig,
        sender: mpsc::Sender<engine_core::event::Event>,
    ) -> Result<Box<dyn EventSourcePlugin>, EngineError> {
        match &config.source_type {
            SourceType::FileWatcher {
                paths,
                pattern,
                recursive,
            } => {
                let mut plugin =
                    FileWatcherPlugin::new(&config.name, paths.clone()).with_recursive(*recursive);

                if let Some(pattern) = pattern {
                    plugin = plugin.with_pattern(pattern);
                }

                plugin
                    .start(sender)
                    .await
                    .map_err(|e| EngineError::PluginInit(config.name.clone(), e.to_string()))?;

                Ok(Box::new(plugin))
            }
            SourceType::WindowWatcher {
                title_pattern,
                process_pattern,
            } => {
                let mut plugin = WindowEventPlugin::new(&config.name);

                if let Some(title) = title_pattern {
                    plugin = plugin.with_title_filter(title);
                }

                if let Some(process) = process_pattern {
                    plugin = plugin.with_process_filter(process);
                }

                plugin
                    .start(sender)
                    .await
                    .map_err(|e| EngineError::PluginInit(config.name.clone(), e.to_string()))?;

                Ok(Box::new(plugin))
            }
            SourceType::ProcessMonitor {
                process_name,
                poll_interval_seconds,
            } => {
                let mut plugin = ProcessMonitorPlugin::new(&config.name)
                    .with_poll_interval(*poll_interval_seconds);

                if let Some(name) = process_name {
                    plugin = plugin.with_name_filter(name);
                }

                plugin
                    .start(sender)
                    .await
                    .map_err(|e| EngineError::PluginInit(config.name.clone(), e.to_string()))?;

                Ok(Box::new(plugin))
            }
            SourceType::RegistryMonitor {
                root,
                key,
                recursive,
            } => {
                let root_enum = match root.as_str() {
                    "HKLM" => RegistryRoot::HKEY_LOCAL_MACHINE,
                    "HKCU" => RegistryRoot::HKEY_CURRENT_USER,
                    "HKU" => RegistryRoot::HKEY_USERS,
                    "HKCC" => RegistryRoot::HKEY_CURRENT_CONFIG,
                    _ => {
                        return Err(EngineError::Config(format!(
                            "Invalid registry root: {}",
                            root
                        )));
                    }
                };

                let mut plugin = if *recursive {
                    RegistryMonitorPlugin::new(&config.name).watch_key_recursive(root_enum, key)
                } else {
                    RegistryMonitorPlugin::new(&config.name).watch_key(root_enum, key)
                };

                plugin
                    .start(sender)
                    .await
                    .map_err(|e| EngineError::PluginInit(config.name.clone(), e.to_string()))?;

                Ok(Box::new(plugin))
            }
        }
    }

    fn initialize_rules(&mut self) {
        for rule_config in &self.config.rules {
            if !rule_config.enabled {
                continue;
            }

            match self.create_rule(rule_config) {
                Ok(rule) => {
                    info!("Loaded rule: {}", rule.name);
                    self.rules.push(rule);
                }
                Err(e) => {
                    error!("Failed to create rule {}: {}", rule_config.name, e);
                }
            }
        }
    }

    fn create_rule(&self, config: &RuleConfig) -> Result<Rule, EngineError> {
        let matcher: Box<dyn RuleMatcher> = match &config.trigger {
            TriggerConfig::FileCreated { pattern } => {
                let mut matcher = FilePatternMatcher::created();
                if let Some(pat) = pattern {
                    matcher = matcher
                        .with_file_pattern(pat)
                        .map_err(|e| EngineError::Config(format!("Invalid pattern: {}", e)))?;
                }
                Box::new(matcher)
            }
            TriggerConfig::FileModified { pattern } => {
                let mut matcher = FilePatternMatcher::modified();
                if let Some(pat) = pattern {
                    matcher = matcher
                        .with_file_pattern(pat)
                        .map_err(|e| EngineError::Config(format!("Invalid pattern: {}", e)))?;
                }
                Box::new(matcher)
            }
            TriggerConfig::FileDeleted { pattern } => {
                let mut matcher = FilePatternMatcher::deleted();
                if let Some(pat) = pattern {
                    matcher = matcher
                        .with_file_pattern(pat)
                        .map_err(|e| EngineError::Config(format!("Invalid pattern: {}", e)))?;
                }
                        Box::new(matcher)
            }
            TriggerConfig::WindowFocused {
                title_contains,
                process_name,
            } => Box::new(WindowMatcher {
                event_type: WindowEventType::Focused,
                title_contains: title_contains.clone(),
                process_name: process_name.clone(),
            }),
            TriggerConfig::WindowUnfocused {
                title_contains,
                process_name,
            } => Box::new(WindowMatcher {
                event_type: WindowEventType::Unfocused,
                title_contains: title_contains.clone(),
                process_name: process_name.clone(),
            }),
            TriggerConfig::WindowCreated => Box::new(EventKindMatcher {
                kind: EventKind::WindowCreated {
                    hwnd: 0,
                    title: String::new(),
                    process_id: 0,
                },
            }),
            TriggerConfig::ProcessStarted { process_name: _ } => Box::new(EventKindMatcher {
                kind: EventKind::ProcessStarted {
                    pid: 0,
                    name: String::new(),
                    command_line: String::new(),
                },
            }),
            TriggerConfig::ProcessStopped { process_name: _ } => Box::new(EventKindMatcher {
                kind: EventKind::ProcessStopped {
                    pid: 0,
                    name: String::new(),
                },
            }),
            TriggerConfig::RegistryChanged { value_name: _ } => Box::new(EventKindMatcher {
                kind: EventKind::RegistryChanged {
                    root: String::new(),
                    key: String::new(),
                    value_name: None,
                    change_type: engine_core::event::RegistryChangeType::Modified,
                },
            }),
            TriggerConfig::Timer {
                interval_seconds: _,
            } => Box::new(EventKindMatcher {
                kind: EventKind::TimerTick,
            }),
        };

        let mut rule = Rule::new(&config.name, matcher);

        if let Some(desc) = &config.description {
            rule = rule.with_description(desc);
        }

        Ok(rule.with_enabled(config.enabled))
    }

    fn initialize_actions(&mut self) {
        // Register actions from rule configurations
        for (idx, rule_config) in self.config.rules.iter().enumerate() {
            let action_name = format!("rule_{}_action", idx);
            let action: Box<dyn Action> = match &rule_config.action {
                ActionConfig::Execute {
                    command,
                    args,
                    working_dir,
                } => {
                    let mut exec = ExecuteAction::new(command).with_args(args.clone());
                    if let Some(dir) = working_dir {
                        exec = exec.with_working_dir(dir.clone());
                    }
                    Box::new(exec)
                }
                ActionConfig::PowerShell {
                    script,
                    working_dir,
                } => {
                    let mut ps = PowerShellAction::new(script);
                    if let Some(dir) = working_dir {
                        ps = ps.with_working_dir(dir.clone());
                    }
                    Box::new(ps)
                }
                ActionConfig::Log { message, level } => {
                    let log_level = match level.as_str() {
                        "debug" => LogLevel::Debug,
                        "info" => LogLevel::Info,
                        "warn" => LogLevel::Warn,
                        "error" => LogLevel::Error,
                        _ => LogLevel::Info,
                    };
                    Box::new(LogAction::new(message).with_level(log_level))
                }
                ActionConfig::Notify { title, message } => {
                    // For now, use log action as a placeholder for notifications
                    Box::new(LogAction::new(format!("{}: {}", title, message)))
                }
                ActionConfig::HttpRequest { url, .. } => {
                    // HTTP requests would need additional implementation
                    Box::new(LogAction::new(format!("HTTP request to: {}", url)))
                }
                ActionConfig::Media { command } => {
                    let script = match command.as_str() {
                        "play" => {
                            r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class MediaKeys {
    [DllImport("user32.dll", CharSet = CharSet.Auto, CallingConvention = CallingConvention.StdCall)]
    public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
    public const byte VK_MEDIA_PLAY_PAUSE = 0xB3;
    public static void PlayPause() {
        keybd_event(VK_MEDIA_PLAY_PAUSE, 0, 0, UIntPtr.Zero);
        keybd_event(VK_MEDIA_PLAY_PAUSE, 0, 2, UIntPtr.Zero);
    }
}
"@
[MediaKeys]::PlayPause()
"#
                        }
                        "pause" => {
                            r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class MediaKeys {
    [DllImport("user32.dll", CharSet = CharSet.Auto, CallingConvention = CallingConvention.StdCall)]
    public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
    public const byte VK_MEDIA_PLAY_PAUSE = 0xB3;
    public static void PlayPause() {
        keybd_event(VK_MEDIA_PLAY_PAUSE, 0, 0, UIntPtr.Zero);
        keybd_event(VK_MEDIA_PLAY_PAUSE, 0, 2, UIntPtr.Zero);
    }
}
"@
[MediaKeys]::PlayPause()
"#
                        }
                        "toggle" => {
                            r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class MediaKeys {
    [DllImport("user32.dll", CharSet = CharSet.Auto, CallingConvention = CallingConvention.StdCall)]
    public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
    public const byte VK_MEDIA_PLAY_PAUSE = 0xB3;
    public static void PlayPause() {
        keybd_event(VK_MEDIA_PLAY_PAUSE, 0, 0, UIntPtr.Zero);
        keybd_event(VK_MEDIA_PLAY_PAUSE, 0, 2, UIntPtr.Zero);
    }
}
"@
[MediaKeys]::PlayPause()
"#
                        }
                        _ => {
                            r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class MediaKeys {
    [DllImport("user32.dll", CharSet = CharSet.Auto, CallingConvention = CallingConvention.StdCall)]
    public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
    public const byte VK_MEDIA_PLAY_PAUSE = 0xB3;
    public static void PlayPause() {
        keybd_event(VK_MEDIA_PLAY_PAUSE, 0, 0, UIntPtr.Zero);
        keybd_event(VK_MEDIA_PLAY_PAUSE, 0, 2, UIntPtr.Zero);
    }
}
"@
[MediaKeys]::PlayPause()
"#
                        }
                    };
                    Box::new(PowerShellAction::new(script))
                }
            };

            self.action_executor.register(action_name, action);
        }
    }

    pub async fn shutdown(&mut self) {
        info!("Shutting down engine");

        for plugin in &mut self.plugins {
            if let Err(e) = plugin.stop().await {
                error!("Error stopping plugin: {}", e);
            }
        }

        info!("Engine shutdown complete");
    }

    pub fn get_status(&self) -> EngineStatus {
        EngineStatus {
            active_plugins: self.plugins.len(),
            active_rules: self.rules.len(),
        }
    }

    pub async fn reload(&mut self, new_config: Config) -> Result<(), EngineError> {
        info!("Starting full config reload");

        if let Err(e) = new_config.validate() {
            warn!(
                "New configuration validation failed: {}, keeping current config",
                e
            );
            return Err(EngineError::Config(e.to_string()));
        }

        info!("Stopping all plugins for reload");
        for plugin in &mut self.plugins {
            if let Err(e) = plugin.stop().await {
                error!("Error stopping plugin during reload: {}", e);
            }
        }
        self.plugins.clear();
        self.rules.clear();

        self.config = new_config;

        if let Some(sender) = &self.event_sender {
            self.initialize_plugins(sender.clone()).await?;
        }

        self.initialize_rules();
        self.initialize_actions();

        if let Some(_sender) = &self.event_sender {
            let rules = self.rules.clone();
            let action_executor = self.action_executor.clone();
            let mut receiver = bus::create_event_bus(self.config.engine.event_buffer_size).1;

            tokio::spawn(async move {
                while let Some(event) = receiver.recv().await {
                    tracing::debug!("Processing event: {:?} from {}", event.kind, event.source);

                    for (idx, rule) in rules.iter().enumerate() {
                        if !rule.enabled {
                            continue;
                        }

                        if rule.matches(&event) {
                            info!("Rule '{}' matched event from {}", rule.name, event.source);

                            let action_name = format!("rule_{}_action", idx);
                            match action_executor.execute(&action_name, &event) {
                                Ok(result) => info!("Action executed successfully: {:?}", result),
                                Err(e) => error!("Action execution failed: {}", e),
                            }
                        }
                    }
                }
            });
        }

        let status = self.get_status();
        info!(
            "Config reload complete: {} plugins, {} rules",
            status.active_plugins, status.active_rules
        );

        Ok(())
    }

    pub async fn watch_config(&mut self) {
        let config_path = match &self.config_path {
            Some(p) => p.clone(),
            None => {
                info!("No config path configured, skipping config watcher");
                return;
            }
        };

        let (tx, rx) = mpsc::channel(10);
        self.config_reload_rx = Some(rx);

        let shutdown_flag = self.shutdown_flag.clone();

        tokio::spawn(async move {
            use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};

            let (notify_tx, mut notify_rx) = mpsc::channel(100);

            let mut watcher: RecommendedWatcher = match Watcher::new(
                move |res: Result<notify::Event, notify::Error>| {
                    if let Ok(event) = res {
                        let _ = notify_tx.blocking_send(event);
                    }
                },
                NotifyConfig::default(),
            ) {
                Ok(w) => w,
                Err(e) => {
                    error!("Failed to create config watcher: {}", e);
                    return;
                }
            };

            let watch_path = if config_path.is_dir() {
                config_path.clone()
            } else {
                config_path.parent().unwrap_or(&config_path).to_path_buf()
            };

            if let Err(e) = watcher.watch(&watch_path, RecursiveMode::Recursive) {
                error!("Failed to watch config path: {}", e);
                return;
            }

            info!("Config watcher started for: {:?}", watch_path);
            let mut last_reload = std::time::Instant::now();
            let debounce_duration = Duration::from_millis(500);

            while !shutdown_flag.load(std::sync::atomic::Ordering::Relaxed) {
                match timeout(Duration::from_millis(250), notify_rx.recv()).await {
                    Ok(Some(event)) => {
                        if let notify::EventKind::Modify(_) | notify::EventKind::Create(_) =
                            event.kind
                        {
                            if last_reload.elapsed() < debounce_duration {
                                continue;
                            }

                            let paths: Vec<_> = event
                                .paths
                                .iter()
                                .filter(|p| p.extension().map(|e| e == "toml").unwrap_or(false))
                                .collect();

                            if paths.is_empty() {
                                continue;
                            }

                            info!("Config change detected, signaling reload...");
                            let _ = tx.send(()).await;
                            last_reload = std::time::Instant::now();
                        }
                    }
                    Ok(None) => break,
                    Err(_) => continue,
                }
            }

            info!("Config watcher stopped");
        });
    }

    pub fn shutdown_flag(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.shutdown_flag.clone()
    }
}

#[derive(Debug, Clone)]
pub struct EngineStatus {
    pub active_plugins: usize,
    pub active_rules: usize,
}

#[derive(Debug, Clone)]
pub enum EngineError {
    Config(String),
    PluginInit(String, String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Config(msg) => write!(f, "Configuration error: {}", msg),
            EngineError::PluginInit(name, msg) => {
                write!(f, "Plugin '{}' initialization error: {}", name, msg)
            }
        }
    }
}

impl std::error::Error for EngineError {}

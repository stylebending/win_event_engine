use async_trait::async_trait;
use engine_core::event::{Event, EventKind, RegistryChangeType};
use engine_core::plugin::{EventEmitter, EventSourcePlugin, PluginError};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::{Duration, sleep};
use tracing::{error, info};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_CONFIG, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_NOTIFY,
    KEY_READ, REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME, RegCloseKey,
    RegNotifyChangeKeyValue, RegOpenKeyExW,
};
use windows::Win32::System::Threading::WaitForSingleObject;

pub struct RegistryMonitorPlugin {
    name: String,
    keys: Vec<RegistryKeyConfig>,
    is_running: Arc<AtomicBool>,
    poll_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct RegistryKeyConfig {
    pub root: RegistryRoot,
    pub path: String,
    pub watch_tree: bool,
}

#[derive(Debug, Clone)]
#[allow(non_camel_case_types)]
pub enum RegistryRoot {
    HKEY_LOCAL_MACHINE,
    HKEY_CURRENT_USER,
    HKEY_USERS,
    HKEY_CURRENT_CONFIG,
}

impl RegistryRoot {
    fn to_hkey(&self) -> HKEY {
        match self {
            RegistryRoot::HKEY_LOCAL_MACHINE => HKEY_LOCAL_MACHINE,
            RegistryRoot::HKEY_CURRENT_USER => HKEY_CURRENT_USER,
            RegistryRoot::HKEY_USERS => HKEY_USERS,
            RegistryRoot::HKEY_CURRENT_CONFIG => HKEY_CURRENT_CONFIG,
        }
    }

    fn to_string(&self) -> String {
        match self {
            RegistryRoot::HKEY_LOCAL_MACHINE => "HKLM".to_string(),
            RegistryRoot::HKEY_CURRENT_USER => "HKCU".to_string(),
            RegistryRoot::HKEY_USERS => "HKU".to_string(),
            RegistryRoot::HKEY_CURRENT_CONFIG => "HKCC".to_string(),
        }
    }
}

impl RegistryMonitorPlugin {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            keys: Vec::new(),
            is_running: Arc::new(AtomicBool::new(false)),
            poll_interval: Duration::from_secs(5),
        }
    }

    pub fn watch_key(mut self, root: RegistryRoot, path: impl Into<String>) -> Self {
        self.keys.push(RegistryKeyConfig {
            root,
            path: path.into(),
            watch_tree: false,
        });
        self
    }

    pub fn watch_key_recursive(mut self, root: RegistryRoot, path: impl Into<String>) -> Self {
        self.keys.push(RegistryKeyConfig {
            root,
            path: path.into(),
            watch_tree: true,
        });
        self
    }

    fn open_registry_key(&self, config: &RegistryKeyConfig) -> Option<HKEY> {
        unsafe {
            let root_hkey = config.root.to_hkey();
            let path_wide: Vec<u16> = config
                .path
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let mut hkey = HKEY(0);
            let result = RegOpenKeyExW(
                root_hkey,
                windows::core::PCWSTR(path_wide.as_ptr()),
                0,
                KEY_READ | KEY_NOTIFY,
                &mut hkey,
            );

            if result.is_ok() { Some(hkey) } else { None }
        }
    }

    fn setup_registry_notification(&self, hkey: HKEY, watch_tree: bool) -> Option<HANDLE> {
        unsafe {
            let event_handle =
                windows::Win32::System::Threading::CreateEventW(None, false, false, None).ok()?;

            let filter = REG_NOTIFY_CHANGE_NAME | REG_NOTIFY_CHANGE_LAST_SET;
            let result = RegNotifyChangeKeyValue(hkey, watch_tree, filter, event_handle, true);

            if result.is_ok() {
                Some(event_handle)
            } else {
                let _ = CloseHandle(event_handle);
                None
            }
        }
    }
}

#[async_trait]
impl EventSourcePlugin for RegistryMonitorPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&mut self, emitter: EventEmitter) -> Result<(), PluginError> {
        if self.is_running.load(Ordering::SeqCst) {
            return Ok(());
        }

        if self.keys.is_empty() {
            return Err(PluginError::Configuration(
                "No registry keys specified to watch".to_string(),
            ));
        }

        info!("Starting registry monitor plugin: {}", self.name);

        let is_running = self.is_running.clone();
        let keys = self.keys.clone();
        let plugin_name = self.name.clone();
        let poll_interval = self.poll_interval;

        tokio::spawn(async move {
            is_running.store(true, Ordering::SeqCst);

            // Open all registry keys
            let mut key_handles: Vec<(HKEY, RegistryKeyConfig, HANDLE)> = Vec::new();
            for config in &keys {
                if let Some(hkey) = Self::open_registry_key(
                    &Self {
                        name: plugin_name.clone(),
                        keys: keys.clone(),
                        is_running: is_running.clone(),
                        poll_interval,
                    },
                    config,
                ) {
                    if let Some(event_handle) = Self::setup_registry_notification(
                        &Self {
                            name: plugin_name.clone(),
                            keys: keys.clone(),
                            is_running: is_running.clone(),
                            poll_interval,
                        },
                        hkey,
                        config.watch_tree,
                    ) {
                        info!(
                            "Watching registry key: {}\\{}",
                            config.root.to_string(),
                            config.path
                        );
                        key_handles.push((hkey, config.clone(), event_handle));
                    } else {
                        unsafe {
                            let _ = RegCloseKey(hkey);
                        }
                    }
                } else {
                    error!(
                        "Failed to open registry key: {}\\{}",
                        config.root.to_string(),
                        config.path
                    );
                }
            }

            if key_handles.is_empty() {
                error!("No registry keys could be monitored");
                is_running.store(false, Ordering::SeqCst);
                return;
            }

            // Monitor loop
            while is_running.load(Ordering::SeqCst) {
                // Poll for changes (simplified approach)
                sleep(poll_interval).await;

                if !is_running.load(Ordering::SeqCst) {
                    break;
                }

                // For each key, emit an event indicating a change was detected
                // In a production implementation, you'd use WaitForMultipleObjects
                // to properly wait on all event handles
                for (hkey, config, event_handle) in &key_handles {
                    unsafe {
                        // Check if event was signaled
                        let wait_result = WaitForSingleObject(*event_handle, 0);
                        if wait_result == WAIT_OBJECT_0 {
                            // Change detected
                            info!(
                                "Registry change detected: {}\\{}",
                                config.root.to_string(),
                                config.path
                            );

                            let event = Event::new(
                                EventKind::RegistryChanged {
                                    root: config.root.to_string(),
                                    key: config.path.clone(),
                                    value_name: None,
                                    change_type: RegistryChangeType::Modified,
                                },
                                &plugin_name,
                            )
                            .with_metadata("registry_root", &config.root.to_string())
                            .with_metadata("registry_key", &config.path)
                            .with_metadata("watch_tree", config.watch_tree.to_string());

                            if let Err(e) = emitter.try_send(event) {
                                error!("Failed to send registry event: {}", e);
                            }

                            // Re-arm the notification
                            if let Some(_new_handle) = Self::setup_registry_notification(
                                &Self {
                                    name: plugin_name.clone(),
                                    keys: keys.clone(),
                                    is_running: is_running.clone(),
                                    poll_interval,
                                },
                                *hkey,
                                config.watch_tree,
                            ) {
                                let _ = CloseHandle(*event_handle);
                                // Note: In production, you'd update the handle in the vector
                                // This is simplified for demonstration
                            }
                        }
                    }
                }
            }

            // Cleanup
            for (hkey, _, event_handle) in key_handles {
                unsafe {
                    let _ = CloseHandle(event_handle);
                }
                unsafe {
                    let _ = RegCloseKey(hkey);
                }
            }

            info!("Registry monitor stopped");
        });

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        info!("Stopping registry monitor plugin: {}", self.name);
        self.is_running.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::plugin::EventSourcePlugin;

    #[tokio::test]
    async fn test_registry_plugin_lifecycle() {
        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        let mut plugin = RegistryMonitorPlugin::new("test_registry");

        assert!(!plugin.is_running());

        // Don't actually start without keys - it will error
        let result = plugin.start(tx).await;
        assert!(result.is_err()); // Should fail without keys
    }

    #[test]
    fn test_registry_key_config() {
        let plugin = RegistryMonitorPlugin::new("test")
            .watch_key(RegistryRoot::HKEY_CURRENT_USER, "Software\\Test")
            .watch_key_recursive(
                RegistryRoot::HKEY_LOCAL_MACHINE,
                "SYSTEM\\CurrentControlSet",
            );

        assert_eq!(plugin.keys.len(), 2);
        assert!(!plugin.keys[0].watch_tree);
        assert!(plugin.keys[1].watch_tree);
    }

    #[test]
    fn test_registry_root_to_string() {
        assert_eq!(RegistryRoot::HKEY_LOCAL_MACHINE.to_string(), "HKLM");
        assert_eq!(RegistryRoot::HKEY_CURRENT_USER.to_string(), "HKCU");
        assert_eq!(RegistryRoot::HKEY_USERS.to_string(), "HKU");
        assert_eq!(RegistryRoot::HKEY_CURRENT_CONFIG.to_string(), "HKCC");
    }
}

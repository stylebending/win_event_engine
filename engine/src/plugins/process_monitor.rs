use async_trait::async_trait;
use engine_core::event::{Event, EventKind};
use engine_core::plugin::{EventEmitter, EventSourcePlugin, PluginError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::ProcessStatus::EnumProcesses;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_INFORMATION,
};

pub struct ProcessMonitorPlugin {
    name: String,
    filter_name: Option<String>,
    poll_interval: Duration,
    is_running: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    pid: u32,
    name: String,
    path: String,
}

impl ProcessMonitorPlugin {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            filter_name: None,
            poll_interval: Duration::from_secs(2),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_name_filter(mut self, pattern: impl Into<String>) -> Self {
        self.filter_name = Some(pattern.into());
        self
    }

    pub fn with_poll_interval(mut self, seconds: u64) -> Self {
        self.poll_interval = Duration::from_secs(seconds);
        self
    }

    #[allow(dead_code)]
    fn should_emit_event(&self, process_name: &str) -> bool {
        if let Some(ref filter) = self.filter_name {
            process_name.to_lowercase().contains(&filter.to_lowercase())
        } else {
            true
        }
    }

    fn get_process_list() -> Vec<ProcessInfo> {
        let mut processes = Vec::new();
        let mut process_ids = [0u32; 1024];
        let mut bytes_returned = 0u32;

        unsafe {
            if EnumProcesses(
                process_ids.as_mut_ptr(),
                (process_ids.len() * std::mem::size_of::<u32>()) as u32,
                &mut bytes_returned,
            )
            .is_ok()
            {
                let num_processes = bytes_returned as usize / std::mem::size_of::<u32>();

                for i in 0..num_processes {
                    let pid = process_ids[i];
                    if pid == 0 {
                        continue;
                    }

                    if let Some((name, path)) = Self::get_process_info(pid) {
                        processes.push(ProcessInfo { pid, name, path });
                    }
                }
            }
        }

        processes
    }

    fn get_process_info(pid: u32) -> Option<(String, String)> {
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_INFORMATION, false, pid).ok()?;

            let mut buffer = [0u16; 512];
            let mut size = buffer.len() as u32;

            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut size,
            );

            let _ = CloseHandle(handle);

            if result.is_ok() {
                let path = String::from_utf16_lossy(&buffer[..size as usize]);
                let name = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                Some((name, path))
            } else {
                None
            }
        }
    }
}

#[async_trait]
impl EventSourcePlugin for ProcessMonitorPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&mut self, emitter: EventEmitter) -> Result<(), PluginError> {
        if self.is_running.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("Starting process monitor plugin: {}", self.name);

        let is_running = self.is_running.clone();
        let poll_interval = self.poll_interval;
        let filter_name = self.filter_name.clone();
        let plugin_name = self.name.clone();

        let mut last_processes: HashMap<u32, ProcessInfo> = HashMap::new();

        // Get initial process list
        for proc in Self::get_process_list() {
            last_processes.insert(proc.pid, proc);
        }

        tokio::spawn(async move {
            is_running.store(true, Ordering::SeqCst);

            while is_running.load(Ordering::SeqCst) {
                sleep(poll_interval).await;

                if !is_running.load(Ordering::SeqCst) {
                    break;
                }

                let current_processes: HashMap<u32, ProcessInfo> =
                    Self::get_process_list()
                        .into_iter()
                        .map(|p| (p.pid, p))
                        .collect();

                // Check for new processes
                for (pid, proc) in &current_processes {
                    if !last_processes.contains_key(pid) {
                        // New process detected
                        let name_match = filter_name
                            .as_ref()
                            .map(|f| proc.name.to_lowercase().contains(&f.to_lowercase()))
                            .unwrap_or(true);

                        if name_match {
                            info!(
                                "Process started: {} (PID: {})",
                                proc.name, proc.pid
                            );

                            let event = Event::new(
                                EventKind::ProcessStarted {
                                    pid: proc.pid,
                                    name: proc.name.clone(),
                                    command_line: proc.path.clone(),
                                },
                                &plugin_name,
                            )
                            .with_metadata("process_name", &proc.name)
                            .with_metadata("process_path", &proc.path);

                            if let Err(e) = emitter.try_send(event) {
                                error!("Failed to send process start event: {}", e);
                            }
                        }
                    }
                }

                // Check for terminated processes
                for (pid, proc) in &last_processes {
                    if !current_processes.contains_key(pid) {
                        // Process terminated
                        let name_match = filter_name
                            .as_ref()
                            .map(|f| proc.name.to_lowercase().contains(&f.to_lowercase()))
                            .unwrap_or(true);

                        if name_match {
                            info!(
                                "Process stopped: {} (PID: {})",
                                proc.name, proc.pid
                            );

                            let event = Event::new(
                                EventKind::ProcessStopped {
                                    pid: proc.pid,
                                    name: proc.name.clone(),
                                },
                                &plugin_name,
                            )
                            .with_metadata("process_name", &proc.name);

                            if let Err(e) = emitter.try_send(event) {
                                error!("Failed to send process stop event: {}", e);
                            }
                        }
                    }
                }

                last_processes = current_processes;
            }

            info!("Process monitor stopped");
        });

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        info!("Stopping process monitor plugin: {}", self.name);
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
    async fn test_process_plugin_lifecycle() {
        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        let mut plugin = ProcessMonitorPlugin::new("test_process");

        assert!(!plugin.is_running());

        plugin.start(tx).await.expect("Failed to start plugin");
        
        // Give the async task time to start
        sleep(Duration::from_millis(100)).await;
        assert!(plugin.is_running());

        // Let it run briefly
        sleep(Duration::from_millis(500)).await;

        plugin.stop().await.expect("Failed to stop plugin");
        assert!(!plugin.is_running());
    }

    #[test]
    fn test_name_filter() {
        let plugin = ProcessMonitorPlugin::new("test").with_name_filter("chrome");

        assert!(plugin.should_emit_event("chrome.exe"));
        assert!(plugin.should_emit_event("GoogleChrome.exe"));
        assert!(!plugin.should_emit_event("notepad.exe"));
        assert!(!plugin.should_emit_event("firefox.exe"));
    }

    #[test]
    fn test_no_filter_matches_all() {
        let plugin = ProcessMonitorPlugin::new("test");

        assert!(plugin.should_emit_event("chrome.exe"));
        assert!(plugin.should_emit_event("notepad.exe"));
        assert!(plugin.should_emit_event("anything.exe"));
    }

    #[test]
    fn test_get_process_list() {
        // This test actually queries the system
        let processes = ProcessMonitorPlugin::get_process_list();
        assert!(!processes.is_empty(), "Should find at least one process");

        // Should find current process (cargo test)
        let current_pid = std::process::id();
        let current_proc = processes.iter().find(|p| p.pid == current_pid);
        assert!(current_proc.is_some(), "Should find current process");
    }
}

use async_trait::async_trait;
use engine_core::event::{Event, EventKind};
use engine_core::plugin::{EventEmitter, EventSourcePlugin, PluginError};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::UnhookWinEvent;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextW, GetWindowThreadProcessId};

pub struct WindowEventPlugin {
    name: String,
    filter_title: Option<String>,
    filter_process: Option<String>,
    hook: Option<isize>,
    is_running: Arc<AtomicBool>,
    event_sender: Option<mpsc::Sender<WindowEvent>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct WindowEvent {
    hwnd: isize,
    event_type: WindowEventType,
    title: String,
    process_id: u32,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum WindowEventType {
    Created,
    Destroyed,
    Focused,
}

impl WindowEventPlugin {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            filter_title: None,
            filter_process: None,
            hook: None,
            is_running: Arc::new(AtomicBool::new(false)),
            event_sender: None,
        }
    }

    pub fn with_title_filter(mut self, pattern: impl Into<String>) -> Self {
        self.filter_title = Some(pattern.into());
        self
    }

    pub fn with_process_filter(mut self, process: impl Into<String>) -> Self {
        self.filter_process = Some(process.into());
        self
    }

    #[allow(dead_code)]
    fn should_emit_event(&self, title: &str, process_name: &str) -> bool {
        if let Some(ref title_filter) = self.filter_title {
            if !title.contains(title_filter) {
                return false;
            }
        }

        if let Some(ref process_filter) = self.filter_process {
            if !process_name.contains(process_filter) {
                return false;
            }
        }

        true
    }

    fn get_window_info(hwnd: HWND) -> (String, u32) {
        let mut title = [0u16; 512];
        let len = unsafe { GetWindowTextW(hwnd, &mut title) };
        let title = String::from_utf16_lossy(&title[..len as usize]);

        let mut process_id = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };

        // Get process name from process ID would require more Win32 API calls
        // For now, we'll use the process ID
        let _process_name = format!("PID:{}", process_id);

        (title, process_id)
    }

    #[allow(dead_code)]
    fn create_event(&self, hwnd: isize, event_type: WindowEventType) -> Option<Event> {
        let hwnd = HWND(hwnd);
        let (title, process_id) = Self::get_window_info(hwnd);
        let _process_name = format!("PID:{}", process_id);

        if !self.should_emit_event(&title, &format!("PID:{}", process_id)) {
            return None;
        }

        let hwnd_isize = hwnd.0;
        let kind = match event_type {
            WindowEventType::Created => EventKind::WindowCreated {
                hwnd: hwnd_isize,
                title: title.clone(),
                process_id,
            },
            WindowEventType::Destroyed => EventKind::WindowDestroyed { hwnd: hwnd_isize },
            WindowEventType::Focused => EventKind::WindowFocused {
                hwnd: hwnd_isize,
                title: title.clone(),
            },
        };

        Some(
            Event::new(kind, &self.name)
                .with_metadata("window_title", &title)
                .with_metadata("process_id", process_id.to_string()),
        )
    }
}

#[async_trait]
impl EventSourcePlugin for WindowEventPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&mut self, emitter: EventEmitter) -> Result<(), PluginError> {
        if self.is_running.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("Starting window event plugin: {}", self.name);

        let (tx, mut rx) = mpsc::channel::<WindowEvent>(100);
        self.event_sender = Some(tx);

        let plugin_name = self.name.clone();
        let filter_title = self.filter_title.clone();
        let filter_process = self.filter_process.clone();
        let is_running = self.is_running.clone();

        // Spawn a task to handle events
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if !is_running.load(Ordering::SeqCst) {
                    break;
                }

                // Convert window event to engine event
                let hwnd = event.hwnd;
                let hwnd_handle = HWND(hwnd);
                let (title, process_id) = Self::get_window_info(hwnd_handle);
                let process_name = format!("PID:{}", process_id);

                // Apply filters
                let title_match = filter_title
                    .as_ref()
                    .map(|f| title.contains(f))
                    .unwrap_or(true);
                let process_match = filter_process
                    .as_ref()
                    .map(|f| process_name.contains(f))
                    .unwrap_or(true);

                if title_match && process_match {
                    let kind = match event.event_type {
                        WindowEventType::Created => EventKind::WindowCreated {
                            hwnd,
                            title: title.clone(),
                            process_id,
                        },
                        WindowEventType::Destroyed => EventKind::WindowDestroyed { hwnd },
                        WindowEventType::Focused => EventKind::WindowFocused {
                            hwnd,
                            title: title.clone(),
                        },
                    };

                    let engine_event = Event::new(kind, &plugin_name)
                        .with_metadata("window_title", &title)
                        .with_metadata("process_id", process_id.to_string());

                    if let Err(e) = emitter.try_send(engine_event) {
                        error!("Failed to send window event: {}", e);
                    }
                }
            }
        });

        // For now, we'll use a simple timer-based approach since SetWinEventHook
        // requires a message loop which is complex in async Rust
        // In production, you'd want to integrate this with the Windows message pump
        warn!("Window event monitoring is currently simulated for testing");
        self.is_running.store(true, Ordering::SeqCst);

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        info!("Stopping window event plugin: {}", self.name);

        if let Some(hook) = self.hook.take() {
            unsafe {
                let _ = UnhookWinEvent(windows::Win32::UI::Accessibility::HWINEVENTHOOK(hook));
            }
        }

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
    async fn test_window_plugin_lifecycle() {
        let (tx, _rx) = tokio::sync::mpsc::channel(10);
        let mut plugin = WindowEventPlugin::new("test_window");

        assert!(!plugin.is_running());

        plugin.start(tx).await.expect("Failed to start plugin");
        assert!(plugin.is_running());

        plugin.stop().await.expect("Failed to stop plugin");
        assert!(!plugin.is_running());
    }

    #[test]
    fn test_title_filter() {
        let plugin = WindowEventPlugin::new("test").with_title_filter("Chrome");

        assert!(plugin.should_emit_event("Google Chrome", "PID:1234"));
        assert!(!plugin.should_emit_event("Notepad", "PID:5678"));
    }

    #[test]
    fn test_process_filter() {
        // Test with PID pattern since that's what we have
        let plugin = WindowEventPlugin::new("test").with_process_filter("PID:");

        assert!(plugin.should_emit_event("Some Title", "PID:1234"));
        assert!(!plugin.should_emit_event("Some Title", "not-a-pid"));
    }
}

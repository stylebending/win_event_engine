use async_trait::async_trait;
use engine_core::event::{Event, EventKind, RegistryChangeType};
use engine_core::plugin::{EventEmitter, EventSourcePlugin, PluginError};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use tracing::{error, info};
use uuid::Uuid;
use windows::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, EnableTraceEx2, OpenTraceW, ProcessTrace, StartTraceW,
    CONTROLTRACE_HANDLE, EVENT_CONTROL_CODE_ENABLE_PROVIDER,
    EVENT_ENABLE_PROPERTY_PROCESS_START_KEY, EVENT_ENABLE_PROPERTY_SID, EVENT_ENABLE_PROPERTY_TS_ID,
    EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_FILE_MODE_NONE, EVENT_TRACE_PROPERTIES,
    EVENT_TRACE_REAL_TIME_MODE, PROCESSTRACE_HANDLE,
};
use windows::core::{GUID, PWSTR};

// ETW Provider GUID for kernel registry events
const KERNEL_REGISTRY_PROVIDER: GUID = GUID::from_u128(0xae5373a1_6483_4880_93b1_5a80e74a4d5e);

// Event IDs for Microsoft-Windows-Kernel-Registry
const EVENT_REG_CREATE_KEY: u16 = 1;
const EVENT_REG_OPEN_KEY: u16 = 2;
const EVENT_REG_DELETE_KEY: u16 = 3;
const _EVENT_REG_QUERY_KEY: u16 = 4;
const EVENT_REG_SET_VALUE: u16 = 5;
const EVENT_REG_DELETE_VALUE: u16 = 6;
const _EVENT_REG_QUERY_VALUE: u16 = 7;
const _EVENT_REG_ENUMERATE_KEY: u16 = 8;
const _EVENT_REG_ENUMERATE_VALUE: u16 = 9;
const _EVENT_REG_QUERY_MULTIPLE_VALUE: u16 = 10;
const _EVENT_REG_SET_INFORMATION: u16 = 11;
const _EVENT_REG_FLUSH: u16 = 12;

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum EtwEvent {
    RegistryKeyCreated {
        process_id: u32,
        thread_id: u32,
        key_path: String,
        status: u32,
    },
    RegistryKeyDeleted {
        process_id: u32,
        thread_id: u32,
        key_path: String,
        status: u32,
    },
    RegistryValueSet {
        process_id: u32,
        thread_id: u32,
        key_path: String,
        value_name: String,
        data_type: u32,
        data_size: u32,
    },
    RegistryValueDeleted {
        process_id: u32,
        thread_id: u32,
        key_path: String,
        value_name: String,
    },
    RegistryKeyOpened {
        process_id: u32,
        thread_id: u32,
        key_path: String,
        desired_access: u32,
    },
}

pub struct RegistryMonitorPlugin {
    name: String,
    keys: Vec<RegistryKeyConfig>,
    is_running: Arc<AtomicBool>,
    session_name: String,
    etw_thread: Option<JoinHandle<()>>,
    event_sender: Option<Sender<EtwEvent>>,
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
    #[allow(dead_code)]
    fn to_string(&self) -> String {
        match self {
            RegistryRoot::HKEY_LOCAL_MACHINE => "HKLM".to_string(),
            RegistryRoot::HKEY_CURRENT_USER => "HKCU".to_string(),
            RegistryRoot::HKEY_USERS => "HKU".to_string(),
            RegistryRoot::HKEY_CURRENT_CONFIG => "HKCC".to_string(),
        }
    }

    fn to_full_name(&self) -> String {
        match self {
            RegistryRoot::HKEY_LOCAL_MACHINE => "HKEY_LOCAL_MACHINE".to_string(),
            RegistryRoot::HKEY_CURRENT_USER => "HKEY_CURRENT_USER".to_string(),
            RegistryRoot::HKEY_USERS => "HKEY_USERS".to_string(),
            RegistryRoot::HKEY_CURRENT_CONFIG => "HKEY_CURRENT_CONFIG".to_string(),
        }
    }
}

// Thread-local storage for ETW callback context
thread_local! {
    static ETW_CALLBACK_CONTEXT: std::cell::RefCell<Option<EtwCallbackContext>> = std::cell::RefCell::new(None);
}

struct EtwCallbackContext {
    sender: Sender<EtwEvent>,
    is_running: Arc<AtomicBool>,
    #[allow(dead_code)]
    key_filters: HashSet<String>, // Keys to filter by
}

impl RegistryMonitorPlugin {
    pub fn new(name: impl Into<String>) -> Self {
        let name_str: String = name.into();
        // Generate unique session name to avoid conflicts
        let session_name = format!("wee_reg_{}_{}", 
            name_str.to_lowercase().replace(" ", "_").replace("-", "_"),
            Uuid::new_v4().simple()
        );
        
        Self {
            name: name_str,
            keys: Vec::new(),
            is_running: Arc::new(AtomicBool::new(false)),
            session_name,
            etw_thread: None,
            event_sender: None,
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

    fn build_key_filters(&self) -> HashSet<String> {
        let mut filters = HashSet::new();
        for key in &self.keys {
            let full_path = format!("{}\\{}", key.root.to_full_name(), key.path);
            filters.insert(full_path.to_lowercase());
            
            if key.watch_tree {
                // For recursive watching, also add as a prefix match
                // We'll check if event paths start with this prefix
            }
        }
        filters
    }

    #[allow(dead_code)]
    fn should_emit_event(&self, key_path: &str) -> bool {
        if self.keys.is_empty() {
            return true; // No filters, emit all
        }

        let key_lower = key_path.to_lowercase();
        
        for config in &self.keys {
            let filter_path = format!("{}\\{}", config.root.to_full_name(), config.path).to_lowercase();
            
            if config.watch_tree {
                // For recursive watching, check if the event path starts with the filter path
                if key_lower.starts_with(&filter_path) || filter_path.starts_with(&key_lower) {
                    return true;
                }
            } else {
                // Exact match or direct child
                if key_lower == filter_path || key_lower.starts_with(&format!("{}\\", filter_path)) {
                    return true;
                }
            }
        }
        
        false
    }

    fn run_etw_session(
        session_name: String,
        sender: Sender<EtwEvent>,
        is_running: Arc<AtomicBool>,
        _key_filters: HashSet<String>,
    ) -> Result<(), String> {
        info!("Starting ETW registry session: {}", session_name);

        // Create ETW session
        let session_handle = Self::create_etw_session(&session_name)?;
        info!("ETW registry session created successfully");

        // Enable registry provider
        Self::enable_registry_provider(session_handle)?;

        // Set up callback context
        let context = EtwCallbackContext {
            sender,
            is_running,
            key_filters: _key_filters,
        };

        // Open trace for processing
        let trace_handle = Self::open_trace(&session_name)?;
        info!("ETW registry trace opened for real-time processing");

        // Process events (this blocks until session stops)
        let result = Self::process_events(trace_handle, context);

        // Cleanup
        info!("Cleaning up ETW registry session");
        let _ = unsafe {
            ControlTraceW(
                session_handle,
                windows::core::PCWSTR::null(),
                std::ptr::null_mut(),
                EVENT_TRACE_CONTROL_STOP,
            )
        };

        result
    }

    fn create_etw_session(session_name: &str) -> Result<CONTROLTRACE_HANDLE, String> {
        let name_wide: Vec<u16> = session_name.encode_utf16().chain(std::iter::once(0)).collect();
        
        let name_len = name_wide.len() * std::mem::size_of::<u16>();
        let properties_size = std::mem::size_of::<EVENT_TRACE_PROPERTIES>() + name_len;
        
        let mut properties_buffer = vec![0u8; properties_size];
        let properties = properties_buffer.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
        
        unsafe {
            (*properties).Wnode.BufferSize = properties_size as u32;
            (*properties).Wnode.Guid = GUID::zeroed();
            (*properties).Wnode.ClientContext = 1;
            (*properties).Wnode.Flags = 0;
            
            (*properties).BufferSize = 64;
            (*properties).MinimumBuffers = 4;
            (*properties).MaximumBuffers = 64;
            (*properties).MaximumFileSize = 0;
            (*properties).LogFileMode = EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_FILE_MODE_NONE;
            (*properties).FlushTimer = 1;
            (*properties).EnableFlags = windows::Win32::System::Diagnostics::Etw::EVENT_TRACE_FLAG(0);
            
            let name_offset = std::mem::size_of::<EVENT_TRACE_PROPERTIES>();
            let name_ptr = properties_buffer.as_mut_ptr().add(name_offset) as *mut u16;
            std::ptr::copy_nonoverlapping(name_wide.as_ptr(), name_ptr, name_wide.len());
            (*properties).LoggerNameOffset = name_offset as u32;
            
            let mut session_handle: CONTROLTRACE_HANDLE = std::mem::zeroed();
            let result = StartTraceW(
                &mut session_handle,
                windows::core::PCWSTR(name_wide.as_ptr()),
                properties,
            );
            
            match result {
                Ok(_) => {
                    info!("ETW registry session '{}' created", session_name);
                    Ok(session_handle)
                }
                Err(e) => {
                    let error_code = e.code().0 as u32;
                    if error_code == 0xB7 {
                        Err(format!("ETW registry session '{}' already exists", session_name))
                    } else {
                        Err(format!("Failed to create ETW registry session: 0x{:08X}", error_code))
                    }
                }
            }
        }
    }

    fn enable_registry_provider(session_handle: CONTROLTRACE_HANDLE) -> Result<(), String> {
        unsafe {
            let result = EnableTraceEx2(
                session_handle,
                &KERNEL_REGISTRY_PROVIDER,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER.0,
                1,
                0,
                0,
                EVENT_ENABLE_PROPERTY_PROCESS_START_KEY | EVENT_ENABLE_PROPERTY_SID | EVENT_ENABLE_PROPERTY_TS_ID,
                None,
            );
            
            match result {
                Ok(_) => {
                    info!("Enabled registry ETW provider");
                    Ok(())
                }
                Err(e) => {
                    let error_code = e.code().0 as u32;
                    Err(format!("Failed to enable registry provider: 0x{:08X}", error_code))
                }
            }
        }
    }

    fn open_trace(session_name: &str) -> Result<PROCESSTRACE_HANDLE, String> {
        let name_wide: Vec<u16> = session_name.encode_utf16().chain(std::iter::once(0)).collect();
        
        unsafe {
            let mut logfile: windows::Win32::System::Diagnostics::Etw::EVENT_TRACE_LOGFILEW = std::mem::zeroed();
            logfile.LoggerName = windows::core::PWSTR(name_wide.as_ptr() as *mut _);
            logfile.Anonymous1.ProcessTraceMode = windows::Win32::System::Diagnostics::Etw::PROCESS_TRACE_MODE_EVENT_RECORD
                | windows::Win32::System::Diagnostics::Etw::PROCESS_TRACE_MODE_REAL_TIME;
            logfile.Anonymous2.EventRecordCallback = Some(etw_event_callback);
            
            let trace_handle = OpenTraceW(&mut logfile);
            
            let invalid_handle = PROCESSTRACE_HANDLE { Value: u64::MAX };
            if trace_handle.Value != 0 && trace_handle.Value != invalid_handle.Value {
                Ok(trace_handle)
            } else {
                Err("Failed to open ETW registry trace".to_string())
            }
        }
    }

    fn process_events(
        trace_handle: PROCESSTRACE_HANDLE,
        context: EtwCallbackContext,
    ) -> Result<(), String> {
        ETW_CALLBACK_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = Some(context);
        });
        
        unsafe {
            let result = ProcessTrace(&[trace_handle], None, None);
            
            let _ = CloseTrace(trace_handle);
            
            ETW_CALLBACK_CONTEXT.with(|ctx| {
                *ctx.borrow_mut() = None;
            });
            
            match result {
                Ok(_) => Ok(()),
                Err(e) => {
                    let error_code = e.code().0 as u32;
                    Err(format!("ProcessTrace failed: 0x{:08X}", error_code))
                }
            }
        }
    }

    fn parse_etw_event(event_record: *const windows::Win32::System::Diagnostics::Etw::EVENT_RECORD) -> Option<EtwEvent> {
        if event_record.is_null() {
            return None;
        }
        
        unsafe {
            let record = &*event_record;
            let event_id = record.EventHeader.EventDescriptor.Id;
            let provider_id = record.EventHeader.ProviderId;
            
            if provider_id != KERNEL_REGISTRY_PROVIDER {
                return None;
            }
            
            let process_id = record.EventHeader.ProcessId;
            let thread_id = record.EventHeader.ThreadId;
            
            let data = std::slice::from_raw_parts(
                record.UserData as *const u8,
                record.UserDataLength as usize,
            );
            
            match event_id {
                EVENT_REG_CREATE_KEY => Self::parse_create_key(data, process_id, thread_id),
                EVENT_REG_DELETE_KEY => Self::parse_delete_key(data, process_id, thread_id),
                EVENT_REG_SET_VALUE => Self::parse_set_value(data, process_id, thread_id),
                EVENT_REG_DELETE_VALUE => Self::parse_delete_value(data, process_id, thread_id),
                EVENT_REG_OPEN_KEY => Self::parse_open_key(data, process_id, thread_id),
                _ => None,
            }
        }
    }

    fn parse_create_key(data: &[u8], process_id: u32, thread_id: u32) -> Option<EtwEvent> {
        if data.len() < 8 {
            return None;
        }
        
        let status = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let key_path = Self::extract_unicode_string(&data[8..]).unwrap_or_default();
        
        Some(EtwEvent::RegistryKeyCreated {
            process_id,
            thread_id,
            key_path,
            status,
        })
    }

    fn parse_delete_key(data: &[u8], process_id: u32, thread_id: u32) -> Option<EtwEvent> {
        if data.len() < 8 {
            return None;
        }
        
        let status = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let key_path = Self::extract_unicode_string(&data[8..]).unwrap_or_default();
        
        Some(EtwEvent::RegistryKeyDeleted {
            process_id,
            thread_id,
            key_path,
            status,
        })
    }

    fn parse_set_value(data: &[u8], process_id: u32, thread_id: u32) -> Option<EtwEvent> {
        if data.len() < 16 {
            return None;
        }
        
        let data_type = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let data_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        
        // Extract strings (key path and value name)
        let (key_path, value_name) = Self::extract_two_unicode_strings(&data[16..]);
        
        Some(EtwEvent::RegistryValueSet {
            process_id,
            thread_id,
            key_path: key_path.unwrap_or_default(),
            value_name: value_name.unwrap_or_default(),
            data_type,
            data_size,
        })
    }

    fn parse_delete_value(data: &[u8], process_id: u32, thread_id: u32) -> Option<EtwEvent> {
        if data.len() < 4 {
            return None;
        }
        
        let (key_path, value_name) = Self::extract_two_unicode_strings(&data[4..]);
        
        Some(EtwEvent::RegistryValueDeleted {
            process_id,
            thread_id,
            key_path: key_path.unwrap_or_default(),
            value_name: value_name.unwrap_or_default(),
        })
    }

    fn parse_open_key(data: &[u8], process_id: u32, thread_id: u32) -> Option<EtwEvent> {
        if data.len() < 8 {
            return None;
        }
        
        let desired_access = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let key_path = Self::extract_unicode_string(&data[8..]).unwrap_or_default();
        
        Some(EtwEvent::RegistryKeyOpened {
            process_id,
            thread_id,
            key_path,
            desired_access,
        })
    }

    fn extract_unicode_string(data: &[u8]) -> Option<String> {
        if data.len() < 2 {
            return None;
        }
        
        let mut chars = Vec::new();
        for chunk in data.chunks_exact(2) {
            let ch = u16::from_le_bytes([chunk[0], chunk[1]]);
            if ch == 0 {
                break;
            }
            chars.push(ch);
        }
        
        if !chars.is_empty() {
            Some(String::from_utf16_lossy(&chars))
        } else {
            None
        }
    }

    fn extract_two_unicode_strings(data: &[u8]) -> (Option<String>, Option<String>) {
        let mut strings = Vec::new();
        let mut current = Vec::new();
        
        for chunk in data.chunks_exact(2) {
            let ch = u16::from_le_bytes([chunk[0], chunk[1]]);
            if ch == 0 {
                if !current.is_empty() {
                    strings.push(String::from_utf16_lossy(&current));
                    current.clear();
                }
            } else {
                current.push(ch);
            }
        }
        
        if !current.is_empty() {
            strings.push(String::from_utf16_lossy(&current));
        }
        
        (strings.get(0).cloned(), strings.get(1).cloned())
    }

    fn get_process_name_from_pid(pid: u32) -> Option<String> {
        if pid == 0 {
            return Some("System".to_string());
        }

        unsafe {
            use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, QueryFullProcessImageNameW};
            use windows::Win32::Foundation::CloseHandle;
            
            let handle = OpenProcess(PROCESS_QUERY_INFORMATION, false, pid).ok()?;
            
            let mut buffer = [0u16; 512];
            let mut size = buffer.len() as u32;
            
            let result = QueryFullProcessImageNameW(
                handle,
                windows::Win32::System::Threading::PROCESS_NAME_FORMAT(0),
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            );
            
            let _ = CloseHandle(handle);
            
            if result.is_ok() {
                let path = String::from_utf16_lossy(&buffer[..size as usize]);
                Some(std::path::Path::new(&path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string())
            } else {
                Some(format!("PID:{}", pid))
            }
        }
    }
}

unsafe extern "system" fn etw_event_callback(
    event_record: *mut windows::Win32::System::Diagnostics::Etw::EVENT_RECORD,
) {
    if event_record.is_null() {
        return;
    }
    
    if let Some(etw_event) = RegistryMonitorPlugin::parse_etw_event(event_record) {
        ETW_CALLBACK_CONTEXT.with(|ctx| {
            if let Some(ref context) = *ctx.borrow() {
                if context.is_running.load(Ordering::SeqCst) {
                    let _ = context.sender.send(etw_event);
                }
            }
        });
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

        info!("Starting ETW registry monitor plugin: {}", self.name);
        info!("Session name: {}", self.session_name);
        info!("Note: ETW requires administrator privileges");

        let session_name = self.session_name.clone();
        let is_running = self.is_running.clone();
        let plugin_name = self.name.clone();
        let key_filters = self.build_key_filters();

        // Create tokio channel for async communication
        let (tokio_sender, mut tokio_receiver) = tokio::sync::mpsc::channel(1000);
        
        // Create std channel for ETW thread
        let (std_sender, std_receiver) = mpsc::channel::<EtwEvent>();
        self.event_sender = Some(std_sender.clone());

        // Spawn dedicated ETW thread
        let is_running_clone = is_running.clone();
        let etw_thread = thread::spawn(move || {
            match Self::run_etw_session(
                session_name,
                std_sender,
                is_running_clone,
                key_filters,
            ) {
                Ok(_) => info!("ETW registry session completed successfully"),
                Err(e) => error!("ETW registry session failed: {}", e),
            }
        });

        // Spawn bridge thread to forward from std channel to tokio channel
        let bridge_thread = thread::spawn(move || {
            while let Ok(etw_event) = std_receiver.recv() {
                if tokio_sender.blocking_send(etw_event).is_err() {
                    break;
                }
            }
        });

        self.etw_thread = Some(etw_thread);

        // Give ETW time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Check if thread is still running
        if self.etw_thread.as_ref().map(|t| t.is_finished()).unwrap_or(true) {
            self.is_running.store(false, Ordering::SeqCst);
            self.etw_thread = None;
            self.event_sender = None;
            return Err(PluginError::Initialization(
                "Failed to start ETW registry monitoring. Administrator privileges are required for ETW.".to_string()
            ));
        }

        self.is_running.store(true, Ordering::SeqCst);

        // Build filter function
        let keys_clone = self.keys.clone();
        let should_emit_fn = move |key_path: &str| -> bool {
            if keys_clone.is_empty() {
                return true;
            }
            let key_lower = key_path.to_lowercase();
            for config in &keys_clone {
                let filter_path = format!("{}\\{}", config.root.to_full_name(), config.path).to_lowercase();
                if config.watch_tree {
                    if key_lower.starts_with(&filter_path) || filter_path.starts_with(&key_lower) {
                        return true;
                    }
                } else {
                    if key_lower == filter_path || key_lower.starts_with(&format!("{}\\", filter_path)) {
                        return true;
                    }
                }
            }
            false
        };

        // Spawn async task to process events
        tokio::spawn(async move {
            info!("ETW registry monitoring active (real-time kernel events)");
            
            let mut event_count = 0u64;
            let start_time = std::time::Instant::now();

            while is_running.load(Ordering::SeqCst) {
                match tokio_receiver.recv().await {
                    Some(etw_event) => {
                        event_count += 1;
                        
                        // Log stats every 100 events
                        if event_count % 100 == 0 {
                            let elapsed = start_time.elapsed().as_secs_f64();
                            let rate = if elapsed > 0.0 { event_count as f64 / elapsed } else { 0.0 };
                            info!("Processed {} ETW registry events ({:.1} events/sec)", event_count, rate);
                        }

                        let (should_emit, event) = match &etw_event {
                            EtwEvent::RegistryKeyCreated { process_id, key_path, status, .. } => {
                                let should = should_emit_fn(key_path);
                                let proc_name = Self::get_process_name_from_pid(*process_id)
                                    .unwrap_or_else(|| format!("PID:{}", process_id));
                                
                                let ev = Event::new(
                                    EventKind::RegistryChanged {
                                        root: Self::extract_root_from_path(key_path),
                                        key: key_path.clone(),
                                        value_name: None,
                                        change_type: RegistryChangeType::Created,
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &proc_name)
                                .with_metadata("process_id", process_id.to_string())
                                .with_metadata("status", status.to_string());
                                
                                (should, ev)
                            }
                            EtwEvent::RegistryKeyDeleted { process_id, key_path, status, .. } => {
                                let should = should_emit_fn(key_path);
                                let proc_name = Self::get_process_name_from_pid(*process_id)
                                    .unwrap_or_else(|| format!("PID:{}", process_id));
                                
                                let ev = Event::new(
                                    EventKind::RegistryChanged {
                                        root: Self::extract_root_from_path(key_path),
                                        key: key_path.clone(),
                                        value_name: None,
                                        change_type: RegistryChangeType::Deleted,
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &proc_name)
                                .with_metadata("process_id", process_id.to_string())
                                .with_metadata("status", status.to_string());
                                
                                (should, ev)
                            }
                            EtwEvent::RegistryValueSet { process_id, key_path, value_name, data_type, data_size, .. } => {
                                let should = should_emit_fn(key_path);
                                let proc_name = Self::get_process_name_from_pid(*process_id)
                                    .unwrap_or_else(|| format!("PID:{}", process_id));
                                
                                let ev = Event::new(
                                    EventKind::RegistryChanged {
                                        root: Self::extract_root_from_path(key_path),
                                        key: key_path.clone(),
                                        value_name: Some(value_name.clone()),
                                        change_type: RegistryChangeType::Modified,
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &proc_name)
                                .with_metadata("process_id", process_id.to_string())
                                .with_metadata("value_name", value_name)
                                .with_metadata("data_type", data_type.to_string())
                                .with_metadata("data_size", data_size.to_string());
                                
                                (should, ev)
                            }
                            EtwEvent::RegistryValueDeleted { process_id, key_path, value_name, .. } => {
                                let should = should_emit_fn(key_path);
                                let proc_name = Self::get_process_name_from_pid(*process_id)
                                    .unwrap_or_else(|| format!("PID:{}", process_id));
                                
                                let ev = Event::new(
                                    EventKind::RegistryChanged {
                                        root: Self::extract_root_from_path(key_path),
                                        key: key_path.clone(),
                                        value_name: Some(value_name.clone()),
                                        change_type: RegistryChangeType::Deleted,
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &proc_name)
                                .with_metadata("process_id", process_id.to_string())
                                .with_metadata("value_name", value_name);
                                
                                (should, ev)
                            }
                            EtwEvent::RegistryKeyOpened { process_id: _, key_path: _, desired_access: _, .. } => {
                                // Don't emit events for open operations to reduce noise
                                (false, Event::new(
                                    EventKind::TimerTick, // Dummy event, will be filtered out
                                    &plugin_name,
                                ))
                            }
                        };

                        if should_emit {
                            if let Err(e) = emitter.try_send(event) {
                                error!("Failed to send registry event: {}", e);
                            }
                        }
                    }
                    None => {
                        // Channel closed
                        break;
                    }
                }
            }

            info!("ETW registry event processing stopped ({} events processed)", event_count);
            
            // Wait for bridge thread to complete
            let _ = bridge_thread.join();
        });

        info!("ETW registry monitoring started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        info!("Stopping ETW registry monitor plugin: {}", self.name);
        self.is_running.store(false, Ordering::SeqCst);
        
        // Signal sender to drop
        self.event_sender = None;
        
        // Wait for ETW thread to finish
        if let Some(thread) = self.etw_thread.take() {
            let _ = thread.join();
        }
        
        info!("ETW registry monitor stopped");
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }
}

impl RegistryMonitorPlugin {
    fn extract_root_from_path(path: &str) -> String {
        if path.starts_with("HKEY_LOCAL_MACHINE") {
            "HKLM".to_string()
        } else if path.starts_with("HKEY_CURRENT_USER") {
            "HKCU".to_string()
        } else if path.starts_with("HKEY_USERS") {
            "HKU".to_string()
        } else if path.starts_with("HKEY_CURRENT_CONFIG") {
            "HKCC".to_string()
        } else if path.starts_with("HKEY_CLASSES_ROOT") {
            "HKCR".to_string()
        } else {
            "UNKNOWN".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::plugin::EventSourcePlugin;
    use tracing::warn;

    #[tokio::test]
    async fn test_registry_plugin_lifecycle() {
        let (tx, _rx) = tokio::sync::mpsc::channel(100);
        let mut plugin = RegistryMonitorPlugin::new("test_registry_etw")
            .watch_key(RegistryRoot::HKEY_CURRENT_USER, "Software\\Test");

        assert!(!plugin.is_running());

        // Note: This test requires admin privileges
        let result = plugin.start(tx).await;
        
        if result.is_ok() {
            assert!(plugin.is_running());
            plugin.stop().await.expect("Failed to stop plugin");
            assert!(!plugin.is_running());
        } else {
            // Expected if not running as admin
            warn!("ETW registry test skipped - requires admin privileges");
        }
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

    #[test]
    fn test_key_filtering() {
        let plugin = RegistryMonitorPlugin::new("test")
            .watch_key(RegistryRoot::HKEY_CURRENT_USER, "Software\\Chrome")
            .watch_key_recursive(RegistryRoot::HKEY_LOCAL_MACHINE, "SYSTEM");

        // Should match exact watched key
        assert!(plugin.should_emit_event("HKEY_CURRENT_USER\\Software\\Chrome"));
        
        // Should match child of watched key
        assert!(plugin.should_emit_event("HKEY_CURRENT_USER\\Software\\Chrome\\Extensions"));
        
        // Should match recursive watched key
        assert!(plugin.should_emit_event("HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet"));
        
        // Should not match unwatched key
        assert!(!plugin.should_emit_event("HKEY_CURRENT_USER\\Software\\Firefox"));
    }

    #[test]
    fn test_unicode_string_extraction() {
        // Create test data with UTF-16LE string
        let mut data = Vec::new();
        for ch in "test_key".encode_utf16() {
            data.extend_from_slice(&ch.to_le_bytes());
        }
        data.extend_from_slice(&[0u8, 0u8]); // null terminator
        
        let result = RegistryMonitorPlugin::extract_unicode_string(&data);
        assert_eq!(result, Some("test_key".to_string()));
    }

    #[test]
    fn test_extract_root_from_path() {
        assert_eq!(
            RegistryMonitorPlugin::extract_root_from_path("HKEY_LOCAL_MACHINE\\Software"),
            "HKLM"
        );
        assert_eq!(
            RegistryMonitorPlugin::extract_root_from_path("HKEY_CURRENT_USER\\Software"),
            "HKCU"
        );
        assert_eq!(
            RegistryMonitorPlugin::extract_root_from_path("HKEY_USERS\\.Default"),
            "HKU"
        );
        assert_eq!(
            RegistryMonitorPlugin::extract_root_from_path("HKEY_CLASSES_ROOT\\.txt"),
            "HKCR"
        );
    }
}

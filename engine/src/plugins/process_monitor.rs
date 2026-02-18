use async_trait::async_trait;
use engine_core::event::{Event, EventKind, NetworkProtocol};
use engine_core::plugin::{EventEmitter, EventSourcePlugin, PluginError};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use tracing::{error, info};
use uuid::Uuid;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::Etw::{
    CloseTrace, ControlTraceW, EnableTraceEx2, OpenTraceW, ProcessTrace, StartTraceW,
    CONTROLTRACE_HANDLE, EVENT_CONTROL_CODE_ENABLE_PROVIDER,
    EVENT_ENABLE_PROPERTY_PROCESS_START_KEY, EVENT_ENABLE_PROPERTY_SID, EVENT_ENABLE_PROPERTY_TS_ID,
    EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_FILE_MODE_NONE, EVENT_TRACE_PROPERTIES,
    EVENT_TRACE_REAL_TIME_MODE, PROCESSTRACE_HANDLE,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, QueryFullProcessImageNameW};
use windows::core::{GUID, PWSTR};

// ETW Provider GUIDs for kernel events
const KERNEL_PROCESS_PROVIDER: GUID = GUID::from_u128(0x22fb2cd6_0e7b_422b_a0c7_2fad1fd0e716);
const KERNEL_FILE_PROVIDER: GUID = GUID::from_u128(0xedd08927_3247_4782_8e9e_16cd77c99a55);
const KERNEL_NETWORK_PROVIDER: GUID = GUID::from_u128(0x7dd42a49_c5b4_4e2b_9f1c_2e4e6e8e6f27);

// Event IDs for Microsoft-Windows-Kernel-Process
const EVENT_PROCESS_START: u16 = 1;
const EVENT_PROCESS_STOP: u16 = 5;
const EVENT_THREAD_START: u16 = 3;
const EVENT_THREAD_STOP: u16 = 4;

// Event IDs for Microsoft-Windows-Kernel-File
const EVENT_FILE_CREATE: u16 = 64;
const EVENT_FILE_DELETE: u16 = 65;
const EVENT_FILE_READ: u16 = 67;
const EVENT_FILE_WRITE: u16 = 68;

// Event IDs for Microsoft-Windows-Kernel-Network
const EVENT_NETWORK_CONNECT: u16 = 11;
const EVENT_NETWORK_DISCONNECT: u16 = 12;

#[derive(Debug, Clone)]
enum EtwEvent {
    ProcessStart {
        pid: u32,
        parent_pid: u32,
        image_name: String,
        command_line: String,
        session_id: u32,
        user_sid: Option<String>,
    },
    ProcessStop {
        pid: u32,
        exit_code: u32,
    },
    ThreadStart {
        pid: u32,
        tid: u32,
        start_address: u64,
    },
    ThreadStop {
        pid: u32,
        tid: u32,
    },
    FileCreate {
        pid: u32,
        path: PathBuf,
    },
    FileDelete {
        pid: u32,
        path: PathBuf,
    },
    FileRead {
        pid: u32,
        path: PathBuf,
        bytes_read: u64,
    },
    FileWrite {
        pid: u32,
        path: PathBuf,
        bytes_written: u64,
    },
    NetworkConnect {
        pid: u32,
        local_addr: String,
        local_port: u16,
        remote_addr: String,
        remote_port: u16,
        protocol: NetworkProtocol,
    },
    NetworkDisconnect {
        pid: u32,
        local_addr: String,
        local_port: u16,
        remote_addr: String,
        remote_port: u16,
    },
}

pub struct ProcessMonitorPlugin {
    name: String,
    filter_name: Option<String>,
    monitor_threads: bool,
    monitor_files: bool,
    monitor_network: bool,
    is_running: Arc<AtomicBool>,
    session_name: String,
    etw_thread: Option<JoinHandle<()>>,
    event_sender: Option<Sender<EtwEvent>>,
}

// Thread-local storage for ETW callback context
thread_local! {
    static ETW_CALLBACK_CONTEXT: std::cell::RefCell<Option<EtwCallbackContext>> = std::cell::RefCell::new(None);
}

struct EtwCallbackContext {
    sender: Sender<EtwEvent>,
    is_running: Arc<AtomicBool>,
    #[allow(dead_code)]
    pid_filter: Option<String>,
}

impl ProcessMonitorPlugin {
    pub fn new(name: impl Into<String>) -> Self {
        let name_str: String = name.into();
        // Generate unique session name to avoid conflicts
        let session_name = format!("wee_{}_{}", 
            name_str.to_lowercase().replace(" ", "_").replace("-", "_"),
            Uuid::new_v4().simple()
        );
        
        Self {
            name: name_str,
            filter_name: None,
            monitor_threads: false,
            monitor_files: false,
            monitor_network: false,
            is_running: Arc::new(AtomicBool::new(false)),
            session_name,
            etw_thread: None,
            event_sender: None,
        }
    }

    pub fn with_name_filter(mut self, pattern: impl Into<String>) -> Self {
        self.filter_name = Some(pattern.into());
        self
    }

    pub fn with_thread_monitoring(mut self, enabled: bool) -> Self {
        self.monitor_threads = enabled;
        self
    }

    pub fn with_file_monitoring(mut self, enabled: bool) -> Self {
        self.monitor_files = enabled;
        self
    }

    pub fn with_network_monitoring(mut self, enabled: bool) -> Self {
        self.monitor_network = enabled;
        self
    }

    fn run_etw_session(
        session_name: String,
        sender: Sender<EtwEvent>,
        is_running: Arc<AtomicBool>,
        pid_filter: Option<String>,
        monitor_threads: bool,
        monitor_files: bool,
        monitor_network: bool,
    ) -> Result<(), String> {
        info!("Starting ETW session: {}", session_name);

        // Create ETW session
        let session_handle = Self::create_etw_session(&session_name)?;
        info!("ETW session created successfully");

        // Enable providers
        Self::enable_provider(session_handle, &KERNEL_PROCESS_PROVIDER, "process")?;
        
        if monitor_threads {
            Self::enable_provider(session_handle, &KERNEL_PROCESS_PROVIDER, "thread")?;
        }
        
        if monitor_files {
            Self::enable_provider(session_handle, &KERNEL_FILE_PROVIDER, "file")?;
        }
        
        if monitor_network {
            Self::enable_provider(session_handle, &KERNEL_NETWORK_PROVIDER, "network")?;
        }

        // Set up callback context
        let context = EtwCallbackContext {
            sender,
            is_running,
            pid_filter,
        };

        // Open trace for processing
        let trace_handle = Self::open_trace(&session_name)?;
        info!("ETW trace opened for real-time processing");

        // Process events (this blocks until session stops)
        let result = Self::process_events(trace_handle, context);

        // Cleanup
        info!("Cleaning up ETW session");
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
        
        // Calculate total size needed for properties struct
        let name_len = name_wide.len() * std::mem::size_of::<u16>();
        let properties_size = std::mem::size_of::<EVENT_TRACE_PROPERTIES>() + name_len;
        
        let mut properties_buffer = vec![0u8; properties_size];
        let properties = properties_buffer.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
        
        unsafe {
            // Initialize properties
            (*properties).Wnode.BufferSize = properties_size as u32;
            (*properties).Wnode.Guid = GUID::zeroed();
            (*properties).Wnode.ClientContext = 1; // Use query performance counter
            (*properties).Wnode.Flags = 0;
            
            (*properties).BufferSize = 64; // 64KB buffers
            (*properties).MinimumBuffers = 4;
            (*properties).MaximumBuffers = 64;
            (*properties).MaximumFileSize = 0; // No file size limit
            (*properties).LogFileMode = EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_FILE_MODE_NONE;
            (*properties).FlushTimer = 1; // 1 second flush
            (*properties).EnableFlags = windows::Win32::System::Diagnostics::Etw::EVENT_TRACE_FLAG(0);
            
            // Set session name at the offset
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
                    info!("ETW session '{}' created", session_name);
                    Ok(session_handle)
                }
                Err(e) => {
                    let error_code = e.code().0 as u32;
                    if error_code == 0xB7 { // ERROR_ALREADY_EXISTS
                        Err(format!("ETW session '{}' already exists. Try restarting or use a different name.", session_name))
                    } else {
                        Err(format!("Failed to create ETW session: 0x{:08X} - {:?}", error_code, e))
                    }
                }
            }
        }
    }

    fn enable_provider(
        session_handle: CONTROLTRACE_HANDLE,
        provider_guid: &GUID,
        provider_name: &str,
    ) -> Result<(), String> {
        unsafe {
            let result = EnableTraceEx2(
                session_handle,
                provider_guid,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER.0,
                1, // Level 1 = all events
                0, // Match any keyword
                0, // Match all keyword
                EVENT_ENABLE_PROPERTY_PROCESS_START_KEY | EVENT_ENABLE_PROPERTY_SID | EVENT_ENABLE_PROPERTY_TS_ID,
                None,
            );
            
            match result {
                Ok(_) => {
                    info!("Enabled {} provider", provider_name);
                    Ok(())
                }
                Err(e) => {
                    let error_code = e.code().0 as u32;
                    Err(format!("Failed to enable {} provider: 0x{:08X} - {:?}", provider_name, error_code, e))
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
                Err("Failed to open ETW trace".to_string())
            }
        }
    }

    fn process_events(
        trace_handle: PROCESSTRACE_HANDLE,
        context: EtwCallbackContext,
    ) -> Result<(), String> {
        // Store context in thread-local storage for the callback
        ETW_CALLBACK_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = Some(context);
        });
        
        unsafe {
            let result = ProcessTrace(&[trace_handle], None, None);
            
            // Cleanup
            let _ = CloseTrace(trace_handle);
            
            ETW_CALLBACK_CONTEXT.with(|ctx| {
                *ctx.borrow_mut() = None;
            });
            
            match result {
                Ok(_) => Ok(()),
                Err(e) => {
                    let error_code = e.code().0 as u32;
                    Err(format!("ProcessTrace failed: 0x{:08X} - {:?}", error_code, e))
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
            
            // Get user data
            let data = std::slice::from_raw_parts(
                record.UserData as *const u8,
                record.UserDataLength as usize,
            );
            
            // Parse based on provider
            if provider_id == KERNEL_PROCESS_PROVIDER {
                match event_id {
                    EVENT_PROCESS_START => Self::parse_process_start(data),
                    EVENT_PROCESS_STOP => Self::parse_process_stop(data),
                    EVENT_THREAD_START => Self::parse_thread_start(data),
                    EVENT_THREAD_STOP => Self::parse_thread_stop(data),
                    _ => None,
                }
            } else if provider_id == KERNEL_FILE_PROVIDER && !data.is_empty() {
                match event_id {
                    EVENT_FILE_CREATE => Self::parse_file_create(data),
                    EVENT_FILE_DELETE => Self::parse_file_delete(data),
                    EVENT_FILE_READ => Self::parse_file_read(data),
                    EVENT_FILE_WRITE => Self::parse_file_write(data),
                    _ => None,
                }
            } else if provider_id == KERNEL_NETWORK_PROVIDER && !data.is_empty() {
                match event_id {
                    EVENT_NETWORK_CONNECT => Self::parse_network_connect(data),
                    EVENT_NETWORK_DISCONNECT => Self::parse_network_disconnect(data),
                    _ => None,
                }
            } else {
                None
            }
        }
    }

    fn parse_process_start(data: &[u8]) -> Option<EtwEvent> {
        // Process start event layout varies by Windows version
        // Common layout: ProcessId(4), ParentId(4), SessionId(4), ... strings
        if data.len() < 16 {
            return None;
        }
        
        let pid = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let parent_pid = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let session_id = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        
        // Try to extract image name and command line from variable portion
        let (image_name, command_line) = Self::extract_unicode_strings(&data[16..]);
        
        Some(EtwEvent::ProcessStart {
            pid,
            parent_pid,
            image_name: image_name.unwrap_or_else(|| format!("PID:{}", pid)),
            command_line: command_line.unwrap_or_default(),
            session_id,
            user_sid: None, // Would need to parse SID from extended data
        })
    }

    fn parse_process_stop(data: &[u8]) -> Option<EtwEvent> {
        if data.len() < 8 {
            return None;
        }
        
        let pid = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let exit_code = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        
        Some(EtwEvent::ProcessStop { pid, exit_code })
    }

    fn parse_thread_start(data: &[u8]) -> Option<EtwEvent> {
        if data.len() < 16 {
            return None;
        }
        
        let tid = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let pid = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let start_address = u64::from_le_bytes([
            data[8], data[9], data[10], data[11],
            data[12], data[13], data[14], data[15],
        ]);
        
        Some(EtwEvent::ThreadStart { pid, tid, start_address })
    }

    fn parse_thread_stop(data: &[u8]) -> Option<EtwEvent> {
        if data.len() < 8 {
            return None;
        }
        
        let tid = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let pid = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        
        Some(EtwEvent::ThreadStop { pid, tid })
    }

    fn parse_file_create(data: &[u8]) -> Option<EtwEvent> {
        if data.len() < 8 {
            return None;
        }
        
        let pid = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let path = Self::extract_file_path(&data[8..]).unwrap_or_default();
        
        Some(EtwEvent::FileCreate { pid, path })
    }

    fn parse_file_delete(data: &[u8]) -> Option<EtwEvent> {
        if data.len() < 8 {
            return None;
        }
        
        let pid = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let path = Self::extract_file_path(&data[8..]).unwrap_or_default();
        
        Some(EtwEvent::FileDelete { pid, path })
    }

    fn parse_file_read(data: &[u8]) -> Option<EtwEvent> {
        if data.len() < 16 {
            return None;
        }
        
        let pid = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let bytes = u64::from_le_bytes([
            data[8], data[9], data[10], data[11],
            data[12], data[13], data[14], data[15],
        ]);
        let path = Self::extract_file_path(&data[16..]).unwrap_or_default();
        
        Some(EtwEvent::FileRead { pid, path, bytes_read: bytes })
    }

    fn parse_file_write(data: &[u8]) -> Option<EtwEvent> {
        if data.len() < 16 {
            return None;
        }
        
        let pid = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let bytes = u64::from_le_bytes([
            data[8], data[9], data[10], data[11],
            data[12], data[13], data[14], data[15],
        ]);
        let path = Self::extract_file_path(&data[16..]).unwrap_or_default();
        
        Some(EtwEvent::FileWrite { pid, path, bytes_written: bytes })
    }

    fn parse_network_connect(data: &[u8]) -> Option<EtwEvent> {
        if data.len() < 32 {
            return None;
        }
        
        let pid = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let local_port = u16::from_le_bytes([data[8], data[9]]);
        let remote_port = u16::from_le_bytes([data[10], data[11]]);
        let protocol = data[12];
        
        let local_addr = Self::parse_ip_address(&data[16..32]);
        let remote_addr = if data.len() >= 48 {
            Self::parse_ip_address(&data[32..48])
        } else {
            "unknown".to_string()
        };
        
        let protocol_enum = match protocol {
            6 => NetworkProtocol::Tcp,
            17 => NetworkProtocol::Udp,
            _ => NetworkProtocol::Other(format!("{}", protocol)),
        };
        
        Some(EtwEvent::NetworkConnect {
            pid,
            local_addr,
            local_port,
            remote_addr,
            remote_port,
            protocol: protocol_enum,
        })
    }

    fn parse_network_disconnect(data: &[u8]) -> Option<EtwEvent> {
        if data.len() < 32 {
            return None;
        }
        
        let pid = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let local_port = u16::from_le_bytes([data[8], data[9]]);
        let remote_port = u16::from_le_bytes([data[10], data[11]]);
        
        let local_addr = Self::parse_ip_address(&data[16..32]);
        let remote_addr = if data.len() >= 48 {
            Self::parse_ip_address(&data[32..48])
        } else {
            "unknown".to_string()
        };
        
        Some(EtwEvent::NetworkDisconnect {
            pid,
            local_addr,
            local_port,
            remote_addr,
            remote_port,
        })
    }

    fn extract_unicode_strings(data: &[u8]) -> (Option<String>, Option<String>) {
        if data.len() < 4 {
            return (None, None);
        }
        
        // Try to find null-terminated UTF-16 strings
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

    fn extract_file_path(data: &[u8]) -> Option<PathBuf> {
        // File path in ETW is typically a count-prefixed or null-terminated UTF-16 string
        if data.len() < 2 {
            return None;
        }
        
        // Try count-prefixed first
        let len = u16::from_le_bytes([data[0], data[1]]) as usize;
        if len > 0 && len < 260 && data.len() >= 2 + len * 2 {
            let path = String::from_utf16_lossy(
                &data[2..2 + len * 2]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect::<Vec<_>>()
            );
            return Some(PathBuf::from(path));
        }
        
        // Try null-terminated
        let mut path_chars = Vec::new();
        for chunk in data.chunks_exact(2) {
            let ch = u16::from_le_bytes([chunk[0], chunk[1]]);
            if ch == 0 {
                break;
            }
            path_chars.push(ch);
        }
        
        if !path_chars.is_empty() {
            Some(PathBuf::from(String::from_utf16_lossy(&path_chars)))
        } else {
            None
        }
    }

    fn parse_ip_address(data: &[u8]) -> String {
        if data.len() >= 16 {
            // Check if IPv4 mapped to IPv6 (::ffff:x.x.x.x)
            let is_ipv4_mapped = data[0..10].iter().all(|&b| b == 0) 
                && data[10] == 0xff && data[11] == 0xff;
            
            if is_ipv4_mapped {
                format!("{}.{}.{}.{}", data[12], data[13], data[14], data[15])
            } else {
                // IPv6
                let segments: Vec<String> = data[..16]
                    .chunks_exact(2)
                    .map(|c| format!("{:02x}{:02x}", c[0], c[1]))
                    .collect();
                segments.join(":")
            }
        } else if data.len() >= 4 {
            format!("{}.{}.{}.{}", data[0], data[1], data[2], data[3])
        } else {
            "unknown".to_string()
        }
    }

    fn get_process_name_from_pid(pid: u32) -> Option<String> {
        if pid == 0 {
            return Some("System".to_string());
        }

        unsafe {
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
    
    if let Some(etw_event) = ProcessMonitorPlugin::parse_etw_event(event_record) {
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
impl EventSourcePlugin for ProcessMonitorPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&mut self, emitter: EventEmitter) -> Result<(), PluginError> {
        if self.is_running.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("Starting ETW process monitor plugin: {}", self.name);
        info!("Session name: {}", self.session_name);

        let session_name = self.session_name.clone();
        let is_running = self.is_running.clone();
        let filter_name = self.filter_name.clone();
        let plugin_name = self.name.clone();
        let monitor_threads = self.monitor_threads;
        let monitor_files = self.monitor_files;
        let monitor_network = self.monitor_network;

        // Create tokio channel for async communication
        let (tokio_sender, mut tokio_receiver) = tokio::sync::mpsc::channel(1000);
        
        // Create std channel for ETW thread to tokio bridge
        let (std_sender, std_receiver) = mpsc::channel::<EtwEvent>();
        self.event_sender = Some(std_sender.clone());

        // Spawn dedicated ETW thread
        let is_running_clone = is_running.clone();
        let etw_thread = thread::spawn(move || {
            match Self::run_etw_session(
                session_name,
                std_sender,
                is_running_clone,
                filter_name,
                monitor_threads,
                monitor_files,
                monitor_network,
            ) {
                Ok(_) => info!("ETW session completed successfully"),
                Err(e) => error!("ETW session failed: {}", e),
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
                "Failed to start ETW monitoring. Administrator privileges are required for ETW.".to_string()
            ));
        }

        self.is_running.store(true, Ordering::SeqCst);

        // Spawn async task to process events
        tokio::spawn(async move {
            info!("ETW process monitoring active (real-time kernel events)");
            
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
                            info!("Processed {} ETW events ({:.1} events/sec)", event_count, rate);
                        }

                        let event = match etw_event {
                            EtwEvent::ProcessStart { pid, parent_pid, image_name, command_line, session_id, user_sid } => {
                                Event::new(
                                    EventKind::ProcessStarted {
                                        pid,
                                        parent_pid,
                                        name: image_name.clone(),
                                        path: image_name.clone(),
                                        command_line,
                                        session_id,
                                        user: user_sid.unwrap_or_default(),
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &image_name)
                                .with_metadata("parent_pid", parent_pid.to_string())
                            }
                            EtwEvent::ProcessStop { pid, exit_code } => {
                                let name = Self::get_process_name_from_pid(pid)
                                    .unwrap_or_else(|| format!("PID:{}", pid));

                                Event::new(
                                    EventKind::ProcessStopped {
                                        pid,
                                        name: name.clone(),
                                        exit_code: Some(exit_code),
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &name)
                                .with_metadata("exit_code", exit_code.to_string())
                            }
                            EtwEvent::ThreadStart { pid, tid, start_address } => {
                                let name = Self::get_process_name_from_pid(pid)
                                    .unwrap_or_else(|| format!("PID:{}", pid));

                                Event::new(
                                    EventKind::ThreadCreated {
                                        pid,
                                        tid,
                                        start_address,
                                        user_stack: None,
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &name)
                            }
                            EtwEvent::ThreadStop { pid, tid } => {
                                let name = Self::get_process_name_from_pid(pid)
                                    .unwrap_or_else(|| format!("PID:{}", pid));

                                Event::new(
                                    EventKind::ThreadDestroyed { pid, tid },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &name)
                            }
                            EtwEvent::FileCreate { pid, path } => {
                                let name = Self::get_process_name_from_pid(pid)
                                    .unwrap_or_else(|| format!("PID:{}", pid));

                                Event::new(
                                    EventKind::FileAccessed {
                                        pid,
                                        path: path.clone(),
                                        access_mask: 0,
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &name)
                                .with_metadata("operation", "create")
                            }
                            EtwEvent::FileDelete { pid, path } => {
                                let name = Self::get_process_name_from_pid(pid)
                                    .unwrap_or_else(|| format!("PID:{}", pid));

                                Event::new(
                                    EventKind::FileIoDelete { pid, path: path.clone() },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &name)
                            }
                            EtwEvent::FileRead { pid, path, bytes_read } => {
                                let name = Self::get_process_name_from_pid(pid)
                                    .unwrap_or_else(|| format!("PID:{}", pid));

                                Event::new(
                                    EventKind::FileIoRead {
                                        pid,
                                        path: path.clone(),
                                        bytes_read,
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &name)
                                .with_metadata("bytes", bytes_read.to_string())
                            }
                            EtwEvent::FileWrite { pid, path, bytes_written } => {
                                let name = Self::get_process_name_from_pid(pid)
                                    .unwrap_or_else(|| format!("PID:{}", pid));

                                Event::new(
                                    EventKind::FileIoWrite {
                                        pid,
                                        path: path.clone(),
                                        bytes_written,
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &name)
                                .with_metadata("bytes", bytes_written.to_string())
                            }
                            EtwEvent::NetworkConnect { pid, local_addr, local_port, remote_addr, remote_port, protocol } => {
                                let name = Self::get_process_name_from_pid(pid)
                                    .unwrap_or_else(|| format!("PID:{}", pid));

                                Event::new(
                                    EventKind::NetworkConnectionCreated {
                                        pid,
                                        local_addr: local_addr.clone(),
                                        local_port,
                                        remote_addr: remote_addr.clone(),
                                        remote_port,
                                        protocol: protocol.clone(),
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &name)
                                .with_metadata("protocol", format!("{:?}", protocol))
                            }
                            EtwEvent::NetworkDisconnect { pid, local_addr, local_port, remote_addr, remote_port } => {
                                let name = Self::get_process_name_from_pid(pid)
                                    .unwrap_or_else(|| format!("PID:{}", pid));

                                Event::new(
                                    EventKind::NetworkConnectionClosed {
                                        pid,
                                        local_addr: local_addr.clone(),
                                        local_port,
                                        remote_addr: remote_addr.clone(),
                                        remote_port,
                                    },
                                    &plugin_name,
                                )
                                .with_metadata("process_name", &name)
                            }
                        };

                        if let Err(e) = emitter.try_send(event) {
                            error!("Failed to send event: {}", e);
                        }
                    }
                    None => {
                        // Channel closed
                        break;
                    }
                }
            }

            info!("ETW event processing stopped ({} events processed)", event_count);
            
            // Wait for bridge thread to complete
            let _ = bridge_thread.join();
        });

        info!("ETW process monitoring started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        info!("Stopping ETW process monitor plugin: {}", self.name);
        self.is_running.store(false, Ordering::SeqCst);
        
        // Signal sender to drop
        self.event_sender = None;
        
        // Wait for ETW thread to finish
        if let Some(thread) = self.etw_thread.take() {
            let _ = thread.join();
        }
        
        info!("ETW process monitor stopped");
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
    use tracing::warn;

    #[tokio::test]
    async fn test_process_plugin_lifecycle() {
        let (tx, _rx) = tokio::sync::mpsc::channel(100);
        let mut plugin = ProcessMonitorPlugin::new("test_process_etw");

        assert!(!plugin.is_running());

        // Note: This test requires admin privileges
        let result = plugin.start(tx).await;
        
        if result.is_ok() {
            assert!(plugin.is_running());
            plugin.stop().await.expect("Failed to stop plugin");
            assert!(!plugin.is_running());
        } else {
            // Expected if not running as admin
            warn!("ETW test skipped - requires admin privileges");
        }
    }

    #[test]
    fn test_ip_address_parsing() {
        // IPv4
        let ipv4_mapped = vec![0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 192, 168, 1, 1];
        assert_eq!(ProcessMonitorPlugin::parse_ip_address(&ipv4_mapped), "192.168.1.1");
        
        // IPv6
        let ipv6 = vec![0x20u8, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00, 
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
        let result = ProcessMonitorPlugin::parse_ip_address(&ipv6);
        assert!(result.contains(":"));
    }

    #[test]
    fn test_unicode_string_extraction() {
        // Create test data with UTF-16 strings
        let mut data = Vec::new();
        // First string: "test"
        for ch in "test".encode_utf16() {
            data.extend_from_slice(&ch.to_le_bytes());
        }
        data.extend_from_slice(&[0u8, 0u8]); // null terminator
        // Second string: "process"
        for ch in "process".encode_utf16() {
            data.extend_from_slice(&ch.to_le_bytes());
        }
        data.extend_from_slice(&[0u8, 0u8]); // null terminator
        
        let (first, second) = ProcessMonitorPlugin::extract_unicode_strings(&data);
        assert_eq!(first, Some("test".to_string()));
        assert_eq!(second, Some("process".to_string()));
    }

    #[test]
    fn test_builder_methods() {
        let plugin = ProcessMonitorPlugin::new("test")
            .with_name_filter("chrome")
            .with_thread_monitoring(true)
            .with_file_monitoring(true)
            .with_network_monitoring(true);

        assert!(plugin.monitor_threads);
        assert!(plugin.monitor_files);
        assert!(plugin.monitor_network);
        assert_eq!(plugin.filter_name, Some("chrome".to_string()));
    }
}

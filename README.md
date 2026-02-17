# Windows Event Automation Engine

[![Rust](https://img.shields.io/badge/Rust-2024%20Edition-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A universal event automation system for Windows built in Rust. Monitor file system events, window activity, process creation/termination, and registry changes - then execute automated actions based on configurable rules.

## Features

### Event Sources
- **File System Watcher** - Monitor file creation, modification, deletion with pattern matching
- **Window Event Monitor** - Track window focus, creation, destruction using Win32 API
- **Process Monitor** - Kernel-level ETW process monitoring with real-time events (process, thread, file I/O, network)
  - **Requires Administrator privileges**
  - Process start/stop events with full details (PID, parent PID, command line, session ID)
  - Thread creation/destruction monitoring
  - File I/O operations per process
  - Network connections per process
- **Registry Monitor** - Kernel-level ETW registry monitoring with real-time events
  - **Requires Administrator privileges**
  - Registry key creation, deletion, modification
  - Registry value set, delete, modify operations
  - Process context for each operation
  - Filter by registry hive and path

### Rule Engine
- Pattern-based matching using glob syntax (`*.txt`, `**/*.log`)
- Composite rules with AND/OR logic
- Enable/disable rules dynamically
- Rule descriptions and metadata

### Action System
- Execute shell commands
- Run PowerShell scripts
- Structured logging with configurable levels
- HTTP webhooks (extensible)
- Windows notifications (extensible)
- Media playback control (play/pause/toggle)

### Production Ready
- TOML-based configuration
- CLI interface with status checking
- Structured logging with `tracing`
- Graceful shutdown handling
- Configuration validation
- Hot-reloading support (enabled by default, disable with `--no-watch`)

## Quick Start

### Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) (1.75 or later)
- Windows 10/11
- Visual Studio 2022 Build Tools (for Windows API support)

### Build from Source

```bash
# Clone the repository
git clone https://github.com/stylebending/win_event_engine.git
cd win_event_engine

# Build in release mode
cargo build --release -p engine

# The executable will be at:
# target/release/engine.exe
```

### Run the Engine

```bash
# Run with default demo configuration
cargo run -p engine

# Run with a specific configuration file
cargo run -p engine -- -c config.toml

# Run with configuration from a directory
cargo run -p engine -- -d config/

# Check engine status without running
cargo run -p engine -- --status

# Enable debug logging
cargo run -p engine -- --log-level debug

# Dry run (log actions but don't execute)
cargo run -p engine -- --dry-run

# Disable hot-reloading (enabled by default)
cargo run -p engine -- --no-watch

# Install as Windows Service (requires admin)
cargo run -p engine -- --install

# Uninstall Windows Service (requires admin)
cargo run -p engine -- --uninstall
```

## Windows Service

Create a `config.toml` file in the project root:

```toml
[engine]
event_buffer_size = 1000
log_level = "info"

# File system watcher - monitor downloads for executable files
[[sources]]
name = "downloads_watcher"
type = "file_watcher"
paths = ["C:/Users/*/Downloads"]
pattern = "*.exe"
recursive = false
enabled = true

# Window watcher - track application focus (disabled by default)
[[sources]]
name = "app_focus_tracker"
type = "window_watcher"
enabled = false

# Process monitor - watch for specific applications
[[sources]]
name = "process_monitor"
type = "process_monitor"
process_name = "chrome"
monitor_threads = false
monitor_files = false
monitor_network = false
enabled = false

# Registry monitor - watch for system changes
[[sources]]
name = "system_settings"
type = "registry_monitor"
root = "HKLM"
key = "SOFTWARE/Microsoft/Windows/CurrentVersion/Run"
recursive = false
enabled = false

# Rule 1: Alert on executable downloads
[[rules]]
name = "executable_downloaded"
description = "Alert when executable files are downloaded"
trigger = { type = "file_created", pattern = "*.exe" }
action = { type = "log", message = "WARNING: Executable file downloaded", level = "warn" }
enabled = true

# Rule 2: Log text file modifications
[[rules]]
name = "text_file_modified"
description = "Log when text files are modified"
trigger = { type = "file_modified", pattern = "*.txt" }
action = { type = "log", message = "Text file modified", level = "info" }
enabled = true

# Rule 3: Run PowerShell script on CSV creation
[[rules]]
name = "process_csv"
description = "Process CSV files when created"
trigger = { type = "file_created", pattern = "*.csv" }
action = { type = "powershell", script = "Write-Host 'CSV file detected: ' $env:EVENT_PATH" }
enabled = false

# Rule 4: Alert when Chrome starts
[[rules]]
name = "chrome_started"
description = "Alert when Chrome starts"
trigger = { type = "process_started", process_name = "chrome" }
action = { type = "notify", title = "Chrome Started", message = "Google Chrome has been launched" }
enabled = false
```

See `config.toml.example`, `rules.toml.example`, and `config.media_automation.toml` for more examples.

## CLI Usage

```
Windows Event Automation Engine v0.1.1

Usage: engine [OPTIONS]

Options:
  -c, --config <FILE>       Path to configuration file
  -d, --config-dir <DIR>    Directory containing configuration files
      --dry-run             Run in dry-run mode (don't execute actions)
  -l, --log-level <LEVEL>  Log level (debug, info, warn, error) [default: info]
      --no-watch            Disable hot-reloading of configuration
      --install             Install as Windows Service (requires admin)
      --uninstall           Uninstall Windows Service (requires admin)
      --run-service         Internal: run as Windows Service
      --status              Show engine status and exit
  -h, --help               Print help
  -V, --version            Print version
```

## Windows Service

The engine can run as a Windows Service for production deployments.

### Installation

```bash
# Install the service (requires admin)
engine.exe --install

# Start the service
sc start WinEventEngine
```

### Configuration

When running as a service:
- Config location: `%PROGRAMDATA%\win_event_engine\config\config.toml`
- Log location: `%PROGRAMDATA%\win_event_engine\logs\service.log`

### Uninstallation

```bash
# Stop the service first
sc stop WinEventEngine

# Uninstall the service
engine.exe --uninstall
```

## Metrics and Monitoring

The engine includes built-in metrics collection with a local HTTP endpoint for monitoring.

### Available Metrics

The following metrics are collected automatically:

- **events_total** - Total events processed by source and type
- **events_dropped_total** - Events dropped due to full buffer
- **events_processing_duration_seconds** - Event processing latency
- **rules_evaluated_total** - Rule evaluations by rule name
- **rules_matched_total** - Successful rule matches
- **rules_match_duration_seconds** - Rule matching latency
- **actions_executed_total** - Action executions by name and status
- **actions_execution_duration_seconds** - Action execution latency
- **plugins_events_generated_total** - Events generated per plugin
- **plugins_errors_total** - Plugin errors
- **config_reload_total** - Configuration reloads
- **engine_uptime_seconds** - Engine uptime

### Retention

Metrics are retained with a sliding window:
- **Regular metrics**: 1 hour
- **Error metrics**: 24 hours

### Accessing Metrics

The metrics endpoint runs on `127.0.0.1:9090` (localhost only):

```bash
# Prometheus format
curl http://127.0.0.1:9090/metrics

# JSON snapshot
curl http://127.0.0.1:9090/api/snapshot

# Health check
curl http://127.0.0.1:9090/health

# Web UI
curl http://127.0.0.1:9090/
```

**Note**: The metrics endpoint is only accessible from localhost for security.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│        Windows Event Automation Engine v0.1.1           │
├─────────────────────────────────────────────────────────┤
│  CLI (clap) → Config (TOML) → Engine                    │
│                    ↑              │                     │
│                    │    Config    │                     │
│                    └──── Watcher ←┘                     │
│                                                         │
│  Event Sources:                                         │
│  ├── File Watcher (notify crate)                        │
│  ├── Window Watcher (Win32 API)                         │
│  ├── Process Monitor (ETW - kernel-level real-time)     │
│  └── Registry Monitor (ETW - kernel-level real-time)    │
│                                                         │
│  Event Bus (tokio mpsc channels)                        │
│                                                         │
│  Rule Engine                                            │
│  ├── File Pattern Matcher                               │
│  ├── Event Kind Matcher                                 │
│  └── Composite Matcher (AND/OR)                         │
│                                                         │
│  Action Executor                                        │
│  ├── Execute Command                                    │
│  ├── PowerShell Script                                  │
│  ├── Log Message                                        │
│  └── HTTP Request (extensible)                          │
│                                                         │
│  Metrics Collector (1h sliding window)                  │
│  └── HTTP Endpoint @ 127.0.0.1:9090                     │
└─────────────────────────────────────────────────────────┘
```

## Project Structure

```
win_event_engine/
├── engine/              # Main application binary
│   ├── src/
│   │   ├── main.rs     # CLI entry point
│   │   ├── engine.rs   # Engine orchestration
│   │   ├── config.rs   # Configuration management
│   │   └── plugins/    # Event source plugins
│   │       ├── file_watcher.rs
│   │       ├── window_watcher.rs
│   │       ├── process_monitor.rs
│   │       └── registry_monitor.rs
│   └── Cargo.toml
├── engine_core/        # Core types and traits
│   ├── src/
│   │   ├── event.rs    # Event types
│   │   ├── plugin.rs   # Plugin trait
│   │   └── lib.rs
│   └── Cargo.toml
├── rules/              # Rule matching engine
│   ├── src/
│   │   └── lib.rs
│   └── Cargo.toml
├── actions/            # Action execution system
│   ├── src/
│   │   └── lib.rs
│   └── Cargo.toml
├── bus/                # Event bus implementation
│   ├── src/
│   │   └── lib.rs
│   └── Cargo.toml
├── metrics/            # Metrics collection and HTTP endpoint
│   ├── src/
│   │   ├── lib.rs      # Metrics collector
│   │   └── server.rs   # HTTP server
│   └── Cargo.toml
├── config.toml.example # Example configuration
├── rules.toml.example  # Example rules
├── AGENTS.md          # Developer guidelines
├── Cargo.toml         # Workspace definition
└── README.md          # This file
```

## Event Types

The engine supports the following event types:

### File Events
- `FileCreated` - New file created
- `FileModified` - File content changed  
- `FileDeleted` - File removed
- `FileRenamed` - File renamed

### Window Events
- `WindowCreated` - New window opened
- `WindowDestroyed` - Window closed
- `WindowFocused` - Window received focus
- `WindowUnfocused` - Window lost focus

### Process Events
- `ProcessStarted` - New process launched (includes PID, parent PID, name, path, command line, session ID, user)
- `ProcessStopped` - Process terminated (includes PID, name, exit code)

### Thread Events
- `ThreadCreated` - New thread created in a process (includes PID, TID, start address)
- `ThreadDestroyed` - Thread terminated (includes PID, TID)

### File I/O Events (Process Context)
- `FileAccessed` - File accessed by a process (includes PID, path, access mask)
- `FileIoRead` - File read operation (includes PID, path, bytes read)
- `FileIoWrite` - File write operation (includes PID, path, bytes written)
- `FileIoDelete` - File deleted by a process (includes PID, path)

### Network Events
- `NetworkConnectionCreated` - New network connection (includes PID, local/remote addresses, protocol)
- `NetworkConnectionClosed` - Network connection closed (includes PID, addresses)

### Registry Events (via ETW - includes process context)
- `RegistryChanged` - Registry operation detected with change type:
  - **Created** - New registry key created
  - **Modified** - Registry value modified or set
  - **Deleted** - Registry key or value deleted
- **Event metadata includes**: Process name, Process ID, Registry path, Value name (if applicable)

## Development

### Running Tests

```bash
# Run all tests
cargo test --all

# Run tests for specific crate
cargo test -p engine
cargo test -p engine_core
cargo test -p actions

# Run with output
cargo test --all -- --nocapture
```

### Code Style

This project follows the guidelines in `AGENTS.md`. Key points:

- Use `cargo fmt` for formatting
- Use `cargo clippy` for linting
- All code must pass tests before merging
- Follow existing code patterns in the codebase

### Adding a New Event Source

1. Create a new plugin in `engine/src/plugins/`
2. Implement the `EventSourcePlugin` trait
3. Add configuration support in `config.rs`
4. Register the plugin in `engine.rs`

Example:

```rust
use async_trait::async_trait;
use engine_core::event::{Event, EventKind};
use engine_core::plugin::{EventEmitter, EventSourcePlugin, PluginError};

pub struct MyPlugin {
    name: String,
}

#[async_trait]
impl EventSourcePlugin for MyPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&mut self, emitter: EventEmitter) -> Result<(), PluginError> {
        // Start monitoring and emit events
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        // Cleanup
        Ok(())
    }

    fn is_running(&self) -> bool {
        // Return running status
        false
    }
}
```

## Contributing

Contributions are welcome! Please read `AGENTS.md` for coding guidelines and development practices.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## Security

This project interacts with the Windows API and can execute arbitrary commands. Please be careful when:
- Running the engine with elevated privileges
- Configuring PowerShell script actions
- Monitoring sensitive directories or registry keys

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built with [Rust](https://www.rust-lang.org/)
- Windows API bindings via [windows-rs](https://github.com/microsoft/windows-rs)
- Async runtime powered by [tokio](https://tokio.rs/)
- CLI interface by [clap](https://github.com/clap-rs/clap)

## Roadmap

- [x] File system monitoring
- [x] Window event monitoring  
- [x] Process monitoring
- [x] Registry monitoring
- [x] TOML configuration
- [x] CLI interface
- [x] Rule engine
- [x] Configuration hot-reloading
- [x] Windows service wrapper
- [x] Metrics and monitoring
- [ ] Web dashboard
- [ ] Plugin system for custom actions

## Support

If you encounter any issues or have questions:
- Open an issue on GitHub
- Check existing issues and documentation
- Review `AGENTS.md` for development questions

---

**Built with ❤️ in Rust for Windows automation**

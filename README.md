# Windows Event Automation Engine

[![Rust](https://img.shields.io/badge/Rust-2024%20Edition-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.2.0-blue.svg)](https://github.com/stylebending/win_event_engine/releases)

A universal event automation system for Windows built in Rust. Monitor file system events, window activity, process creation/termination, and registry changes - then execute automated actions based on configurable rules.

## Features

- **File System Watcher** - Monitor file creation, modification, deletion with pattern matching
- **Window Event Monitor** - Track window focus, creation, destruction using Win32 API
- **Process Monitor** - Kernel-level ETW process monitoring with real-time events (requires Administrator)
- **Registry Monitor** - Kernel-level ETW registry monitoring with real-time events (requires Administrator)
- **Rule Engine** - Pattern-based matching with composite AND/OR logic
- **Lua Scripting** - Write custom actions in Lua without recompiling
- **Web Dashboard** - Real-time monitoring via WebSocket at `http://127.0.0.1:9090`
- **Production Ready** - Windows service support, structured logging, hot-reloading

## Quick Start

### Prerequisites

- Windows 10/11
- [Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe) (usually pre-installed)

### Installation

**Option 1: Download Release**
```bash
# Download from GitHub Releases
# No compilation needed - standalone executable
```

**Option 2: Build from Source**
```bash
git clone https://github.com/stylebending/win_event_engine.git
cd win_event_engine
cargo build --release -p engine
# Executable: target/release/engine.exe
```

### Basic Usage

```bash
# Run with a configuration file
cargo run -p engine -- -c config.toml

# Check status
cargo run -p engine -- --status

# Install as Windows Service (requires admin)
cargo run -p engine -- --install
```

### Simple Configuration

```toml
[engine]
event_buffer_size = 1000
log_level = "info"

[[sources]]
name = "downloads_watcher"
type = "file_watcher"
paths = ["C:/Users/%USERNAME%/Downloads"]
pattern = "*.exe"

[[rules]]
name = "executable_alert"
trigger = { type = "file_created", pattern = "*.exe" }
action = { type = "log", message = "Executable downloaded!", level = "warn" }
```

## Web Dashboard

Access real-time monitoring at `http://127.0.0.1:9090`:

![Dashboard Overview](docs/images/dashboard-overview.png)

The dashboard provides:
- **Live Event Stream** - Watch events as they happen
- **Real-time Charts** - Events/sec, rule matches, action execution
- **System Health** - Uptime, active plugins, rules count
- **WebSocket Updates** - Automatic reconnection, <100ms latency

**Security**: Dashboard is localhost-only (`127.0.0.1`) - cannot be accessed remotely.

For detailed dashboard usage, see the [Web Dashboard Documentation](https://github.com/stylebending/win_event_engine/wiki/Web-Dashboard).

## Lua Scripting

Write custom actions in Lua:

```lua
-- plugins/actions/my_action.lua
function on_event(event)
    log.info("Event: " .. event.kind)
    
    if event.metadata.path then
        local size = fs.file_size(event.metadata.path)
        log.info("File size: " .. size .. " bytes")
    end
    
    return {success = true}
end
```

Reference in config:
```toml
action = { type = "script", path = "my_action.lua", function = "on_event" }
```

**Available APIs:**
- `log.debug/info/warn/error()` - Logging
- `exec.run(cmd, args)` - Execute commands
- `http.get/post(url, options)` - HTTP requests
- `json.encode/decode()` - JSON processing
- `fs.file_size/exists/move/delete()` - File operations (restricted)
- `os.date/time()` - Date/time functions

See [Lua Scripting API](https://github.com/stylebending/win_event_engine/wiki/Lua-Scripting-API) for complete documentation.

## Documentation

- **[Configuration Reference](https://github.com/stylebending/win_event_engine/wiki/Configuration-Reference)** - Complete config options
- **[Event Types](https://github.com/stylebending/win_event_engine/wiki/Event-Types)** - All available event types
- **[Lua Scripting API](https://github.com/stylebending/win_event_engine/wiki/Lua-Scripting-API)** - Writing custom scripts
- **[Web Dashboard](https://github.com/stylebending/win_event_engine/wiki/Web-Dashboard)** - Monitoring and metrics
- **[Troubleshooting](https://github.com/stylebending/win_event_engine/wiki/Troubleshooting)** - Common issues and solutions
- **[Architecture](https://github.com/stylebending/win_event_engine/wiki/Architecture)** - Technical deep-dive

## Project Status

- [x] Core event engine
- [x] File system watcher
- [x] Window event monitor
- [x] Process monitor (ETW)
- [x] Registry monitor (ETW)
- [x] Rule engine with pattern matching
- [x] CLI interface
- [x] Configuration hot-reloading
- [x] Windows service wrapper
- [x] Metrics and monitoring
- [x] Web dashboard (real-time)
- [x] Plugin system for custom actions (Lua scripting)

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Support

- 📖 [Documentation Wiki](https://github.com/stylebending/win_event_engine/wiki)
- 🐛 [Issue Tracker](https://github.com/stylebending/win_event_engine/issues)
- 💡 [Discussions](https://github.com/stylebending/win_event_engine/discussions)

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

**Built with ❤️ in Rust for Windows automation**

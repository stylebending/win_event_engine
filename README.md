<p align="center">

<h3>Windows Event Automation Engine</h3>

</p>

<p align="center">

<a href="https://www.rust-lang.org">
  <img src="https://img.shields.io/badge/Rust%202024%20Edition-orange?style=for-the-badge&logo=rust">
</a>

<a href="https://github.com/stylebending/win_event_engine/releases">
  <img src="https://img.shields.io/github/v/release/stylebending/win_event_engine?style=for-the-badge&color=green&logo=git&logoColor=white&label=Release">
</a>

<a href="https://github.com/stylebending/win_event_engine/releases">
  <img src="https://img.shields.io/github/downloads/stylebending/win_event_engine/total?color=green&logo=github&label=Total%20Downloads&style=for-the-badge">
</a>

</p>

<p align="center">

<a href="https://github.com/stylebending/win_event_engine?tab=MIT-1-ov-file">
  <img src="https://img.shields.io/badge/📄-MIT%20License-blue?style=for-the-badge">
</a>

<a href="https://github.com/stylebending/win_event_engine?tab=contributing-ov-file">
  <img src="https://img.shields.io/badge/🪽-Contributing-blue?style=for-the-badge">
</a>

</p>

<p align="center">

<h3>🚀 Quick Navigation</h3>

</p>

<p align="center">


<a href="https://github.com/stylebending/win_event_engine/wiki">
  <img src="https://img.shields.io/badge/📖-Documentation-blue?style=for-the-badge">
</a>

<a href="https://github.com/stylebending/win_event_engine?tab=readme-ov-file#features">
  <img src="https://img.shields.io/badge/✨-Features-blue?style=for-the-badge">
</a>

<a href="https://github.com/stylebending/win_event_engine?tab=readme-ov-file#first-time-setup-5-minutes">
  <img src="https://img.shields.io/badge/🚀-First%20Time%20Setup-blue?style=for-the-badge">
</a>

<a href="https://github.com/stylebending/win_event_engine?tab=readme-ov-file#next-steps">
  <img src="https://img.shields.io/badge/🧭-Next%20Steps-blue?style=for-the-badge">
</a>

</p>

<p align="center">

<a href="https://github.com/stylebending/win_event_engine?tab=readme-ov-file#running-as-a-windows-service">
  <img src="https://img.shields.io/badge/🪟-Windows%20Service-blue?style=for-the-badge">
</a>

<a href="https://github.com/stylebending/win_event_engine?tab=readme-ov-file#common-commands">
  <img src="https://img.shields.io/badge/💻-Common%20Commands-blue?style=for-the-badge">
</a>

<a href="https://github.com/stylebending/win_event_engine?tab=readme-ov-file#web-dashboard">
  <img src="https://img.shields.io/badge/📊-Web%20Dashboard-blue?style=for-the-badge">
</a>

<a href="https://github.com/stylebending/win_event_engine?tab=readme-ov-file#lua-scripting">
  <img src="https://img.shields.io/badge/🔧-Lua%20Scripting-blue?style=for-the-badge">
</a>

</p>

---

## 📦 What is Win Event Engine?

Win Event Engine is an event-driven automation framework for Windows that allows you to:

- React to system events in real time
- Automate workflows using Lua scripts
- Monitor activity through a web dashboard
- Run as a background Windows service
- Extend behavior with custom event handlers

Automate everything: play/pause media when focusing specific windows, backup files while you work, get Discord/Slack/Telegram notifications for important events, auto-commit code changes, or write easy-to-learn Lua scripts to customize your workflow - like auto-organizing screenshots or tracking daily habits. Simple configuration, powerful results.

## Features

- **File System Watcher** - Monitor file creation, modification, deletion with pattern matching
- **Window Event Monitor** - Track window focus, creation, destruction using Win32 API
- **Process Monitor** - Kernel-level ETW process monitoring with real-time events (requires Administrator)
- **Registry Monitor** - Kernel-level ETW registry monitoring with real-time events (requires Administrator)
- **Rule Engine** - Pattern-based matching with composite AND/OR logic
- **Lua Scripting** - Write custom actions in Lua without recompiling
- **Web Dashboard** - Real-time monitoring via WebSocket at `http://127.0.0.1:9090`
- **Production Ready** - Windows service support, structured logging, hot-reloading

## First Time Setup (5 minutes)

This minimal example verifies everything works. You'll create a file watcher that logs when text files are created.

### Step 1: Download

Download `engine.exe` from [GitHub Releases](https://github.com/stylebending/win_event_engine/releases) and save it to a folder (e.g., `C:\Tools\win_event_engine\`).

**Requirements:**
- Windows 10/11
- [Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe) (usually pre-installed on most systems)

### Step 2: Create Your First Config

Create a file named `config.toml` in the same folder as `engine.exe`. This config watches the `test_folder` subdirectory for new `.txt` files and logs when they're created:

```toml
[engine]
event_buffer_size = 100
log_level = "info"

# Watch for new files in a test directory
[[sources]]
name = "my_watcher"
type = "file_watcher"
paths = ["./test_folder"]
pattern = "*.txt"
enabled = true

# Log when text files are created
[[rules]]
name = "text_file_alert"
description = "Notify when text files are created"
trigger = { type = "file_created", pattern = "*.txt" }
action = { type = "log", message = "New text file created!", level = "info" }
enabled = true
```

### Step 3: Run the Engine

Open a terminal in the folder with `engine.exe`:

```bash
# Create a test folder (in the same folder as engine.exe)
mkdir test_folder

# Start the engine
engine.exe -c config.toml

# The engine is now running!
# You'll see output showing it's watching the test folder
```

### Step 4: View the Dashboard

Open your browser and go to: **http://127.0.0.1:9090**

The dashboard is now ready and waiting for events. You'll see the connection status (green dot = connected).

### Step 5: Test It

In another terminal, create a test file:

```bash
echo "Hello World" > test_folder\test.txt
```

Watch as:
1. The engine logs: `[LUA] New text file created!`
2. The dashboard shows the event in real-time!
3. The event counter increases

## Next Steps

### Try a Real Use Case

Once you've verified the engine works, try **media automation**:

1. Copy `config.media_automation.toml.example` to `config.toml`
2. Edit the `title_contains` value to match your preferred window (e.g., "nvim", "VS Code", "Firefox")
3. Run the engine and switch between windows
4. Your media will automatically pause when you leave that window, and resume when you return

### Learn More

- **Full Documentation** - [GitHub Wiki](https://github.com/stylebending/win_event_engine/wiki)
- **All Config Options** - [Configuration Reference](https://github.com/stylebending/win_event_engine/wiki/Configuration-Reference)
- **Write Custom Scripts** - [Lua Scripting API](https://github.com/stylebending/win_event_engine/wiki/Lua-Scripting-API)
- **More Examples** - See `config.toml.example`

## Running as a Windows Service

Run the engine in the background without keeping a terminal open. The service starts automatically on Windows startup.

### Install the Service

Open an **Administrator terminal in the folder containing `engine.exe`**:

```bash
# Install the service
engine.exe --install

# Start the service
sc start WinEventEngine
```

### Manage the Service

```bash
# Check status
sc query WinEventEngine

# Stop the service
sc stop WinEventEngine

# Uninstall (stops and removes)
engine.exe --uninstall
```

**Notes:**
- Service starts automatically on Windows boot
- Config file path must be absolute or relative to engine.exe location
- Requires Administrator privileges for install/uninstall

## Common Commands

```bash
# View help
engine.exe --help

# Run with a config file
engine.exe -c config.toml

# Check if the engine is running
engine.exe --status

# Enable debug logging
engine.exe -c config.toml --log-level debug

# Dry run (see what would happen without executing)
engine.exe -c config.toml --dry-run

# Install as Windows Service (requires admin terminal)
engine.exe --install

# Start the Service (requires admin terminal)
sc start WinEventEngine

# Stop the Service (requires admin terminal)
sc stop WinEventEngine

# Uninstall as Windows Service (requires admin terminal)
engine.exe --uninstall
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
- **[Lua Scripting API](https://github.com/stylebending/win_event_engine/wiki/Lua-Scripting-API)** - Custom script documentation
- **[Web Dashboard](https://github.com/stylebending/win_event_engine/wiki/Web-Dashboard)** - Monitoring and metrics
- **[Troubleshooting](https://github.com/stylebending/win_event_engine/wiki/Troubleshooting)** - Common issues and solutions
- **[Architecture](https://github.com/stylebending/win_event_engine/wiki/Architecture)** - Technical deep-dive

## For Developers

**Build from Source** (requires [Rust](https://rustup.rs/)):

```bash
git clone https://github.com/stylebending/win_event_engine.git
cd win_event_engine
cargo build --release -p engine
# Executable: target/release/engine.exe
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

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

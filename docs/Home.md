# Windows Event Automation Engine

Welcome to the WinEventEngine documentation! This wiki contains everything you need to understand, configure, and extend the engine.

## What is WinEventEngine?

A universal event automation system for Windows that monitors system events (file changes, window activity, process creation, registry modifications) and executes automated actions based on configurable rules.

Automate everything: backup files while you work, get Discord/Slack/Telegram notifications for important events, auto-commit code changes, or write easy-to-learn Lua scripts to customize your workflow - like auto-organizing screenshots or tracking daily habits. Simple configuration, powerful results.

## Key Features

- **Event Monitoring**: File system, windows, processes, and registry
- **Rule Engine**: Pattern-based matching with Lua scripting
- **Web Dashboard**: Real-time monitoring at `http://localhost:9090`
- **Windows Service**: Run as background service
- **Plugin System**: Write custom actions in Lua

## First Time Setup (5 minutes)

New to WinEventEngine? Follow these steps to get up and running:

### 1. Download

Download `engine.exe` from [GitHub Releases](https://github.com/stylebending/win_event_engine/releases) and save it to a folder (e.g., `C:\Tools\win_event_engine\`).

**Requirements:**
- Windows 10/11
- [Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe) (usually pre-installed on most systems)

### 2. Create Your First Config

Create a file named `config.toml` in the same folder:

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

**Need help with configuration?** See the **[Configuration Reference](Configuration-Reference)** for all available options.

### 3. Run the Engine

Open a terminal in the folder with `engine.exe`:

```bash
# Create a test folder
mkdir test_folder

# Start the engine
engine.exe -c config.toml

# The engine is now running!
```

### 4. View the Dashboard

Open your browser and go to: **http://127.0.0.1:9090**

The dashboard is now ready and waiting for events. You'll see the connection status (green dot = connected).

### 5. Test It

In another terminal, create a test file:

```bash
echo "Hello World" > test_folder\test.txt
```

Watch as:
1. The engine logs: `[LUA] New text file created!`
2. The dashboard shows the event in real-time!
3. The event counter increases

You'll see real-time events as they happen!

## Next Steps

Once you've completed the first time setup, explore these resources:

### Getting Started
- **[Configuration Reference](Configuration-Reference)** - Complete configuration options and examples
- **[Event Types](Event-Types)** - All available events and their data

### Custom Actions
- **[Lua Scripting API](Lua-Scripting-API)** - Write custom actions in Lua

### Monitoring & Debugging
- **[Web Dashboard](Web-Dashboard)** - Real-time monitoring and metrics
- **[Troubleshooting](Troubleshooting)** - Common issues and solutions

### Development
- **[Architecture](Architecture)** - Technical deep-dive into the system
- **[Contributing](https://github.com/stylebending/win_event_engine/blob/main/CONTRIBUTING.md)** - How to contribute to the project

## Quick Reference

### Common Commands

```bash
# Run with a config file
engine.exe -c config.toml

# Check if the engine is running
engine.exe --status

# Enable debug logging
engine.exe -c config.toml --log-level debug

# Dry run (see what would happen without executing)
engine.exe -c config.toml --dry-run

# Install as Windows Service (requires admin)
engine.exe --install

# View help
engine.exe --help
```

### Example Configurations

**Download Alert:**
```toml
[[rules]]
name = "download_alert"
trigger = { type = "file_created", pattern = "*.exe" }
action = { type = "log", message = "Executable downloaded!", level = "warn" }
```

**Auto-backup:**
```toml
[[rules]]
name = "auto_backup"
trigger = { type = "file_modified", pattern = "*.docx" }
action = { type = "script", path = "backup.lua", function = "on_event" }
```

See **[Configuration Reference](Configuration-Reference)** for more examples.

## Quick Links

- [GitHub Repository](https://github.com/stylebending/win_event_engine)
- [Releases](https://github.com/stylebending/win_event_engine/releases)
- [Issues](https://github.com/stylebending/win_event_engine/issues)
- [License: MIT](https://github.com/stylebending/win_event_engine/blob/main/LICENSE)

## Version

Current version: **v0.2.0**

---

*Built with ❤️ in Rust for Windows automation*

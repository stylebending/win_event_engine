# Configuration Reference

Complete guide to configuring the Windows Event Automation Engine.

## Table of Contents

- [Basic Structure](#basic-structure)
- [Engine Settings](#engine-settings)
- [Event Sources](#event-sources)
- [Rules](#rules)
- [Actions](#actions)
- [Examples](#examples)

## Basic Structure

Configuration files use TOML format:

```toml
[engine]
event_buffer_size = 1000
log_level = "info"

[[sources]]
name = "my_watcher"
type = "file_watcher"
paths = ["C:/Data"]
pattern = "*.txt"

[[rules]]
name = "my_rule"
trigger = { type = "file_created", pattern = "*.txt" }
action = { type = "log", message = "File created!" }
```

## Engine Settings

```toml
[engine]
event_buffer_size = 1000      # Max events in buffer (default: 1000)
log_level = "info"            # debug, info, warn, error (default: info)
```

## Event Sources

### File Watcher

```toml
[[sources]]
name = "file_monitor"
type = "file_watcher"
paths = ["C:/Data", "D:/Backup"]    # Directories to watch (required)
pattern = "*.txt"                    # File pattern (optional)
recursive = true                     # Watch subdirectories (default: false)
enabled = true                       # Enable/disable (default: true)
```

### Window Watcher

```toml
[[sources]]
name = "window_monitor"
type = "window_watcher"
enabled = true
```

### Process Monitor

```toml
[[sources]]
name = "process_monitor"
type = "process_monitor"
process_names = ["chrome.exe", "notepad.exe"]  # Filter processes (optional)
enabled = true
```

### Registry Monitor

```toml
[[sources]]
name = "registry_monitor"
type = "registry_monitor"
keys = [
    { root = "HKEY_LOCAL_MACHINE", path = "SOFTWARE", watch_tree = true }
]
enabled = true
```

### Timer

```toml
[[sources]]
name = "hourly_timer"
type = "timer"
interval_seconds = 3600  # Trigger every hour
enabled = true
```

## Rules

### Basic Rule Structure

```toml
[[rules]]
name = "rule_name"              # Unique name
description = "What this does"  # Optional description
enabled = true                  # Enable/disable

[rules.trigger]                 # When to trigger
type = "file_created"
pattern = "*.txt"

[rules.action]                  # What to do
type = "log"
message = "File created!"
```

### Multiple Actions

```toml
[[rules]]
name = "multi_action"
trigger = { type = "file_created" }

[[rules.action]]
type = "log"
message = "Step 1"

[[rules.action]]
type = "execute"
command = "backup.exe"
```

## Actions

### Log

```toml
action = { type = "log", message = "Event occurred", level = "info" }
```

Levels: `debug`, `info`, `warn`, `error`

### Execute Command

```toml
action = { 
    type = "execute", 
    command = "notepad.exe",
    args = ["file.txt"],
    working_dir = "C:/Temp"
}
```

### PowerShell

```toml
action = { 
    type = "powershell", 
    script = """
        Write-Host "Event: $env:EVENT_PATH"
    """,
    working_dir = "C:/Scripts"
}
```

### HTTP Request

```toml
action = { 
    type = "http_request", 
    url = "https://api.example.com/webhook",
    method = "POST",
    headers = { "Authorization" = "Bearer token" },
    body = '{"event": "{{EVENT_PATH}}"}'
}
```

### Lua Script

```toml
action = { 
    type = "script", 
    path = "my_script.lua",
    function = "on_event",
    timeout_ms = 30000,
    on_error = "fail"  # fail, continue, or log
}
```

### Media Control

```toml
action = { type = "media", command = "play" }   # play, pause, toggle
```

## Examples

### Monitor Downloads for Executables

```toml
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

### Backup Important Files

```toml
[[sources]]
name = "important_files"
type = "file_watcher"
paths = ["C:/Important"]
pattern = "*.docx"

[[rules]]
name = "backup_docs"
trigger = { type = "file_modified" }
action = { 
    type = "script", 
    path = "backup.lua",
    function = "on_event"
}
```

### Auto-commit on Config Changes

```toml
[[sources]]
name = "git_repo_watcher"
type = "file_watcher"
paths = ["C:/Projects/MyRepo"]
pattern = "*.toml"

[[rules]]
name = "auto_commit"
trigger = { type = "file_modified" }
action = { 
    type = "script", 
    path = "git_autocommit.lua",
    on_error = "log"
}
```

## Environment Variables

Actions have access to these environment variables:

- `EVENT_PATH` - Path to the file (file events)
- `EVENT_TYPE` - Type of event
- `EVENT_SOURCE` - Source plugin name

## See Also

- [Event Types](Event-Types.md) - All available event types
- [Lua Scripting API](Lua-Scripting-API.md) - Custom script documentation
- [Troubleshooting](Troubleshooting.md) - Common configuration issues

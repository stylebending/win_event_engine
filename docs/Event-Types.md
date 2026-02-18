# Event Types

Complete reference of all event types supported by the engine.

## Table of Contents

- [File System Events](#file-system-events)
- [Window Events](#window-events)
- [Process Events](#process-events)
- [Registry Events](#registry-events)
- [Timer Events](#timer-events)

## File System Events

Triggered by the `file_watcher` source.

### FileCreated

Fired when a new file is created.

```lua
-- Event structure:
{
    kind = "FileCreated",
    source = "file_watcher",
    metadata = {
        path = "C:/Data/file.txt"
    }
}
```

### FileModified

Fired when a file is modified.

```lua
{
    kind = "FileModified",
    metadata = {
        path = "C:/Data/file.txt"
    }
}
```

### FileDeleted

Fired when a file is deleted.

```lua
{
    kind = "FileDeleted",
    metadata = {
        path = "C:/Data/file.txt"
    }
}
```

### FileRenamed

Fired when a file is renamed.

```lua
{
    kind = "FileRenamed",
    metadata = {
        old_path = "C:/Data/old.txt",
        new_path = "C:/Data/new.txt"
    }
}
```

## Window Events

Triggered by the `window_watcher` source.

### WindowCreated

Fired when a new window is opened.

```lua
{
    kind = "WindowCreated",
    source = "window_watcher",
    metadata = {
        title = "Untitled - Notepad",
        class = "Notepad",
        exe = "notepad.exe",
        pid = "1234"
    }
}
```

### WindowDestroyed

Fired when a window is closed.

```lua
{
    kind = "WindowDestroyed",
    metadata = {
        title = "Untitled - Notepad",
        class = "Notepad",
        exe = "notepad.exe"
    }
}
```

### WindowFocused

Fired when a window gains focus.

```lua
{
    kind = "WindowFocused",
    metadata = {
        title = "Document.docx - Word",
        exe = "winword.exe"
    }
}
```

### WindowUnfocused

Fired when a window loses focus.

```lua
{
    kind = "WindowUnfocused",
    metadata = {
        title = "Document.docx - Word",
        exe = "winword.exe"
    }
}
```

### WindowTitleChanged

Fired when a window title changes.

```lua
{
    kind = "WindowTitleChanged",
    metadata = {
        title = "New Title",
        old_title = "Old Title",
        exe = "chrome.exe"
    }
}
```

## Process Events

Triggered by the `process_monitor` source.

### ProcessStarted

Fired when a process starts.

```lua
{
    kind = "ProcessStarted",
    source = "process_monitor",
    metadata = {
        process_name = "chrome.exe",
        pid = "1234",
        command_line = "C:\\Program Files\\Chrome\\chrome.exe"
    }
}
```

### ProcessStopped

Fired when a process ends.

```lua
{
    kind = "ProcessStopped",
    metadata = {
        process_name = "chrome.exe",
        pid = "1234",
        exit_code = "0"
    }
}
```

## Registry Events

Triggered by the `registry_monitor` source.

### RegistryKeyCreated

Fired when a registry key is created.

```lua
{
    kind = "RegistryKeyCreated",
    source = "registry_monitor",
    metadata = {
        key_path = "HKLM\\SOFTWARE\\MyApp",
        process_id = "1234"
    }
}
```

### RegistryKeyDeleted

Fired when a registry key is deleted.

```lua
{
    kind = "RegistryKeyDeleted",
    metadata = {
        key_path = "HKLM\\SOFTWARE\\OldApp"
    }
}
```

### RegistryValueSet

Fired when a registry value is created or modified.

```lua
{
    kind = "RegistryValueSet",
    metadata = {
        key_path = "HKLM\\SOFTWARE\\MyApp",
        value_name = "InstallPath",
        data_type = "REG_SZ",
        data_size = "42"
    }
}
```

### RegistryValueDeleted

Fired when a registry value is deleted.

```lua
{
    kind = "RegistryValueDeleted",
    metadata = {
        key_path = "HKLM\\SOFTWARE\\MyApp",
        value_name = "OldValue"
    }
}
```

## Timer Events

Triggered by the `timer` source.

### TimerTick

Fired at regular intervals.

```lua
{
    kind = "TimerTick",
    source = "hourly_timer",
    metadata = {
        interval_seconds = "3600",
        tick_count = "42"
    }
}
```

## Common Event Fields

All events include these fields:

```lua
{
    id = "uuid-string",
    timestamp = "2024-01-15T10:30:00+00:00",
    kind = "EventType",
    source = "plugin_name",
    metadata = {
        -- Event-specific fields
    }
}
```

## Pattern Matching

Use patterns in triggers:

```toml
# Match specific file types
trigger = { type = "file_created", pattern = "*.txt" }

# Match process names
trigger = { type = "process_started", process_name = "chrome.exe" }

# Match window titles (substring)
trigger = { type = "window_focused", title_contains = "Visual Studio" }
```

## See Also

- [Configuration Reference](Configuration-Reference.md) - How to configure event sources
- [Lua Scripting API](Lua-Scripting-API.md) - Handling events in scripts

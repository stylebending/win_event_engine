# Lua Scripting API

Complete API reference for writing Lua scripts to handle events.

## Table of Contents

- [Getting Started](#getting-started)
- [Event Object](#event-object)
- [Logging](#logging)
- [Command Execution](#command-execution)
- [HTTP Requests](#http-requests)
- [JSON](#json)
- [File System](#file-system)
- [Date/Time](#datetime)
- [Examples](#examples)

## Getting Started

Create a script in `plugins/actions/`:

```lua
-- plugins/actions/my_script.lua
function on_event(event)
    log.info("Got event: " .. event.kind)
    return {success = true}
end
```

Reference it in your config:

```toml
[[rules]]
action = { 
    type = "script", 
    path = "my_script.lua",
    function = "on_event"
}
```

## Event Object

The `event` parameter contains:

```lua
{
    id = "uuid-string",              -- Unique event ID
    timestamp = "2024-01-15T...",    -- ISO 8601 timestamp
    kind = "FileCreated",            -- Event type
    source = "file_watcher",         -- Source plugin name
    metadata = {                     -- Event-specific data
        path = "C:/file.txt",
        -- ... other fields
    }
}
```

### Accessing Metadata

```lua
function on_event(event)
    -- Check if path exists in metadata
    if event.metadata.path then
        local path = event.metadata.path
        log.info("File: " .. path)
    end
    
    -- Iterate all metadata
    for key, value in pairs(event.metadata) do
        log.info(key .. " = " .. value)
    end
end
```

## Logging

Four log levels are available:

```lua
log.debug("Debug message")     -- Detailed information
log.info("Info message")       -- General information
log.warn("Warning message")    -- Warning events
log.error("Error message")     -- Error events
```

Logs appear in the engine output with `[LUA]` prefix.

## Command Execution

Execute system commands:

```lua
local result = exec.run("cmd.exe", {"arg1", "arg2"})

-- Result structure:
-- {
--     exit_code = 0,
--     stdout = "output text",
--     stderr = "error text"
-- }
```

### Examples

```lua
-- Run PowerShell
local result = exec.run("powershell.exe", {
    "-Command", 
    "Get-Process | Select-Object -First 5"
})

if result.exit_code == 0 then
    log.info("Output: " .. result.stdout)
else
    log.error("Failed: " .. result.stderr)
end

-- Copy a file
exec.run("cmd.exe", {"/c", "copy", source, dest})
```

## HTTP Requests

Make HTTP GET and POST requests:

### GET Request

```lua
local response = http.get("https://api.example.com/data", {
    headers = {
        ["Authorization"] = "Bearer token123"
    }
})

-- Response structure:
-- {
--     status = 200,
--     body = "response text"
-- }
```

### POST Request

```lua
local response = http.post("https://hooks.slack.com/webhook", {
    body = json.encode({
        text = "Alert!"
    }),
    headers = {
        ["Content-Type"] = "application/json"
    }
})
```

### Error Handling

```lua
if response.status >= 200 and response.status < 300 then
    log.info("Success: " .. response.body)
else
    log.error("HTTP " .. response.status .. ": " .. response.body)
end
```

## JSON

Encode and decode JSON:

### Encoding

```lua
local data = {
    event_type = event.kind,
    source = event.source,
    processed = true
}

local json_string = json.encode(data)
-- Result: '{"event_type":"FileCreated","source":"file_watcher","processed":true}'
```

### Decoding

```lua
local json_string = '{"name":"test","value":42}'
local table = json.decode(json_string)

log.info("Name: " .. table.name)        -- "test"
log.info("Value: " .. table.value)      -- 42
```

## File System

File system operations (restricted to safe directories):

### Check File Existence

```lua
if fs.exists("C:/path/to/file.txt") then
    log.info("File exists")
end
```

### Get File Size

```lua
local size = fs.file_size("path/to/file.txt")
if size >= 0 then
    log.info("Size: " .. size .. " bytes")
end
```

### Get Filename

```lua
local filename = fs.basename("/path/to/file.txt")
-- Result: "file.txt"
```

### Move File

```lua
local success = fs.move("source.txt", "dest.txt")
if success then
    log.info("File moved")
end
```

### Delete File

```lua
local success = fs.delete("old_file.txt")
if success then
    log.info("File deleted")
end
```

**Note**: File operations are restricted to:
- Current working directory
- Temp directory
- Documents folder
- Relative paths

## Date/Time

Get current time:

```lua
-- Unix timestamp
local timestamp = os.time()
-- Result: 1705312800

-- Formatted date
local datetime = os.date("%Y-%m-%d %H:%M:%S")
-- Result: "2024-01-15 10:30:00"

-- Custom format
local date_only = os.date("%Y-%m-%d")
-- Result: "2024-01-15"
```

## Return Values

Scripts must return a table:

```lua
-- Success
return {
    success = true,
    message = "Processed successfully"
}

-- Failure (respects on_error setting)
return {
    success = false,
    message = "Something went wrong"
}
```

## Configuration Options

```toml
[rules.action]
type = "script"
path = "script.lua"
function = "on_event"           # Function to call (default: "on_event")
timeout_ms = 30000              # Execution timeout (default: 30000)
on_error = "fail"               # fail | continue | log (default: fail)
```

### Error Behavior

- **fail**: Stop rule execution, log error
- **continue**: Continue with next action, log warning
- **log**: Only log error, continue silently

## Examples

### Discord Webhook

```lua
function on_event(event)
    local webhook = "https://discord.com/api/webhooks/..."
    
    local payload = {
        content = "Event: " .. event.kind,
        embeds = {{
            title = event.source,
            fields = {
                {name = "Time", value = event.timestamp}
            }
        }}
    }
    
    http.post(webhook, {
        body = json.encode(payload),
        headers = {["Content-Type"] = "application/json"}
    })
    
    return {success = true}
end
```

### Smart Backup

```lua
function on_event(event)
    local path = event.metadata.path
    local size = fs.file_size(path)
    
    -- Only backup files > 1MB
    if size > 1048576 then
        local timestamp = os.date("%Y%m%d_%H%M%S")
        local backup = "backups/" .. timestamp .. "_" .. fs.basename(path)
        
        exec.run("cmd.exe", {"/c", "copy", path, backup})
        log.info("Backed up to: " .. backup)
    end
    
    return {success = true}
end
```

### VirusTotal Check (Template)

```lua
function on_event(event)
    local path = event.metadata.path
    
    -- Check file hash
    local result = exec.run("certutil.exe", {"-hashfile", path, "SHA256"})
    local hash = string.match(result.stdout, "SHA256.-%r-%n(%x+)%r-%n")
    
    -- Query your malware API
    -- (Implementation depends on your API)
    
    return {success = true}
end
```

## Security

Scripts run in a sandboxed environment:

- ❌ No access to `dofile`, `loadfile`, `require`
- ❌ No access to `io` library
- ❌ No access to `os` library (except safe functions)
- ❌ No access to `debug` library
- ✅ File operations restricted to safe directories
- ✅ 30-second timeout by default
- ✅ Runs in isolated Lua state per execution

## See Also

- [Configuration Reference](Configuration-Reference.md) - Full config options
- [Event Types](Event-Types.md) - Available event types
- [Troubleshooting](Troubleshooting.md) - Common script issues

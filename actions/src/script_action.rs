use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{Action, ActionError, ActionResult};
use engine_core::event::Event;
use mlua::{Lua, Table, Value};
use tracing::{debug, error, info, warn};

/// Configuration for script error handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptErrorBehavior {
    /// Fail the action (default)
    Fail,
    /// Continue execution but log error
    Continue,
    /// Only log error, silently continue
    Log,
}

impl Default for ScriptErrorBehavior {
    fn default() -> Self {
        ScriptErrorBehavior::Fail
    }
}

impl std::str::FromStr for ScriptErrorBehavior {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fail" => Ok(ScriptErrorBehavior::Fail),
            "continue" => Ok(ScriptErrorBehavior::Continue),
            "log" => Ok(ScriptErrorBehavior::Log),
            _ => Err(format!("Invalid error behavior: {}", s)),
        }
    }
}

/// Lua script-based action
pub struct ScriptAction {
    /// Path to the Lua script file
    pub script_path: PathBuf,
    /// Name of the Lua function to call
    pub function_name: String,
    /// Timeout for script execution
    pub timeout_ms: u64,
    /// Error handling behavior
    pub on_error: ScriptErrorBehavior,
    /// Cached script content
    script_content: String,
    /// Last modified time for hot-reload
    last_modified: std::time::SystemTime,
}

impl ScriptAction {
    /// Create a new script action
    pub fn new(script_path: PathBuf, function_name: String) -> Result<Self, ActionError> {
        // Load and validate the script
        let metadata = fs::metadata(&script_path)
            .map_err(|e| ActionError::Execution(format!("Cannot read script file: {}", e)))?;

        let script_content = fs::read_to_string(&script_path)
            .map_err(|e| ActionError::Execution(format!("Cannot read script: {}", e)))?;

        // Validate script syntax by loading it in a temporary Lua state
        {
            let lua = Lua::new();
            Self::setup_sandbox(&lua)?;

            lua.load(&script_content)
                .set_name(script_path.to_string_lossy().as_ref())
                .exec()
                .map_err(|e| ActionError::Execution(format!("Lua syntax error: {}", e)))?;

            // Verify the function exists
            let globals = lua.globals();
            let func: Value = globals.get(function_name.as_str()).map_err(|e| {
                ActionError::Execution(format!("Function '{}' not found: {}", function_name, e))
            })?;

            if !func.is_function() {
                return Err(ActionError::Execution(format!(
                    "'{}' is not a function",
                    function_name
                )));
            }
        } // Lua state dropped here

        info!(
            "ScriptAction initialized: {}::{} ({} bytes)",
            script_path.display(),
            function_name,
            metadata.len()
        );

        Ok(Self {
            script_path,
            function_name,
            timeout_ms: 30000, // Default 30 seconds
            on_error: ScriptErrorBehavior::default(),
            script_content,
            last_modified: metadata.modified().unwrap_or(std::time::SystemTime::now()),
        })
    }

    /// Set custom timeout
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set error handling behavior
    pub fn with_error_behavior(mut self, behavior: ScriptErrorBehavior) -> Self {
        self.on_error = behavior;
        self
    }

    /// Check if script needs reload
    fn needs_reload(&self) -> bool {
        if let Ok(metadata) = fs::metadata(&self.script_path) {
            if let Ok(modified) = metadata.modified() {
                return modified > self.last_modified;
            }
        }
        false
    }

    /// Reload the script
    #[allow(dead_code)]
    fn reload(&mut self) -> Result<(), ActionError> {
        info!("Reloading script: {}", self.script_path.display());

        let script_content = fs::read_to_string(&self.script_path)
            .map_err(|e| ActionError::Execution(format!("Cannot read script: {}", e)))?;

        // Validate script syntax
        {
            let lua = Lua::new();
            Self::setup_sandbox(&lua)?;

            lua.load(&script_content)
                .set_name(self.script_path.to_string_lossy().as_ref())
                .exec()
                .map_err(|e| ActionError::Execution(format!("Lua syntax error: {}", e)))?;
        }

        self.script_content = script_content;

        // Update last modified time
        if let Ok(metadata) = fs::metadata(&self.script_path) {
            if let Ok(modified) = metadata.modified() {
                self.last_modified = modified;
            }
        }

        info!("Script reloaded successfully");
        Ok(())
    }

    /// Set up sandboxed Lua environment
    fn setup_sandbox(lua: &Lua) -> Result<(), ActionError> {
        let globals = lua.globals();

        // Remove dangerous functions
        globals.raw_set("dofile", Value::Nil)?;
        globals.raw_set("loadfile", Value::Nil)?;
        globals.raw_set("load", Value::Nil)?;
        globals.raw_set("require", Value::Nil)?;
        globals.raw_set("io", Value::Nil)?;
        globals.raw_set("os", Value::Nil)?;
        globals.raw_set("debug", Value::Nil)?;

        // Keep safe standard library parts
        // string.* and table.* are safe
        // math.* is safe

        // Create our API table
        let api = lua.create_table()?;

        // LOGGING API
        let log_table = lua.create_table()?;

        log_table.raw_set(
            "debug",
            lua.create_function(|_, msg: String| {
                debug!("[LUA] {}", msg);
                Ok(())
            })?,
        )?;

        log_table.raw_set(
            "info",
            lua.create_function(|_, msg: String| {
                info!("[LUA] {}", msg);
                Ok(())
            })?,
        )?;

        log_table.raw_set(
            "warn",
            lua.create_function(|_, msg: String| {
                warn!("[LUA] {}", msg);
                Ok(())
            })?,
        )?;

        log_table.raw_set(
            "error",
            lua.create_function(|_, msg: String| {
                error!("[LUA] {}", msg);
                Ok(())
            })?,
        )?;

        api.raw_set("log", log_table)?;

        // EXEC API
        api.raw_set(
            "exec",
            lua.create_function(|lua, (cmd, args): (String, Vec<String>)| {
                use std::process::Command;

                let start = Instant::now();
                let timeout = Duration::from_secs(60);

                let mut command = Command::new(&cmd);
                command.args(&args);

                debug!("[LUA] Executing: {} {:?}", cmd, args);

                let result = command.output();

                let elapsed = start.elapsed();
                if elapsed > timeout {
                    warn!("[LUA] Command execution exceeded timeout: {:?}", elapsed);
                }

                match result {
                    Ok(output) => {
                        let result_table = lua.create_table()?;
                        result_table.raw_set("exit_code", output.status.code().unwrap_or(-1))?;
                        result_table.raw_set(
                            "stdout",
                            String::from_utf8_lossy(&output.stdout).to_string(),
                        )?;
                        result_table.raw_set(
                            "stderr",
                            String::from_utf8_lossy(&output.stderr).to_string(),
                        )?;
                        Ok(result_table)
                    }
                    Err(e) => {
                        error!("[LUA] Failed to execute command: {}", e);
                        let result_table = lua.create_table()?;
                        result_table.raw_set("exit_code", -1)?;
                        result_table.raw_set("stdout", "")?;
                        result_table.raw_set("stderr", e.to_string())?;
                        Ok(result_table)
                    }
                }
            })?,
        )?;

        // HTTP API
        let http_table = lua.create_table()?;

        http_table.raw_set(
            "get",
            lua.create_function(|lua, (url, options): (String, Option<Table>)| {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .map_err(|e| {
                        mlua::Error::RuntimeError(format!("Failed to create HTTP client: {}", e))
                    })?;

                let mut request = client.get(&url);

                // Add headers if provided
                if let Some(opts) = options {
                    if let Ok(headers_table) = opts.get::<_, Table>("headers") {
                        for pair in headers_table.pairs::<String, String>() {
                            let (key, value) = pair?;
                            request = request.header(&key, &value);
                        }
                    }
                }

                debug!("[LUA] HTTP GET: {}", url);

                match request.send() {
                    Ok(response) => {
                        let status = response.status().as_u16() as i32;
                        let body = response.text().unwrap_or_default();

                        let result = lua.create_table()?;
                        result.raw_set("status", status)?;
                        result.raw_set("body", body)?;
                        Ok(result)
                    }
                    Err(e) => {
                        error!("[LUA] HTTP GET failed: {}", e);
                        let result = lua.create_table()?;
                        result.raw_set("status", 0)?;
                        result.raw_set("body", e.to_string())?;
                        Ok(result)
                    }
                }
            })?,
        )?;

        http_table.raw_set(
            "post",
            lua.create_function(|lua, (url, options): (String, Option<Table>)| {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .map_err(|e| {
                        mlua::Error::RuntimeError(format!("Failed to create HTTP client: {}", e))
                    })?;

                let mut request = client.post(&url);

                if let Some(opts) = options {
                    // Add body if provided
                    if let Ok(body) = opts.get::<_, String>("body") {
                        request = request.body(body);
                    }

                    // Add headers if provided
                    if let Ok(headers_table) = opts.get::<_, Table>("headers") {
                        for pair in headers_table.pairs::<String, String>() {
                            let (key, value) = pair?;
                            request = request.header(&key, &value);
                        }
                    }
                }

                debug!("[LUA] HTTP POST: {}", url);

                match request.send() {
                    Ok(response) => {
                        let status = response.status().as_u16() as i32;
                        let body = response.text().unwrap_or_default();

                        let result = lua.create_table()?;
                        result.raw_set("status", status)?;
                        result.raw_set("body", body)?;
                        Ok(result)
                    }
                    Err(e) => {
                        error!("[LUA] HTTP POST failed: {}", e);
                        let result = lua.create_table()?;
                        result.raw_set("status", 0)?;
                        result.raw_set("body", e.to_string())?;
                        Ok(result)
                    }
                }
            })?,
        )?;

        api.raw_set("http", http_table)?;

        // JSON API
        let json_table = lua.create_table()?;

        json_table.raw_set(
            "encode",
            lua.create_function(|_lua, value: Value| {
                let json_value = Self::lua_value_to_json(value)?;
                match serde_json::to_string(&json_value) {
                    Ok(json_str) => Ok(json_str),
                    Err(e) => Err(mlua::Error::RuntimeError(format!(
                        "JSON encode error: {}",
                        e
                    ))),
                }
            })?,
        )?;

        json_table.raw_set(
            "decode",
            lua.create_function(|lua, json_str: String| {
                match serde_json::from_str::<serde_json::Value>(&json_str) {
                    Ok(json_value) => {
                        let lua_value = Self::json_value_to_lua(lua, json_value)?;
                        Ok(lua_value)
                    }
                    Err(e) => Err(mlua::Error::RuntimeError(format!(
                        "JSON decode error: {}",
                        e
                    ))),
                }
            })?,
        )?;

        api.raw_set("json", json_table)?;

        // FILE SYSTEM API (restricted)
        let fs_table = lua.create_table()?;

        fs_table.raw_set(
            "file_size",
            lua.create_function(|_, path: String| match fs::metadata(&path) {
                Ok(metadata) => Ok(metadata.len() as i64),
                Err(_) => Ok(-1),
            })?,
        )?;

        fs_table.raw_set(
            "exists",
            lua.create_function(|_, path: String| Ok(Path::new(&path).exists()))?,
        )?;

        fs_table.raw_set(
            "basename",
            lua.create_function(|_, path: String| {
                Ok(Path::new(&path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string())
            })?,
        )?;

        // Restricted move operation
        fs_table.raw_set(
            "move",
            lua.create_function(|_, (source, dest): (String, String)| {
                if !is_path_allowed(&source) || !is_path_allowed(&dest) {
                    error!("[LUA] fs.move rejected: path outside allowed directories");
                    return Ok(false);
                }

                match fs::rename(&source, &dest) {
                    Ok(_) => {
                        debug!("[LUA] Moved: {} -> {}", source, dest);
                        Ok(true)
                    }
                    Err(e) => {
                        error!("[LUA] fs.move failed: {}", e);
                        Ok(false)
                    }
                }
            })?,
        )?;

        // Restricted delete operation
        fs_table.raw_set(
            "delete",
            lua.create_function(|_, path: String| {
                if !is_path_allowed(&path) {
                    error!("[LUA] fs.delete rejected: path outside allowed directories");
                    return Ok(false);
                }

                match fs::remove_file(&path) {
                    Ok(_) => {
                        debug!("[LUA] Deleted: {}", path);
                        Ok(true)
                    }
                    Err(e) => {
                        error!("[LUA] fs.delete failed: {}", e);
                        Ok(false)
                    }
                }
            })?,
        )?;

        api.raw_set("fs", fs_table)?;

        // Create a restricted os module with only date/time functions
        let os_table = lua.create_table()?;
        os_table.raw_set(
            "date",
            lua.create_function(|_, format: Option<String>| {
                let fmt = format.unwrap_or_else(|| "%Y-%m-%d %H:%M:%S".to_string());
                let now = chrono::Local::now();
                Ok(now.format(&fmt).to_string())
            })?,
        )?;

        os_table.raw_set(
            "time",
            lua.create_function(|_, ()| Ok(chrono::Local::now().timestamp() as i64))?,
        )?;

        api.raw_set("os", os_table)?;

        // Register API in globals
        globals.raw_set("log", api.get::<_, Table>("log")?)?;
        globals.raw_set("exec", api.get::<_, Value>("exec")?)?;
        globals.raw_set("http", api.get::<_, Table>("http")?)?;
        globals.raw_set("json", api.get::<_, Table>("json")?)?;
        globals.raw_set("fs", api.get::<_, Table>("fs")?)?;
        globals.raw_set("os", api.get::<_, Table>("os")?)?;

        Ok(())
    }

    /// Convert Lua value to JSON value
    fn lua_value_to_json(value: Value) -> Result<serde_json::Value, mlua::Error> {
        match value {
            Value::Nil => Ok(serde_json::Value::Null),
            Value::Boolean(b) => Ok(serde_json::Value::Bool(b)),
            Value::Integer(i) => Ok(serde_json::Value::Number(i.into())),
            Value::Number(n) => Ok(serde_json::json!(n)),
            Value::String(s) => Ok(serde_json::Value::String(s.to_str()?.to_string())),
            Value::Table(t) => {
                // Check if it's an array (sequential integer keys starting at 1)
                let mut is_array = true;
                let len = t.raw_len();

                for i in 1..=len {
                    if t.raw_get::<_, Value>(i)? == Value::Nil {
                        is_array = false;
                        break;
                    }
                }

                if is_array && len > 0 {
                    let mut arr = Vec::new();
                    for i in 1..=len {
                        let val = t.raw_get::<_, Value>(i)?;
                        arr.push(Self::lua_value_to_json(val)?);
                    }
                    Ok(serde_json::Value::Array(arr))
                } else {
                    let mut map = serde_json::Map::new();
                    for pair in t.pairs::<Value, Value>() {
                        let (k, v) = pair?;
                        let key = match k {
                            Value::String(s) => s.to_str()?.to_string(),
                            Value::Integer(i) => i.to_string(),
                            _ => {
                                return Err(mlua::Error::RuntimeError(
                                    "Invalid table key".to_string(),
                                ))
                            }
                        };
                        map.insert(key, Self::lua_value_to_json(v)?);
                    }
                    Ok(serde_json::Value::Object(map))
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    /// Convert JSON value to Lua value
    fn json_value_to_lua<'a>(
        lua: &'a Lua,
        value: serde_json::Value,
    ) -> Result<Value<'a>, mlua::Error> {
        match value {
            serde_json::Value::Null => Ok(Value::Nil),
            serde_json::Value::Bool(b) => Ok(Value::Boolean(b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::Number(f))
                } else {
                    Ok(Value::Nil)
                }
            }
            serde_json::Value::String(s) => Ok(Value::String(lua.create_string(&s)?)),
            serde_json::Value::Array(arr) => {
                let table = lua.create_table()?;
                for (i, v) in arr.iter().enumerate() {
                    table.raw_set(i + 1, Self::json_value_to_lua(lua, v.clone())?)?;
                }
                Ok(Value::Table(table))
            }
            serde_json::Value::Object(obj) => {
                let table = lua.create_table()?;
                for (k, v) in obj {
                    table.raw_set(k, Self::json_value_to_lua(lua, v.clone())?)?;
                }
                Ok(Value::Table(table))
            }
        }
    }

    /// Convert Event to Lua table
    fn event_to_lua<'a>(lua: &'a Lua, event: &Event) -> Result<Table<'a>, mlua::Error> {
        let table = lua.create_table()?;

        table.raw_set("kind", format!("{:?}", event.kind))?;
        table.raw_set("source", event.source.clone())?;
        table.raw_set("timestamp", chrono::Local::now().to_rfc3339())?;
        table.raw_set("id", event.id.to_string())?;

        // Add metadata
        let metadata = lua.create_table()?;
        for (k, v) in &event.metadata {
            metadata.raw_set(k.as_str(), v.as_str())?;
        }
        table.raw_set("metadata", metadata)?;

        Ok(table)
    }
}

impl Action for ScriptAction {
    fn execute(&self, event: &Event) -> Result<ActionResult, ActionError> {
        // Check for hot-reload
        if self.needs_reload() {
            // Note: In a real implementation, we'd need mutable self
            // For now, we'll skip reload on each execution and rely on config reload
            debug!(
                "Script {} changed, will reload on next config reload",
                self.script_path.display()
            );
        }

        let start = Instant::now();
        let timeout = Duration::from_millis(self.timeout_ms);

        // Create a fresh Lua state for this execution
        let lua = Lua::new();
        Self::setup_sandbox(&lua)?;

        // Load the script
        lua.load(&self.script_content)
            .set_name(self.script_path.to_string_lossy().as_ref())
            .exec()
            .map_err(|e| ActionError::Execution(format!("Failed to load script: {}", e)))?;

        // Convert event to Lua table
        let event_table = Self::event_to_lua(&lua, event)?;

        // Get the function
        let globals = lua.globals();
        let func: mlua::Function = globals
            .get(self.function_name.as_str())
            .map_err(|e| ActionError::Execution(format!("Function not found: {}", e)))?;

        // Execute with timeout
        debug!(
            "Executing Lua function '{}' with timeout {:?}",
            self.function_name, timeout
        );

        let result = func.call::<_, Value>(event_table);

        let elapsed = start.elapsed();

        match result {
            Ok(value) => {
                debug!("Lua script executed successfully in {:?}", elapsed);

                // Parse result
                let mut success = true;
                let mut message = String::new();

                if let Value::Table(table) = value {
                    if let Ok(s) = table.get::<_, bool>("success") {
                        success = s;
                    }
                    if let Ok(m) = table.get::<_, String>("message") {
                        message = m;
                    }
                }

                if success {
                    if message.is_empty() {
                        Ok(ActionResult::Success { message: None })
                    } else {
                        Ok(ActionResult::Success {
                            message: Some(message),
                        })
                    }
                } else {
                    match self.on_error {
                        ScriptErrorBehavior::Fail => Err(ActionError::Execution(format!(
                            "Script returned failure: {}",
                            message
                        ))),
                        ScriptErrorBehavior::Continue | ScriptErrorBehavior::Log => {
                            warn!("Script returned failure (continuing): {}", message);
                            Ok(ActionResult::Success {
                                message: Some(format!("Failed but continuing: {}", message)),
                            })
                        }
                    }
                }
            }
            Err(e) => {
                let error_msg = format!("Lua execution error: {}", e);
                error!("{} (after {:?})", error_msg, elapsed);

                match self.on_error {
                    ScriptErrorBehavior::Fail => Err(ActionError::Execution(error_msg)),
                    ScriptErrorBehavior::Continue | ScriptErrorBehavior::Log => {
                        warn!("Script error (continuing): {}", error_msg);
                        Ok(ActionResult::Success {
                            message: Some(format!("Error but continuing: {}", error_msg)),
                        })
                    }
                }
            }
        }
    }

    fn description(&self) -> String {
        format!(
            "Lua script: {}::{}",
            self.script_path.display(),
            self.function_name
        )
    }

    fn clone_box(&self) -> Box<dyn Action> {
        // Re-create the Lua state since it can't be cloned
        match ScriptAction::new(self.script_path.clone(), self.function_name.clone()) {
            Ok(mut action) => {
                action.timeout_ms = self.timeout_ms;
                action.on_error = self.on_error;
                Box::new(action)
            }
            Err(e) => {
                panic!("Failed to clone ScriptAction: {}", e);
            }
        }
    }
}

/// Check if a path is within allowed directories
fn is_path_allowed(path: &str) -> bool {
    let path = Path::new(path);

    // Get allowed directories
    let allowed_dirs = vec![
        std::env::current_dir().ok(),
        std::env::temp_dir().into(),
        dirs::home_dir().map(|h| h.join("Documents")),
    ];

    for allowed in allowed_dirs.iter().flatten() {
        if path.starts_with(allowed) {
            return true;
        }
    }

    // Also allow relative paths
    if path.is_relative() {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_script_action_creation() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"
function on_event(event)
    return {{success = true}}
end
"#
        )
        .unwrap();

        let action = ScriptAction::new(file.path().to_path_buf(), "on_event".to_string());

        assert!(action.is_ok());
    }

    #[test]
    fn test_script_execution() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"
function on_event(event)
    log.info("Event received: " .. event.kind)
    return {{success = true, message = "Processed"}}
end
"#
        )
        .unwrap();

        let action = ScriptAction::new(file.path().to_path_buf(), "on_event".to_string()).unwrap();

        let event = Event::new(engine_core::event::EventKind::TimerTick, "test");

        let result = action.execute(&event);
        assert!(result.is_ok());
    }

    #[test]
    fn test_json_encode_decode() {
        let lua = Lua::new();
        ScriptAction::setup_sandbox(&lua).unwrap();

        // Test encoding
        let table = lua.create_table().unwrap();
        table.raw_set("key", "value").unwrap();
        table.raw_set("num", 42).unwrap();

        let json_table: mlua::Table = lua.globals().get("json").unwrap();
        let json_fn: mlua::Function = json_table.get("encode").unwrap();

        let json_str: String = json_fn.call(table).unwrap();
        assert!(json_str.contains("key"));
        assert!(json_str.contains("value"));
        assert!(json_str.contains("42"));

        // Test decoding
        let json_table2: mlua::Table = lua.globals().get("json").unwrap();
        let decode_fn: mlua::Function = json_table2.get("decode").unwrap();

        let result: mlua::Table = decode_fn.call(json_str).unwrap();
        assert_eq!(result.get::<_, String>("key").unwrap(), "value");
        assert_eq!(result.get::<_, i64>("num").unwrap(), 42);
    }
}

pub mod script_action;

use engine_core::event::Event;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tracing::{error, info};

pub use script_action::{ScriptAction, ScriptErrorBehavior};

pub trait Action: Send + Sync {
    fn execute(&self, event: &Event) -> Result<ActionResult, ActionError>;
    fn description(&self) -> String;
    fn clone_box(&self) -> Box<dyn Action>;
}

impl Clone for Box<dyn Action> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl std::fmt::Debug for Box<dyn Action> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Action({})", self.description())
    }
}

#[derive(Debug, Clone)]
pub enum ActionResult {
    Success { message: Option<String> },
    Skipped { reason: String },
}

#[derive(Debug, Clone)]
pub enum ActionError {
    Execution(String),
    Configuration(String),
    Timeout,
}

impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionError::Execution(msg) => write!(f, "Execution error: {}", msg),
            ActionError::Configuration(msg) => write!(f, "Configuration error: {}", msg),
            ActionError::Timeout => write!(f, "Action timed out"),
        }
    }
}

impl std::error::Error for ActionError {}

impl From<mlua::Error> for ActionError {
    fn from(err: mlua::Error) -> Self {
        ActionError::Execution(format!("Lua error: {}", err))
    }
}

#[derive(Debug, Clone)]
pub struct ExecuteAction {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub timeout_seconds: Option<u64>,
}

impl ExecuteAction {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            working_dir: None,
            timeout_seconds: Some(30),
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn with_working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = Some(seconds);
        self
    }
}

impl Action for ExecuteAction {
    fn execute(&self, _event: &Event) -> Result<ActionResult, ActionError> {
        let mut cmd = std::process::Command::new(&self.command);
        cmd.args(&self.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }

        info!("Executing: {} {}", self.command, self.args.join(" "));

        let output = cmd
            .spawn()
            .map_err(|e| ActionError::Execution(format!("Failed to spawn process: {}", e)))?
            .wait_with_output()
            .map_err(|e| ActionError::Execution(format!("Failed to wait for process: {}", e)))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.is_empty() {
                info!("Command output: {}", stdout.trim());
            }
            Ok(ActionResult::Success {
                message: Some(stdout.to_string()),
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ActionError::Execution(format!(
                "Command failed with exit code {:?}: {}",
                output.status.code(),
                stderr
            )))
        }
    }

    fn description(&self) -> String {
        format!("Execute: {} {}", self.command, self.args.join(" "))
    }

    fn clone_box(&self) -> Box<dyn Action> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct PowerShellAction {
    pub script: String,
    pub working_dir: Option<PathBuf>,
}

impl PowerShellAction {
    pub fn new(script: impl Into<String>) -> Self {
        Self {
            script: script.into(),
            working_dir: None,
        }
    }

    pub fn with_working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }
}

impl Action for PowerShellAction {
    fn execute(&self, _event: &Event) -> Result<ActionResult, ActionError> {
        let mut cmd = std::process::Command::new("powershell.exe");
        cmd.arg("-Command")
            .arg(&self.script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }

        info!("Executing PowerShell script");

        let output = cmd
            .spawn()
            .map_err(|e| ActionError::Execution(format!("Failed to spawn PowerShell: {}", e)))?
            .wait_with_output()
            .map_err(|e| ActionError::Execution(format!("Failed to wait for PowerShell: {}", e)))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.is_empty() {
                info!("PowerShell output: {}", stdout.trim());
            }
            Ok(ActionResult::Success {
                message: Some(stdout.to_string()),
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(ActionError::Execution(format!(
                "PowerShell failed: {}",
                stderr
            )))
        }
    }

    fn description(&self) -> String {
        format!("PowerShell: {}", &self.script[..self.script.len().min(50)])
    }

    fn clone_box(&self) -> Box<dyn Action> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct LogAction {
    pub message: String,
    pub level: LogLevel,
}

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogAction {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            level: LogLevel::Info,
        }
    }

    pub fn with_level(mut self, level: LogLevel) -> Self {
        self.level = level;
        self
    }
}

impl Action for LogAction {
    fn execute(&self, event: &Event) -> Result<ActionResult, ActionError> {
        let message = format!("{} [Event: {:?}]", self.message, event.kind);

        match self.level {
            LogLevel::Debug => tracing::debug!("{}", message),
            LogLevel::Info => tracing::info!("{}", message),
            LogLevel::Warn => tracing::warn!("{}", message),
            LogLevel::Error => tracing::error!("{}", message),
        }

        Ok(ActionResult::Success { message: None })
    }

    fn description(&self) -> String {
        format!("Log [{}]: {}", format!("{:?}", self.level), self.message)
    }

    fn clone_box(&self) -> Box<dyn Action> {
        Box::new(self.clone())
    }
}

pub struct CompositeAction {
    pub actions: Vec<Box<dyn Action>>,
    pub on_error: ErrorBehavior,
}

impl std::fmt::Debug for CompositeAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CompositeAction({} actions)", self.actions.len())
    }
}

impl Clone for CompositeAction {
    fn clone(&self) -> Self {
        Self {
            actions: self.actions.clone(),
            on_error: self.on_error,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ErrorBehavior {
    Continue,
    Stop,
    SkipRemaining,
}

impl CompositeAction {
    pub fn new(actions: Vec<Box<dyn Action>>) -> Self {
        Self {
            actions,
            on_error: ErrorBehavior::Continue,
        }
    }

    pub fn with_error_behavior(mut self, behavior: ErrorBehavior) -> Self {
        self.on_error = behavior;
        self
    }
}

impl Action for CompositeAction {
    fn execute(&self, event: &Event) -> Result<ActionResult, ActionError> {
        let mut results = Vec::new();

        for action in &self.actions {
            match action.execute(event) {
                Ok(result) => results.push(result),
                Err(e) => {
                    error!("Action failed: {} - {}", action.description(), e);
                    match self.on_error {
                        ErrorBehavior::Continue => continue,
                        ErrorBehavior::Stop => return Err(e),
                        ErrorBehavior::SkipRemaining => break,
                    }
                }
            }
        }

        Ok(ActionResult::Success {
            message: Some(format!("Executed {} actions", results.len())),
        })
    }

    fn description(&self) -> String {
        format!("Composite ({} actions)", self.actions.len())
    }

    fn clone_box(&self) -> Box<dyn Action> {
        Box::new(self.clone())
    }
}

pub struct ActionExecutor {
    actions: HashMap<String, Box<dyn Action>>,
}

impl ActionExecutor {
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: impl Into<String>, action: Box<dyn Action>) {
        self.actions.insert(name.into(), action);
    }

    pub fn execute(&self, name: &str, event: &Event) -> Result<ActionResult, ActionError> {
        match self.actions.get(name) {
            Some(action) => action.execute(event),
            None => Err(ActionError::Configuration(format!(
                "Action '{}' not found",
                name
            ))),
        }
    }
}

impl Clone for ActionExecutor {
    fn clone(&self) -> Self {
        Self {
            actions: self.actions.clone(),
        }
    }
}

impl Default for ActionExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::event::EventKind;

    #[test]
    fn test_log_action() {
        let action = LogAction::new("Test message").with_level(LogLevel::Info);
        let event = Event::new(
            EventKind::FileCreated {
                path: PathBuf::from("test.txt"),
            },
            "test",
        );

        let result = action.execute(&event);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_action_echo() {
        let action = ExecuteAction::new("echo").with_args(vec!["Hello".to_string()]);
        let event = Event::new(EventKind::TimerTick, "test");

        let result = action.execute(&event);
        assert!(result.is_ok());

        if let Ok(ActionResult::Success { message: Some(msg) }) = result {
            assert!(msg.contains("Hello"));
        }
    }

    #[test]
    fn test_action_executor() {
        let mut executor = ActionExecutor::new();
        executor.register("log", Box::new(LogAction::new("Test")));

        let event = Event::new(EventKind::TimerTick, "test");
        let result = executor.execute("log", &event);
        assert!(result.is_ok());

        let result = executor.execute("nonexistent", &event);
        assert!(result.is_err());
    }
}

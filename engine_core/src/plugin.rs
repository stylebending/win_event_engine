use async_trait::async_trait;
use crate::event::Event;
use std::fmt;
use tokio::sync::mpsc::Sender;

pub type EventEmitter = Sender<Event>;

#[derive(Debug, Clone)]
pub enum PluginError {
    Initialization(String),
    Runtime(String),
    Configuration(String),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginError::Initialization(msg) => write!(f, "Plugin initialization error: {}", msg),
            PluginError::Runtime(msg) => write!(f, "Plugin runtime error: {}", msg),
            PluginError::Configuration(msg) => write!(f, "Plugin configuration error: {}", msg),
        }
    }
}

impl std::error::Error for PluginError {}

#[async_trait]
pub trait EventSourcePlugin: Send + Sync {
    fn name(&self) -> &str;
    
    async fn start(&mut self, emitter: EventEmitter) -> Result<(), PluginError>;
    
    async fn stop(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
    
    fn is_running(&self) -> bool {
        false
    }
}

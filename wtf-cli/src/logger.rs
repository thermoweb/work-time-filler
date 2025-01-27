use once_cell::sync::OnceCell;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use log::{Log, Metadata, Record};

/// Global debug flag
static DEBUG_MODE: AtomicBool = AtomicBool::new(false);

/// Enable debug logging
pub fn enable_debug() {
    DEBUG_MODE.store(true, Ordering::Relaxed);
}

/// Check if debug mode is enabled
pub fn is_debug_enabled() -> bool {
    DEBUG_MODE.load(Ordering::Relaxed)
}

/// A logger that can output to different targets (stdout or collect for TUI)
pub trait Logger: Send + Sync {
    fn log(&self, message: String);
}

/// Standard output logger for CLI mode
pub struct StdoutLogger;

impl Logger for StdoutLogger {
    fn log(&self, message: String) {
        println!("{}", message);
    }
}

/// Collecting logger for TUI mode - stores messages in memory
pub struct CollectingLogger {
    messages: Arc<Mutex<Vec<String>>>,
}

impl CollectingLogger {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn get_messages(&self) -> Vec<String> {
        self.messages.lock().unwrap().clone()
    }

    #[allow(dead_code)]
    pub fn clear(&self) {
        self.messages.lock().unwrap().clear();
    }
}

impl Logger for CollectingLogger {
    fn log(&self, message: String) {
        let mut messages = self.messages.lock().unwrap();
        messages.push(message);
        // Keep only last 100 messages to avoid memory growth
        if messages.len() > 100 {
            messages.remove(0);
        }
    }
}

/// Global logger instance
static GLOBAL_LOGGER: OnceCell<Arc<dyn Logger>> = OnceCell::new();

/// Initialize the global logger (call once at app start)
pub fn init_logger(logger: Arc<dyn Logger>) {
    GLOBAL_LOGGER.set(logger).ok();
}

/// Log a message using the global logger
pub fn log(message: String) {
    if let Some(logger) = GLOBAL_LOGGER.get() {
        logger.log(message);
    } else {
        // Fallback to stdout if logger not initialized
        println!("{}", message);
    }
}

/// Log an info message
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::logger::log(format!($($arg)*))
    };
}

/// Log a warning message
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::logger::log(format!("⚠️  {}", format!($($arg)*)))
    };
}

/// Log an error message
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::logger::log(format!("❌ {}", format!($($arg)*)))
    };
}

/// Log a success message
#[macro_export]
macro_rules! success {
    ($($arg:tt)*) => {
        $crate::logger::log(format!("✓ {}", format!($($arg)*)))
    };
}

/// Log a debug message (only if debug mode is enabled)
pub fn debug(message: String) {
    if is_debug_enabled() {
        log(format!("DEBUG: {}", message));
    }
}

/// Log a debug message with formatting (only if debug mode is enabled)
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if $crate::logger::is_debug_enabled() {
            $crate::logger::log(format!("DEBUG: {}", format!($($arg)*)))
        }
    };
}

/// Get the default logger (stdout)
pub fn stdout_logger() -> Arc<dyn Logger> {
    Arc::new(StdoutLogger)
}

/// Get a collecting logger for TUI mode
pub fn collecting_logger() -> Arc<CollectingLogger> {
    Arc::new(CollectingLogger::new())
}

/// A bridge that implements log::Log to route log crate messages to our custom logger
struct LogBridge {
    logger: Arc<dyn Logger>,
}

impl Log for LogBridge {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let level_prefix = match record.level() {
            log::Level::Error => "❌",
            log::Level::Warn => "⚠️",
            log::Level::Info => "ℹ️",
            log::Level::Debug => "🔍",
            log::Level::Trace => "🔬",
        };
        
        let message = format!("{} {}", level_prefix, record.args());
        self.logger.log(message);
    }

    fn flush(&self) {}
}

/// Initialize both the custom logger and the log crate backend
pub fn init_logger_with_log_bridge(logger: Arc<dyn Logger>) {
    // Initialize our custom logger
    GLOBAL_LOGGER.set(logger.clone()).ok();
    
    // Initialize the log crate to use our bridge
    let bridge = LogBridge { logger };
    log::set_boxed_logger(Box::new(bridge)).ok();
    
    // Set log level based on debug mode
    let level = if is_debug_enabled() {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    log::set_max_level(level);
}

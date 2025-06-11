//! Error types for the mapping subsystem
//!
//! Defines specific error categories for mapping engine operations, strategy
//! configuration, and inter-thread communication failures.

use thiserror::Error;

/// Error types for mapping engine operations
///
/// Each variant represents a specific failure mode in the mapping pipeline.
/// Errors are designed to provide clear context for debugging and error handling.
#[derive(Debug, Error)]
pub enum MappingError {
    /// Configuration validation failed
    ///
    /// Occurs when a mapping strategy configuration has invalid parameters,
    /// missing required fields, or inconsistent settings.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Strategy initialization failed
    ///
    /// Returned when a mapping strategy's `initialize()` method fails,
    /// typically due to resource allocation or setup problems.
    #[error("Initialization error: {0}")]
    InitializationError(String),

    /// Inter-thread channel communication failed
    ///
    /// Occurs when sending or receiving through mpsc channels fails,
    /// usually due to channel closure or buffer overflow.
    #[error("Channel error: {0}")]
    ChannelError(String),

    /// Background task management failed
    ///
    /// Returned when tokio tasks panic, fail to spawn, or encounter
    /// join handle errors during shutdown.
    #[error("Thread error: {0}")]
    ThreadError(String),

    /// Strategy execution failed
    ///
    /// Occurs when a mapping strategy is in an invalid state or
    /// encounters runtime errors during event processing.
    #[error("Strategy error: {0}")]
    StrategyError(String),
}

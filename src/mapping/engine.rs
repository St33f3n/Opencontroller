//! Mapping engine with statum state machine for strategy execution
//!
//! Implements a 5-state lifecycle for mapping strategies with compile-time state safety.
//! Each engine runs in its own tokio task and processes controller input through a
//! pluggable strategy with optional rate limiting.
//!
//! # State Machine
//!
//! ```text
//! Initializing ──► Configured ──► Active ──► Deactivating ──► Deactivated
//!                     │              │           ▲
//!                     └──────────────┘           │
//!                       (activate/deactivate)    │
//!                                              (shutdown)
//! ```
//!
//! # Architecture
//!
//! ```text
//! ControllerOutput ──► [Strategy] ──► MappedEvent
//!       ▲                  │              │
//!       │             [Rate Limiter]      ▼
//!   Input Channel                    Output Channel
//! ```

use crate::controller::controller_handle::ControllerOutput;
use crate::mapping::{
    strategy::MappingContext, MappedEvent, MappingError, MappingStrategy, MappingType, RateLimiter,
};
use statum::{machine, state};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// States for mapping engine lifecycle using statum
#[state]
#[derive(Debug, Clone)]
pub enum MappingEngineState {
    Initializing, // Setting up engine structure
    Configured,   // Strategy loaded and validated
    Active,       // Processing events in main loop
    Deactivating, // Shutting down gracefully
    Deactivated,  // Fully stopped, ready for cleanup
}

/// Mapping engine with compile-time state safety via statum
///
/// Wraps a strategy trait object and manages its lifecycle through distinct states.
/// Each state has specific allowed operations enforced at compile time.
#[machine]
pub struct MappingEngine<S: MappingEngineState> {
    input_receiver: mpsc::Receiver<ControllerOutput>,
    output_sender: mpsc::Sender<MappedEvent>,
    engine_type: MappingType,
    name: String,
    strategy: Option<Box<dyn MappingStrategy>>,
    rate_limiter: Option<RateLimiter>,
    context: MappingContext,
}
impl<S: MappingEngineState> MappingEngine<S> {
    pub fn get_type(&self) -> MappingType {
        self.engine_type
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }
}

impl MappingEngine<Initializing> {
    pub fn create(
        input_receiver: mpsc::Receiver<ControllerOutput>,
        output_sender: mpsc::Sender<MappedEvent>,
        engine_type: MappingType,
        name: String,
    ) -> Self {
        info!("Initializing new mapping engine: {}", name);

        Self::new(
            input_receiver,
            output_sender,
            engine_type,
            name,
            None,                      // strategy
            None,                      // rate_limiter
            MappingContext::default(), // context
        )
    }

    /// Configures engine with strategy and transitions to Configured state
    ///
    /// Initializes the strategy, sets up rate limiting if requested by strategy,
    /// and transitions to Configured state on success.
    pub fn configure(
        mut self,
        mut strategy: Box<dyn MappingStrategy>,
    ) -> Result<MappingEngine<Configured>, MappingError> {
        info!("Configuring mapping engine: {}", self.name);

        match strategy.initialize() {
            Ok(_) => {
                debug!("Strategy initialized successfully");

                let rate_limiter = strategy.get_rate_limit().map(RateLimiter::new);
                if let Some(ref limiter) = rate_limiter {
                    debug!(
                        "Rate limiter configured with {}ms interval",
                        limiter.min_interval_ms
                    );
                }

                self.strategy = Some(strategy);
                self.rate_limiter = rate_limiter;

                info!("Engine configured successfully: {}", self.name);
                Ok(self.transition())
            }
            Err(e) => {
                error!("Failed to initialize strategy: {}", e);
                Err(MappingError::InitializationError(format!(
                    "Failed to initialize strategy: {}",
                    e
                )))
            }
        }
    }
}

impl MappingEngine<Configured> {
    pub fn activate(self) -> MappingEngine<Active> {
        info!("Activating mapping engine: {}", self.name);
        self.transition()
    }
}

impl MappingEngine<Active> {
    /// Processes a single controller event through the strategy
    ///
    /// Applies rate limiting if configured, then calls the strategy's map method.
    /// Returns None if no input available, rate limited, or strategy produces no output.
    pub fn process_event(&mut self) -> Result<Option<MappedEvent>, MappingError> {
        let strategy = match &mut self.strategy {
            Some(s) => s,
            None => {
                return Err(MappingError::StrategyError(
                    "No strategy available".to_string(),
                ))
            }
        };

        let controller_state = self.input_receiver.try_recv();

        if let Ok(controller_output) = controller_state {
            if let Some(limiter) = &mut self.rate_limiter {
                if !limiter.should_process() {
                    return Ok(None);
                }
            }

            match strategy.map(&controller_output) {
                Some(mapped_event) => {
                    info!("Successfully mapped event to {:?}", mapped_event);
                    return Ok(Some(mapped_event));
                }
                None => {
                    debug!("No event mapped for this input");
                    return Ok(None);
                }
            }
        }
        Ok(None)
    }

    /// Sends mapped event to output channel
    pub async fn send_event(&self, event: MappedEvent) -> Result<(), MappingError> {
        match self.output_sender.try_send(event) {
            Ok(_) => {
                info!("Event sent successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to send mapped event: {}", e);
                Err(MappingError::ChannelError(format!(
                    "Failed to send mapped event: {}",
                    e
                )))
            }
        }
    }

    /// Main processing loop with graceful shutdown support
    ///
    /// Runs until shutdown signal received. Processes events every 10ms with
    /// error recovery - individual event processing errors don't stop the loop.
    pub async fn run_until_shutdown(
        mut self,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) -> Result<MappingEngine<Deactivating>, MappingError> {
        info!("Starting event processing loop for: {}", self.name);

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    info!("Shutdown signal received for: {}", self.name);
                    break;
                }

                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    match self.process_event() {
                        Ok(Some(event)) => {
                            if let Err(e) = self.send_event(event).await {
                                warn!("Failed to send event: {}", e);
                            }
                        }
                        Ok(None) => {
                        }
                        Err(e) => {
                            error!("Error processing event: {}", e);
                        }
                    }
                }
            }
        }

        info!("Transitioning to Deactivating state: {}", self.name);
        Ok(self.transition())
    }

    pub fn deactivate(self) -> MappingEngine<Deactivating> {
        info!("Deactivating mapping engine: {}", self.name);
        self.transition()
    }
}

impl MappingEngine<Deactivating> {
    /// Shuts down strategy and transitions to Deactivated state
    pub async fn shutdown(mut self) -> MappingEngine<Deactivated> {
        info!("Shutting down mapping engine: {}", self.name);

        if let Some(strategy) = &mut self.strategy {
            debug!("Shutting down strategy");
            strategy.shutdown();
        }

        // In den Deactivated-Zustand wechseln
        info!("Engine shut down successfully: {}", self.name);
        self.transition()
    }
}

// Implementierung für den deaktivierten Zustand
impl MappingEngine<Deactivated> {}
/// Handle for managing mapping engine in a tokio task
///
/// Provides lifecycle management for engines running in background tasks.
/// Handles task spawning, graceful shutdown, and resource cleanup.
#[derive(Debug)]
pub struct MappingEngineHandle {
    pub engine_type: MappingType,

    pub name: String,

    task_handle: Option<JoinHandle<Result<(), MappingError>>>,

    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl MappingEngineHandle {
    pub fn new(engine_type: MappingType, name: String) -> Self {
        Self {
            engine_type,
            name,
            task_handle: None,
            shutdown_tx: None,
        }
    }
    /// Starts engine in tokio task and returns communication channels
    ///
    /// Creates engine, configures it with strategy, activates it, and spawns
    /// the main processing loop in a background task.
    ///
    /// # Returns
    ///
    /// * Output receiver for mapped events
    /// * Input sender for controller data
    pub fn start(
        &mut self,
        strategy: Box<dyn MappingStrategy>,
    ) -> Result<(mpsc::Receiver<MappedEvent>, mpsc::Sender<ControllerOutput>), MappingError> {
        let (controller_state_sender, controller_state_receiver) = mpsc::channel(100);
        let (mapped_event_sender, mapped_event_receiver) = mpsc::channel(100);
        let engine_name = self.name.clone();
        let engine = MappingEngine::create(
            controller_state_receiver,
            mapped_event_sender,
            self.engine_type,
            engine_name.clone(),
        )
        .configure(strategy)?;

        let active_engine = engine.activate();

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);
        let task_handle = tokio::spawn(async move {
            info!("Spawning running engine: {}", engine_name);
            match active_engine.run_until_shutdown(shutdown_rx).await {
                Ok(deactivating_engine) => {
                    info!("Engine entering deactivating state: {}", engine_name);
                    let _ = deactivating_engine.shutdown().await;
                    Ok(())
                }
                Err(e) => {
                    error!("Error running engine: {} - {}", engine_name, e);
                    Err(e)
                }
            }
        });

        self.task_handle = Some(task_handle);

        info!(
            "Mapping engine activated: {} ({})",
            self.name, self.engine_type
        );
        Ok((mapped_event_receiver, controller_state_sender))
    }

    /// Gracefully shuts down engine and waits for task completion
    pub async fn shutdown(&mut self) -> Result<(), MappingError> {
        debug!("Sending shutdown signal to engine: {}", self.name);

        //Send shutdown signal
        if let Some(tx) = self.shutdown_tx.take() {
            if tx.send(()).is_err() {
                warn!("Engine task already terminated: {}", self.name);
            }
        }

        // Wait for task completion
        if let Some(handle) = self.task_handle.take() {
            match handle.await {
                Ok(result) => {
                    debug!("Engine task completed: {}", self.name);
                    result
                }
                Err(e) => {
                    error!("Engine task panicked: {} - {}", self.name, e);
                    Err(MappingError::ThreadError(format!(
                        "Engine task panicked: {}",
                        e
                    )))
                }
            }
        } else {
            debug!("Engine already shut down: {}", self.name);
            Ok(())
        }
    }
}

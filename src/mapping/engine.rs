//! Implementierung der Mapping-Engine mittels Statum State Machine.

use crate::controller::controller::ControllerOutput;
use crate::mapping::{
    strategy::MappingContext, MappedEvent, MappingError, MappingStrategy, MappingType, RateLimiter,
};
use statum::{machine, state};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// State-Definition für die Mapping-Engine mit Statum
#[state]
#[derive(Debug, Clone)]
pub enum MappingEngineState {
    /// Initialisierungszustand
    Initializing,

    /// Konfigurierter, aber inaktiver Zustand
    Configured,

    /// Aktiver Zustand, verarbeitet Events
    Active,

    /// Deaktivierungszustand, fährt sauber herunter
    Deactivating,

    /// Deaktivierter Zustand, bereit zur Entsorgung
    Deactivated,
}

/// State Machine für die Mapping-Engine mittels Statum
#[machine]
pub struct MappingEngine<S: MappingEngineState> {
    /// Empfänger für Controller-Events
    input_receiver: watch::Receiver<ControllerOutput>,

    /// Sender für gemappte Events
    output_sender: mpsc::Sender<MappedEvent>,

    /// Typ der Mapping-Strategie
    engine_type: MappingType,

    /// Name der Engine-Instanz
    name: String,

    /// Die aktuelle Mapping-Strategie
    strategy: Option<Box<dyn MappingStrategy>>,

    /// Rate-Limiter für Event-Verarbeitung
    rate_limiter: Option<RateLimiter>,

    /// Kontext für Zustandserhaltung zwischen Mapping-Aufrufen
    context: MappingContext,
}

// Implementierung für alle Zustände
impl<S: MappingEngineState> MappingEngine<S> {
    /// Gibt den Typ der Mapping-Engine zurück
    pub fn get_type(&self) -> MappingType {
        self.engine_type
    }

    /// Gibt den Namen der Mapping-Engine zurück
    pub fn get_name(&self) -> &str {
        &self.name
    }
}

// Implementierung für den Initialisierungszustand
impl MappingEngine<Initializing> {
    /// Erstellt eine neue Mapping-Engine im Initialisierungszustand
    /// Verwendet die von Statum generierte new() Methode zur korrekten Initialisierung
    pub fn create(
        input_receiver: watch::Receiver<ControllerOutput>,
        output_sender: mpsc::Sender<MappedEvent>,
        engine_type: MappingType,
        name: String,
    ) -> Self {
        info!("Initializing new mapping engine: {}", name);

        // Hier verwenden wir die von Statum generierte new() Methode,
        // die die zusätzlichen Felder (marker, state_data) korrekt initialisiert
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

    /// Konfiguriert die Engine mit einer Strategie und wechselt in den Configured-Zustand
    pub fn configure(
        mut self,
        mut strategy: Box<dyn MappingStrategy>,
    ) -> Result<MappingEngine<Configured>, MappingError> {
        info!("Configuring mapping engine: {}", self.name);

        // Strategie initialisieren
        match strategy.initialize() {
            Ok(_) => {
                debug!("Strategy initialized successfully");

                // Rate-Limiter erstellen, wenn von der Strategie gefordert
                let rate_limiter = strategy.get_rate_limit().map(RateLimiter::new);
                if let Some(ref limiter) = rate_limiter {
                    debug!(
                        "Rate limiter configured with {}ms interval",
                        limiter.min_interval_ms
                    );
                }

                // Strategie und Rate-Limiter speichern
                self.strategy = Some(strategy);
                self.rate_limiter = rate_limiter;

                // In den Configured-Zustand wechseln
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

// Implementierung für den konfigurierten Zustand
impl MappingEngine<Configured> {
    /// Aktiviert die Engine und wechselt in den Active-Zustand
    pub fn activate(self) -> MappingEngine<Active> {
        info!("Activating mapping engine: {}", self.name);
        self.transition()
    }
}

// Implementierung für den aktiven Zustand
impl MappingEngine<Active> {
    /// Verarbeitet ein einzelnes Controller-Event
    pub fn process_event(&mut self) -> Result<Option<MappedEvent>, MappingError> {
        // Strategie muss vorhanden sein
        let strategy = match &mut self.strategy {
            Some(s) => s,
            None => {
                return Err(MappingError::StrategyError(
                    "No strategy available".to_string(),
                ))
            }
        };

        // Controller-Zustand lesen
        let controller_state = self.input_receiver.borrow().clone();

        // Rate-Limiting prüfen, wenn konfiguriert
        if let Some(limiter) = &mut self.rate_limiter {
            if !limiter.should_process() {
                // Event aufgrund von Rate-Limiting überspringen
                return Ok(None);
            }
        }

        // Mapping durchführen
        match strategy.map(&controller_state) {
            Some(mapped_event) => {
                debug!("Successfully mapped event to {:?}", mapped_event);
                Ok(Some(mapped_event))
            }
            None => {
                debug!("No event mapped for this input");
                Ok(None)
            }
        }
    }

    /// Sendet ein gemapptes Event über den Ausgabekanal
    pub async fn send_event(&self, event: MappedEvent) -> Result<(), MappingError> {
        match self.output_sender.send(event).await {
            Ok(_) => {
                debug!("Event sent successfully");
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

    /// Hauptverarbeitungsschleife für die aktive Engine
    pub async fn run_until_shutdown(
        mut self,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) -> Result<MappingEngine<Deactivating>, MappingError> {
        info!("Starting event processing loop for: {}", self.name);

        loop {
            tokio::select! {
                // Shutdown-Signal prüfen
                _ = &mut shutdown_rx => {
                    info!("Shutdown signal received for: {}", self.name);
                    break;
                }

                // Kurze Pause, um CPU-Last zu reduzieren
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    // Event verarbeiten
                    match self.process_event() {
                        Ok(Some(event)) => {
                            // Event senden
                            if let Err(e) = self.send_event(event).await {
                                warn!("Failed to send event: {}", e);
                                // Weitermachen trotz Fehler
                            }
                        }
                        Ok(None) => {
                            // Kein Event zu senden, weitermachen
                        }
                        Err(e) => {
                            error!("Error processing event: {}", e);
                            // Weitermachen trotz Fehler
                        }
                    }
                }
            }
        }

        // In den Deactivating-Zustand wechseln
        info!("Transitioning to Deactivating state: {}", self.name);
        Ok(self.transition())
    }

    /// Deaktiviert die Engine und wechselt in den Deactivating-Zustand
    pub fn deactivate(self) -> MappingEngine<Deactivating> {
        info!("Deactivating mapping engine: {}", self.name);
        self.transition()
    }
}

// Implementierung für den Deaktivierungszustand
impl MappingEngine<Deactivating> {
    /// Fährt die Engine sauber herunter und wechselt in den Deactivated-Zustand
    pub async fn shutdown(mut self) -> MappingEngine<Deactivated> {
        info!("Shutting down mapping engine: {}", self.name);

        // Strategie herunterfahren, falls vorhanden
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
impl MappingEngine<Deactivated> {
    // Im deaktivierten Zustand gibt es keine speziellen Methoden
}

/// Handle für eine laufende Mapping-Engine
#[derive(Debug)]
pub struct MappingEngineHandle {
    /// Typ der Mapping-Engine
    pub engine_type: MappingType,

    /// Name der Mapping-Engine
    pub name: String,

    /// Join-Handle für den Tokio-Task
    task_handle: Option<JoinHandle<Result<(), MappingError>>>,

    /// Sender für das Shutdown-Signal
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl MappingEngineHandle {
    /// Erstellt ein neues Handle für eine Mapping-Engine
    pub fn new(
        engine_type: MappingType,
        name: String,
        task_handle: JoinHandle<Result<(), MappingError>>,
        shutdown_tx: oneshot::Sender<()>,
    ) -> Self {
        Self {
            engine_type,
            name,
            task_handle: Some(task_handle), // In Some wrappen
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Fährt die Engine herunter und wartet auf Beendigung
    pub async fn shutdown(&mut self) -> Result<(), MappingError> {
        debug!("Sending shutdown signal to engine: {}", self.name);

        // Shutdown-Signal senden
        if let Some(tx) = self.shutdown_tx.take() {
            if tx.send(()).is_err() {
                warn!("Engine task already terminated: {}", self.name);
            }
        }

        // Auf Beendigung des Tasks warten - take() nimmt ownership des JoinHandles
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
            // Task-Handle wurde bereits genommen, Engine ist bereits heruntergefahren
            debug!("Engine already shut down: {}", self.name);
            Ok(())
        }
    }
}

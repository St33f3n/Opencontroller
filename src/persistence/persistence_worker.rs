use crate::mqtt::mqtt_handler::Configured;

use super::{
    config_portal::{ConfigPortal, ConfigResult, PortalAction},
    session_client::SessionClient,
};
use color_eyre::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

macro_rules! handle_action {
    ($action:expr, $response_tx:expr) => {
        if let Err(_) = $response_tx.send($action.await) {
            error!("Failed to send response");
        }
    };
}

pub struct PersistenceManager {
    tx: Sender<SessionAction>,
    worker_handle: tokio::task::JoinHandle<()>,
    autosave_handle: tokio::task::JoinHandle<()>,
    session_client: Arc<Mutex<SessionClient>>,
}

impl PersistenceManager {
    pub async fn new() -> Self {
        let session_client = Arc::new(Mutex::new(SessionClient::load_last_session().await));
        let session_cpy = session_client.clone();
        let (tx, mut rx) = channel::<SessionAction>(32);
        let handle = tokio::spawn(async move {
            while let Some(action) = rx.recv().await {
                match action {
                    SessionAction::CreateSession { name, response_tx } => {
                        handle_action!(session_client.lock().await.save_session(name), response_tx);
                    }
                    SessionAction::LoadSession { name, response_tx } => {
                        handle_action!(
                            session_client.lock().await.change_session(&name),
                            response_tx
                        );
                    }
                    SessionAction::SaveCurrentSession { response_tx } => {
                        handle_action!(
                            session_client.lock().await.save_current_session(),
                            response_tx
                        );
                    }
                    SessionAction::DeleteSession { name, response_tx } => {
                        handle_action!(
                            session_client.lock().await.delete_session(&name),
                            response_tx
                        );
                    }
                    SessionAction::ListSessions { response_tx } => {
                        handle_action!(SessionClient::scan_available_sessions(), response_tx);
                    }
                }
            }
        });

        let autosave = SessionClient::start_autosave_task(session_cpy.clone(), 60).await;

        Self {
            tx,
            autosave_handle: autosave,
            worker_handle: handle,
            session_client: session_cpy.clone(),
        }
    }

    pub fn get_sender(&self) -> Sender<SessionAction> {
        self.tx.clone()
    }

    pub async fn get_cfg_portal(&self) -> Arc<ConfigPortal> {
        self.session_client.lock().await.get_portal_ref()
    }
}

// Aktion-Enum für den Config-Worker
#[derive(Debug)]
pub enum SessionAction {
    CreateSession {
        name: String,
        response_tx: tokio::sync::oneshot::Sender<Result<()>>,
    },
    LoadSession {
        name: String,
        response_tx: tokio::sync::oneshot::Sender<Result<()>>,
    },
    SaveCurrentSession {
        response_tx: tokio::sync::oneshot::Sender<Result<()>>,
    },
    DeleteSession {
        name: String,
        response_tx: tokio::sync::oneshot::Sender<Result<()>>,
    },
    ListSessions {
        response_tx: tokio::sync::oneshot::Sender<Result<HashMap<String, PathBuf>>>,
    },
}

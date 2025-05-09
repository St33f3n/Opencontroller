// pub fn create_config_worker(
//     config_portal: Arc<Self>,
// ) -> (ConfigClient, tokio::task::JoinHandle<()>) {
//     let (tx, mut rx) = mpsc::channel::<ConfigAction>(32);
//     let client = ConfigClient::new(tx.clone());
//     // Spawn worker task
//     let handle = tokio::spawn(async move {
//         while let Some(action) = rx.recv().await {
//             match action {
//                 ConfigAction::CreateSession { name, response_tx } => {
//                     let result = config_portal.create_session(name).await;
//                     if let Err(_) = response_tx.send(result) {
//                         error!("Failed to send create session response");
//                     }
//                 }
//                 ConfigAction::LoadSession { name, response_tx } => {
//                     let result = config_portal.load_session(&name).await;
//                     if let Err(_) = response_tx.send(result) {
//                         error!("Failed to send load session response");
//                     }
//                 }
//                 ConfigAction::SaveCurrentSession { response_tx } => {
//                     let result = config_portal.save_current_session().await;
//                     if let Err(_) = response_tx.send(result) {
//                         error!("Failed to send save session response");
//                     }
//                 }
//                 ConfigAction::DeleteSession { name, response_tx } => {
//                     let result = config_portal.delete_session(&name).await;
//                     if let Err(_) = response_tx.send(result) {
//                         error!("Failed to send delete session response");
//                     }
//                 }
//                 ConfigAction::ListSessions { response_tx } => {
//                     let result = Self::list_available_sessions().await;
//                     if let Err(_) = response_tx.send(result) {
//                         error!("Failed to send list sessions response");
//                     }
//                 }
//                 ConfigAction::SaveMessage {
//                     message,
//                     response_tx,
//                 } => {
//                     let result = config_portal.save_message(message).await;
//                     if let Err(_) = response_tx.send(result) {
//                         error!("Failed to send save message response");
//                     }
//                 }
//             }
//         }
//     });

//     (client, handle)
// }
//}

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
        response_tx: tokio::sync::oneshot::Sender<Result<Vec<String>>>,
    },
    SaveMessage {
        message: MQTTMessage,
        response_tx: tokio::sync::oneshot::Sender<Result<()>>,
    },
}

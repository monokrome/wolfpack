use anyhow::Result;
use rand::Rng;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

/// How long a pairing code remains valid
const CODE_EXPIRY: Duration = Duration::from_secs(300); // 5 minutes

/// A 6-digit pairing code
pub type PairingCode = String;

/// Request to join a pairing session
#[derive(Debug, Clone)]
pub struct PairingRequest {
    pub device_id: String,
    pub device_name: String,
    pub public_key: String,
}

/// Response from the initiator
#[derive(Debug, Clone)]
pub struct PairingResponse {
    pub device_id: String,
    pub device_name: String,
    pub public_key: String,
}

/// Result of a pairing attempt
#[derive(Debug, Clone)]
pub enum PairingResult {
    Accepted(PairingResponse),
    Rejected,
    Expired,
    InvalidCode,
}

/// A pending pairing session (initiator side)
struct PendingSession {
    code: PairingCode,
    created_at: Instant,
}

/// Commands for the pairing manager
pub enum PairingCommand {
    /// Initiator: Create a new pairing session, returns the code
    CreateSession {
        response_tx: oneshot::Sender<PairingCode>,
    },
    /// Joiner: Attempt to join with a code
    JoinSession {
        code: PairingCode,
        request: PairingRequest,
        response_tx: oneshot::Sender<PairingResult>,
    },
    /// Initiator: Get pending request for confirmation
    GetPendingRequest {
        response_tx: oneshot::Sender<Option<PairingRequest>>,
    },
    /// Initiator: Respond to a pairing request
    RespondToRequest {
        accepted: bool,
        response: Option<PairingResponse>,
    },
    /// Cancel current session
    CancelSession,
}

/// Manages pairing sessions
pub struct PairingManager {
    command_tx: mpsc::Sender<PairingCommand>,
}

impl PairingManager {
    /// Start the pairing manager
    pub fn new() -> (Self, mpsc::Receiver<PairingCommand>) {
        let (command_tx, command_rx) = mpsc::channel(16);
        (Self { command_tx }, command_rx)
    }

    /// Create a new pairing session (initiator)
    pub async fn create_session(&self) -> Result<PairingCode> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(PairingCommand::CreateSession { response_tx })
            .await?;
        Ok(response_rx.await?)
    }

    /// Join a pairing session (joiner)
    pub async fn join_session(
        &self,
        code: PairingCode,
        request: PairingRequest,
    ) -> Result<PairingResult> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(PairingCommand::JoinSession {
                code,
                request,
                response_tx,
            })
            .await?;
        Ok(response_rx.await?)
    }

    /// Get pending request (initiator checking for incoming requests)
    pub async fn get_pending_request(&self) -> Result<Option<PairingRequest>> {
        let (response_tx, response_rx) = oneshot::channel();
        self.command_tx
            .send(PairingCommand::GetPendingRequest { response_tx })
            .await?;
        Ok(response_rx.await?)
    }

    /// Respond to a pairing request (initiator)
    pub async fn respond(&self, accepted: bool, response: Option<PairingResponse>) -> Result<()> {
        self.command_tx
            .send(PairingCommand::RespondToRequest { accepted, response })
            .await?;
        Ok(())
    }

    /// Cancel current session
    pub async fn cancel(&self) -> Result<()> {
        self.command_tx.send(PairingCommand::CancelSession).await?;
        Ok(())
    }
}

/// State machine for pairing sessions
pub struct PairingState {
    /// Current session (only one at a time)
    current_session: Option<PendingSession>,
    /// Pending joiner waiting for response
    pending_joiner: Option<oneshot::Sender<PairingResult>>,
    /// Pending request waiting for user confirmation
    pending_request: Option<PairingRequest>,
}

impl PairingState {
    pub fn new() -> Self {
        Self {
            current_session: None,
            pending_joiner: None,
            pending_request: None,
        }
    }

    /// Process a pairing command
    #[allow(clippy::too_many_lines)] // Command handler with multiple match arms
    pub fn handle_command(&mut self, cmd: PairingCommand) {
        match cmd {
            PairingCommand::CreateSession { response_tx } => {
                // Clean up expired session
                if let Some(session) = &self.current_session
                    && session.created_at.elapsed() > CODE_EXPIRY
                {
                    self.current_session = None;
                }

                // Generate new session
                let code = generate_pairing_code();
                self.current_session = Some(PendingSession {
                    code: code.clone(),
                    created_at: Instant::now(),
                });

                let _ = response_tx.send(code);
            }

            PairingCommand::JoinSession {
                code,
                request,
                response_tx,
            } => {
                // Check if we have a valid session with this code
                let valid = self
                    .current_session
                    .as_ref()
                    .map(|s| s.code == code && s.created_at.elapsed() <= CODE_EXPIRY)
                    .unwrap_or(false);

                if !valid {
                    let result = if self.current_session.is_none() {
                        PairingResult::InvalidCode
                    } else {
                        PairingResult::Expired
                    };
                    let _ = response_tx.send(result);
                    return;
                }

                // Store the joiner's channel and request for later response
                self.pending_joiner = Some(response_tx);
                self.pending_request = Some(request);
            }

            PairingCommand::GetPendingRequest { response_tx } => {
                let _ = response_tx.send(self.pending_request.clone());
            }

            PairingCommand::RespondToRequest { accepted, response } => {
                if let Some(joiner_tx) = self.pending_joiner.take() {
                    let result = if accepted {
                        if let Some(resp) = response {
                            PairingResult::Accepted(resp)
                        } else {
                            PairingResult::Rejected
                        }
                    } else {
                        PairingResult::Rejected
                    };
                    let _ = joiner_tx.send(result);
                }
                self.pending_request = None;
                self.current_session = None;
            }

            PairingCommand::CancelSession => {
                if let Some(joiner_tx) = self.pending_joiner.take() {
                    let _ = joiner_tx.send(PairingResult::Rejected);
                }
                self.pending_request = None;
                self.current_session = None;
            }
        }
    }

    /// Check if there's an active session
    pub fn has_active_session(&self) -> bool {
        self.current_session
            .as_ref()
            .map(|s| s.created_at.elapsed() <= CODE_EXPIRY)
            .unwrap_or(false)
    }

    /// Get current code if session is active
    pub fn current_code(&self) -> Option<&str> {
        self.current_session
            .as_ref()
            .filter(|s| s.created_at.elapsed() <= CODE_EXPIRY)
            .map(|s| s.code.as_str())
    }
}

impl Default for PairingState {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a 6-digit pairing code
fn generate_pairing_code() -> PairingCode {
    let mut rng = rand::thread_rng();
    let code: u32 = rng.gen_range(100000..1000000);
    code.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pairing_code_format() {
        let code = generate_pairing_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_pairing_state_create_session() {
        let mut state = PairingState::new();

        let (tx, rx) = oneshot::channel();
        state.handle_command(PairingCommand::CreateSession { response_tx: tx });

        let code = rx.blocking_recv().unwrap();
        assert_eq!(code.len(), 6);
        assert!(state.has_active_session());
    }
}

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use pumpkin_util::text::TextComponent;
use tokio::sync::oneshot;
use uuid::Uuid;

/// How long a deferred login may wait before it is refused.
pub const LOGIN_DEFER_TIMEOUT: Duration = Duration::from_secs(30);

/// Lets plugins hold a joining player at the end of login while they do async work, such as
/// loading data or taking a network-wide session lock.
///
/// A plugin calls [`defer`](Self::defer) while handling `AsyncPlayerPreLoginEvent` and later
/// [`resume`](Self::resume)s the token. The login continues once every deferral for the player
/// has resumed, and is refused if any of them passes a kick message or the timeout runs out.
#[derive(Default)]
pub struct LoginGate {
    next_token: AtomicU32,
    senders: Mutex<HashMap<u32, oneshot::Sender<Option<TextComponent>>>>,
    waiting: Mutex<HashMap<Uuid, Vec<oneshot::Receiver<Option<TextComponent>>>>>,
}

impl LoginGate {
    /// Adds a deferral for `player` and returns the token that resumes it.
    pub fn defer(&self, player: Uuid) -> u32 {
        let token = self
            .next_token
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let (sender, receiver) = oneshot::channel();
        self.senders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(token, sender);
        self.waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(player)
            .or_default()
            .push(receiver);
        token
    }

    /// Resumes a deferral; a `kick_message` refuses the login. Returns false for an unknown token.
    pub fn resume(&self, token: u32, kick_message: Option<TextComponent>) -> bool {
        let sender = self
            .senders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&token);
        sender.is_some_and(|sender| sender.send(kick_message).is_ok())
    }

    /// Waits for every deferral of `player`. Returns the kick message when the login is refused.
    pub async fn wait(&self, player: Uuid) -> Option<TextComponent> {
        let receivers = self
            .waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&player)
            .unwrap_or_default();
        for receiver in receivers {
            match tokio::time::timeout(LOGIN_DEFER_TIMEOUT, receiver).await {
                Ok(Ok(None)) => {}
                Ok(Ok(Some(kick_message))) => return Some(kick_message),
                // The plugin dropped the token (unloaded) or never resumed it.
                Ok(Err(_)) | Err(_) => return Some(TextComponent::text("Login timed out")),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn waits_for_every_deferral_and_reports_a_refusal() {
        let gate = LoginGate::default();
        let player = Uuid::new_v4();
        let first = gate.defer(player);
        let second = gate.defer(player);
        assert!(gate.resume(first, None));
        assert!(gate.resume(second, Some(TextComponent::text("no session"))));
        assert!(gate.wait(player).await.is_some());
        assert!(!gate.resume(first, None));
    }

    #[tokio::test]
    async fn a_player_without_deferrals_is_not_held() {
        let gate = LoginGate::default();
        assert!(gate.wait(Uuid::new_v4()).await.is_none());
    }
}

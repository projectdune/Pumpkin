#[allow(clippy::wildcard_imports)]
use super::*;

impl JavaClient {
    pub fn handle_confirm_teleport(&self, player: &Player, confirm_teleport: &SConfirmTeleport) {
        enum TeleportResult {
            Success,
            WrongId,
            NotTeleporting,
        }

        let result = {
            let mut awaiting_teleport = player
                .awaiting_teleport
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some((id, position)) = awaiting_teleport.as_ref() {
                if id == &confirm_teleport.teleport_id {
                    // We should set the position now to what we requested in the teleport packet.
                    // This may fix issues when the client sends the position while being teleported.
                    player.get_entity().set_pos(*position);
                    *awaiting_teleport = None;
                    TeleportResult::Success
                } else {
                    TeleportResult::WrongId
                }
            } else {
                TeleportResult::NotTeleporting
            }
        };

        // Like vanilla (`handleAcceptTeleportPacket`), only the confirmation of the teleport we're
        // waiting for counts; any other is ignored. A client confirms each teleport in order, so
        // when two are sent close together (a spawn teleport, then a plugin's) the first one's
        // confirmation arrives after the second was sent. Kicking for it dropped players behind a
        // protocol translator (ViaVersion), which delays confirmations.
        match result {
            TeleportResult::Success => {}
            TeleportResult::WrongId | TeleportResult::NotTeleporting => {
                debug!(
                    "Ignoring teleport confirmation {:?} from {}",
                    confirm_teleport.teleport_id,
                    player.gameprofile.name
                );
            }
        }
    }
}

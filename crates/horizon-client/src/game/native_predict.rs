//! What a client mod may show ahead of the server (API @API:incari_fast_crystal@), as vanilla's
//! client predicts a broken block: nothing is sent, and the server's answer settles it. An end
//! crystal the player hit taken off this client, and put back if the server keeps it; a block
//! predicted as air under a use's sequence, which the server's acknowledgement restores.
//! Client-side, on a server.

use super::Game;
use horizon_world::registry::BlockStateId;

impl Game<'_> {
    /// Takes the end crystal `entity` off this client, as the server's removal would, keeping
    /// it for `native_return_crystal`. Kept until then, until the server removes it, or until
    /// the level changes. False if it isn't a server's crystal on this client, or it can't be
    /// broken (`setPermanentlyInvulnerable`), or a spectator hit it (the server only follows it).
    pub(super) fn native_take_crystal(&mut self, entity: u32) -> bool {
        if self.remote.is_none() || entity == 0 || self.spectator() {
            return false;
        }
        let Some(crystal) = self
            .crystals
            .iter()
            .find(|c| c.id == entity && !c.invulnerable)
            .cloned()
        else {
            return false;
        };
        // As remove_entity: the dragon's beam index follows the crystals.
        self.remove_remote_dragon(entity as i32);
        self.crystals.retain(|c| c.id != entity);
        if let Some(r) = self.remote.as_mut() {
            r.taken_crystals.push(crystal);
        }
        true
    }

    /// Puts back the crystal `entity` taken with `native_take_crystal`, for a server that kept
    /// it. False if the server has removed it since, the level changed, or it's back already.
    pub(super) fn native_return_crystal(&mut self, entity: u32) -> bool {
        let Some(r) = self.remote.as_mut() else {
            return false;
        };
        let Some(i) = r.taken_crystals.iter().position(|c| c.id == entity) else {
            return false;
        };
        let crystal = r.taken_crystals.swap_remove(i);
        if self.crystals.iter().any(|c| c.id == entity) {
            return false;
        }
        self.crystals.push(crystal);
        true
    }

    /// The player's ping in milliseconds as the server's player list gives it, for how long a
    /// prediction waits for the server; None before the server has said (it updates every 30
    /// seconds) or off a server.
    pub(super) fn native_latency(&self) -> Option<i32> {
        self.own_latency()
    }

    /// Whether the player's attack does damage (`Player.attack`'s gate: the attack damage
    /// attribute, with the held weapon's and the effects' modifiers, above 0), so the server
    /// breaks what it hits. Weakness with a bare hand does none.
    pub(super) fn native_attack_hurts(&mut self) -> bool {
        self.attack_damage() > 0.0
    }

    /// Shows the block at `pos` as air ahead of the server, for the use it acknowledges as
    /// `sequence` (`native_block_used`'s): until then the server's updates there are only
    /// remembered, and then the block takes the server's state, as for a predicted break.
    /// False off a server.
    pub(super) fn native_predict_air(&mut self, pos: [i32; 3], sequence: i32) -> bool {
        if self.remote.is_none() {
            return false;
        }
        let at = horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]);
        self.predict_blocks(&vec![(at, BlockStateId(0))], sequence);
        true
    }
}

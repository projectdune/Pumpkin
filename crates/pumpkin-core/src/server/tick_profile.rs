//! Per-phase tick timings for load tests, on with `PUMPKIN_TICK_PROFILE=1`.
//!
//! Server phases are wall time on the tick thread; world phases are summed over every world
//! (worlds tick in parallel, so their sum is CPU time and can exceed the tick). Every 100 ticks the
//! averages per tick are logged as one `TICKPROF` line and reset.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tracing::info;

#[derive(Clone, Copy)]
pub enum Phase {
    /// `ServerTickStartEvent` + `ServerTickEndEvent` (plugins).
    TickEvents,
    /// `Server::tick` as a whole.
    Tick,
    Datapack,
    PluginLoaders,
    Scheduled,
    /// The parallel `World::tick` over every world (wall).
    Worlds,
    PlayerData,
    FlushBlockUpdates,
    // Per world, summed over worlds.
    WorldFlush,
    WorldEnvironment,
    WorldChunks,
    WorldPlayers,
    WorldEntities,
    WorldBlockEntities,
    WorldRest,
    WorldTotal,
    /// Inside `Player::tick`, summed over players: inbound packets, then chunk sending.
    PlayerPackets,
    PlayerChunks,
}

const NAMES: [&str; 18] = [
    "events",
    "tick",
    "datapack",
    "plugin_loaders",
    "scheduled",
    "worlds_wall",
    "player_data",
    "flush_updates",
    "w_flush",
    "w_env",
    "w_chunks",
    "w_players",
    "w_entities",
    "w_block_entities",
    "w_rest",
    "w_total",
    "p_packets",
    "p_chunks",
];

static SUMS: [AtomicU64; 18] = [const { AtomicU64::new(0) }; 18];
static MAX_WORLD: AtomicU64 = AtomicU64::new(0);
static WORLDS: AtomicU64 = AtomicU64::new(0);
static TICKS: AtomicU64 = AtomicU64::new(0);
/// Plugin event calls by event name: (calls, nanoseconds), including the wait for the plugin.
static EVENTS: std::sync::Mutex<Vec<(&'static str, u64, u64)>> = std::sync::Mutex::new(Vec::new());

pub fn event(name: &'static str, elapsed: Duration) {
    let mut events = EVENTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let ns = elapsed.as_nanos() as u64;
    match events.iter_mut().find(|e| e.0 == name) {
        Some(e) => {
            e.1 += 1;
            e.2 += ns;
        }
        None => events.push((name, 1, ns)),
    }
}

#[must_use]
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("PUMPKIN_TICK_PROFILE").is_ok_and(|v| v == "1"))
}

pub fn add(phase: Phase, elapsed: Duration) {
    SUMS[phase as usize].fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    if matches!(phase, Phase::WorldTotal) {
        MAX_WORLD.fetch_max(elapsed.as_nanos() as u64, Ordering::Relaxed);
        WORLDS.fetch_add(1, Ordering::Relaxed);
    }
}

/// Times `f` into `phase` when profiling is on.
pub fn time<T>(phase: Phase, f: impl FnOnce() -> T) -> T {
    if !enabled() {
        return f();
    }
    let start = Instant::now();
    let out = f();
    add(phase, start.elapsed());
    out
}

/// Called once per tick, after it ends.
pub fn end_tick(threads: usize) {
    if !enabled() || TICKS.fetch_add(1, Ordering::Relaxed) + 1 < 100 {
        return;
    }
    let ticks = TICKS.swap(0, Ordering::Relaxed) as f64;
    let mut line = String::from("TICKPROF ms/tick:");
    for (i, name) in NAMES.iter().enumerate() {
        let ns = SUMS[i].swap(0, Ordering::Relaxed) as f64;
        line.push_str(&format!(" {name}={:.2}", ns / ticks / 1e6));
    }
    let worlds = WORLDS.swap(0, Ordering::Relaxed) as f64 / ticks;
    let max = MAX_WORLD.swap(0, Ordering::Relaxed) as f64 / 1e6;
    line.push_str(&format!(" worlds={worlds:.0} w_max={max:.2} threads={threads}"));
    let mut events = std::mem::take(&mut *EVENTS.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    events.sort_by_key(|e| std::cmp::Reverse(e.2));
    for (name, calls, ns) in events.iter().take(6) {
        line.push_str(&format!(" | {name} {:.1}/t {:.2}ms/t", *calls as f64 / ticks, *ns as f64 / ticks / 1e6));
    }
    info!("{line}");
}

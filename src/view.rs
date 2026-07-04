//! The map-view plumbing between the command loop and the Bevy visualiser:
//! spawning the view subprocess (winit event loops can only be created once
//! per process, so `view` re-invokes this binary with --view) and keeping
//! its snapshot file — a live channel, rewritten after every successful
//! command and polled by the window — in sync with the game.

use postcard::{from_bytes, to_allocvec};

use crate::Error;
use crate::core::unit::UnitLocation;
use crate::game::Game;
use crate::visualiser;

pub(crate) fn view(game: &Game) -> Result<(), Error> {
    let snapshot_path = view_snapshot_path();
    write_view_snapshot(game, &snapshot_path)?;
    spawn_view_subprocess(&snapshot_path)
}

fn build_snapshot(game: &Game) -> visualiser::MapSnapshot {
    let hexes = game.state.map.all_locations()
        .into_iter()
        .map(|((x, y), terrain)| visualiser::HexDisplay { x, y, terrain })
        .collect();

    let units = game.state.units.values()
        .filter_map(|unit| match &unit.location {
            UnitLocation::OnMap(coords) => Some(visualiser::UnitDisplay {
                x: coords.x,
                y: coords.y,
                name: unit.name.clone(),
                faction: unit.faction.clone(),
            }),
            UnitLocation::Offmap(_) => None,
        })
        .collect();

    let victory_hexes = game.victory_hexes()
        .into_iter()
        .map(|hex| visualiser::VictoryHexDisplay { x: hex.x, y: hex.y, points: hex.points, name: hex.name })
        .collect();

    visualiser::MapSnapshot { hexes, units, victory_hexes }
}

/// One snapshot file per game session; view windows poll it for changes.
fn view_snapshot_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cse_view_{}.snapshot", std::process::id()))
}

// Written to a temp file first, then renamed into place (atomic on the same
// filesystem), so a polling view subprocess never reads a half-written snapshot.
fn write_view_snapshot(game: &Game, path: &std::path::Path) -> Result<(), Error> {
    let bin: Vec<u8> = to_allocvec(&build_snapshot(game))?;
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, &bin)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Rewrite the snapshot file if a view has been opened this session. A refresh
/// failure only warns: the player's command itself already succeeded.
pub(crate) fn refresh_view(game: &Game) {
    let snapshot_path = view_snapshot_path();
    if !snapshot_path.exists() {
        return;
    }
    if let Err(error) = write_view_snapshot(game, &snapshot_path) {
        eprintln!("Failed to refresh the map view: {}", error.error_message);
    }
}

/// Remove this session's snapshot file. Called by the command loop on exit;
/// a missing file is an open view window's cue to close itself.
pub fn cleanup_view() {
    let _ = std::fs::remove_file(view_snapshot_path());
}

// winit event loops can only be created once per process, so each `view` runs
// the visualiser in a fresh subprocess (this binary re-invoked with --view).
// The snapshot crosses over via a temp file the subprocess keeps polling, so
// the window follows the game as further commands change the state.
fn spawn_view_subprocess(snapshot_path: &std::path::Path) -> Result<(), Error> {
    let current_exe = std::env::current_exe()?;
    let mut child = std::process::Command::new(current_exe)
        .arg("--view")
        .arg(snapshot_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    // Reap the child when the window closes so it doesn't linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}

pub fn run_view_subprocess(snapshot_path: &str) -> Result<(), Error> {
    let contents = std::fs::read(snapshot_path)?;
    let snapshot: visualiser::MapSnapshot = from_bytes(&contents)?;
    visualiser::launch(snapshot, snapshot_path.into(), contents);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_snapshot_roundtrips_through_its_file() {
        let scenario = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scenarios/basic_scenario.scen"
        ))
        .unwrap();
        let game = Game::build(scenario).unwrap();
        let path = std::env::temp_dir()
            .join(format!("cse_test_snapshot_{}.snapshot", std::process::id()));

        write_view_snapshot(&game, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        let snapshot: visualiser::MapSnapshot = from_bytes(&bytes).unwrap();
        assert!(!snapshot.hexes.is_empty());
        // Only on-map units are drawn; the basic scenario has some.
        assert!(!snapshot.units.is_empty());
    }
}

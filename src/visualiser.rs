use bevy::prelude::*;
use hexx::{Hex, HexLayout, HexOrientation, OffsetHexMode, Vec2 as HexVec2};

use crate::core::location::Terrain;

// ── Snapshot data (plain data handed in from the game state) ─────────────────
// Serializable because the visualiser runs in a subprocess (winit event loops
// can only be created once per process) and the snapshot crosses over as postcard.

#[derive(Resource, serde::Serialize, serde::Deserialize)]
pub struct MapSnapshot {
    pub hexes: Vec<HexDisplay>,
    pub units: Vec<UnitDisplay>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct HexDisplay {
    pub x: u32,
    pub y: u32,
    pub terrain: Terrain,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct UnitDisplay {
    pub x: u32,
    pub y: u32,
    pub name: String,
    pub faction: String,
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn launch(snapshot: MapSnapshot) {
    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "CSE — Map View  [Esc to close]".into(),
                    resolution: (1200., 800.).into(),
                    ..default()
                }),
                ..default()
            }),
        )
        .insert_resource(snapshot)
        .add_plugins(MapViewPlugin)
        .run();
}

// ── Plugin ────────────────────────────────────────────────────────────────────

struct MapViewPlugin;

impl Plugin for MapViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, exit_on_esc);
    }
}

// ── Systems ───────────────────────────────────────────────────────────────────

const HEX_SIZE: f32 = 52.0;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    snapshot: Res<MapSnapshot>,
) {
    commands.spawn(Camera2d);

    let layout = HexLayout {
        orientation: HexOrientation::Pointy,
        scale: HexVec2::splat(HEX_SIZE),
        origin: HexVec2::ZERO,
    };

    // ── Terrain hexes ─────────────────────────────────────────────────────────
    for hex in &snapshot.hexes {
        let h = Hex::from_offset_coordinates(
            [hex.x as i32, hex.y as i32],
            OffsetHexMode::Even,
            HexOrientation::Pointy,
        );
        let pos = layout.hex_to_world_pos(h);

        commands.spawn((
            Mesh2d(meshes.add(RegularPolygon::new(HEX_SIZE * 0.93, 6))),
            MeshMaterial2d(materials.add(ColorMaterial::from(terrain_color(hex.terrain)))),
            Transform::from_xyz(pos.x, pos.y, 0.0),
        ));

        // Coordinate label at bottom of hex
        commands.spawn((
            Text2d::new(format!("{},{}", hex.x, hex.y)),
            TextFont {
                font_size: 11.0,
                ..default()
            },
            TextColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            Transform::from_xyz(pos.x, pos.y - HEX_SIZE * 0.55, 1.0),
        ));
    }

    // ── Unit markers ──────────────────────────────────────────────────────────
    // Track how many units we've placed per hex so stacked units offset nicely.
    let mut hex_unit_count: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();

    for unit in &snapshot.units {
        let slot = hex_unit_count.entry((unit.x, unit.y)).or_insert(0);
        let offset_x = *slot as f32 * HEX_SIZE * 0.28;
        *slot += 1;

        let h = Hex::from_offset_coordinates(
            [unit.x as i32, unit.y as i32],
            OffsetHexMode::Even,
            HexOrientation::Pointy,
        );
        let pos = layout.hex_to_world_pos(h);
        let cx = pos.x + offset_x;
        let cy = pos.y + HEX_SIZE * 0.15;

        commands.spawn((
            Mesh2d(meshes.add(Circle::new(HEX_SIZE * 0.22))),
            MeshMaterial2d(materials.add(ColorMaterial::from(faction_color(&unit.faction)))),
            Transform::from_xyz(cx, cy, 2.0),
        ));

        let label: String = if unit.name.len() > 18 {
            format!("{}…", &unit.name[..17])
        } else {
            unit.name.clone()
        };

        commands.spawn((
            Text2d::new(label),
            TextFont {
                font_size: 9.0,
                ..default()
            },
            TextColor(Color::WHITE),
            Transform::from_xyz(cx, cy, 3.0),
        ));
    }
}

fn exit_on_esc(keys: Res<ButtonInput<KeyCode>>, mut exit: EventWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.send(AppExit::Success);
    }
}

// ── Colour helpers ────────────────────────────────────────────────────────────

fn terrain_color(terrain: Terrain) -> Color {
    match terrain {
        Terrain::Plains   => Color::srgb(0.55, 0.76, 0.40),
        Terrain::Forest   => Color::srgb(0.18, 0.45, 0.18),
        Terrain::Hills    => Color::srgb(0.72, 0.60, 0.38),
        Terrain::Mountain => Color::srgb(0.58, 0.58, 0.60),
        Terrain::Swamp    => Color::srgb(0.38, 0.50, 0.30),
        Terrain::Desert   => Color::srgb(0.90, 0.82, 0.50),
        Terrain::Water    => Color::srgb(0.20, 0.42, 0.78),
        Terrain::Urban    => Color::srgb(0.50, 0.50, 0.52),
    }
}

fn faction_color(faction: &str) -> Color {
    match faction {
        "AX" => Color::srgb(0.80, 0.20, 0.20),
        "SU" => Color::srgb(0.20, 0.20, 0.80),
        _    => Color::srgb(0.50, 0.50, 0.50),
    }
}

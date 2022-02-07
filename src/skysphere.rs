use std::default::Default;
use std::f32::consts::PI;

use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;

use crate::AppState;

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_system_set(SystemSet::on_enter(AppState::Run).with_system(setup))
            .add_system_set(SystemSet::on_update(AppState::Run).with_system(move_light));
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let sky_color = materials.add(StandardMaterial {
        base_color: Color::Rgba {
            red: 0.443137,
            blue: 0.737255,
            green: 0.882353,
            alpha: 1.0,
        },
        unlit: true,
        ..Default::default()
    });
    let mut mesh = Mesh::from(shape::Icosphere {
        radius: 999.0,
        ..Default::default()
    });
    // Invert sphere
    let positions = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION);
    if let Some(VertexAttributeValues::Float32x3(positions)) = positions {
        for [x, y, z] in positions.iter_mut() {
            *x = -*x;
            *y = -*y;
            *z = -*z;
        }
    }
    let sphere = meshes.add(mesh);

    commands.spawn_bundle(PbrBundle {
        mesh: sphere,
        material: sky_color,
        ..Default::default()
    });
    // spawn sun ?:
    commands.spawn_bundle(DirectionalLightBundle {
        directional_light: DirectionalLight {
            shadows_enabled: true,
            illuminance: 250_000.0,
            color: Color::from([1.0, 1.0, 1.0]),
            ..Default::default()
        },
        ..Default::default()
    });
}

fn recalc(r: f32, theta: f32, fi: f32) -> [f32; 3] {
    [
        r * theta.sin() * fi.cos(),
        r * theta.sin() * fi.sin(),
        r * theta.cos(),
    ]
}

fn move_light(mut query: Query<&mut Transform, With<DirectionalLight>>, time: Res<Time>) {
    for mut t in query.iter_mut() {
        *t = Transform::from_translation(
            recalc(10.0, time.seconds_since_startup() as f32, PI / 2.0).into(),
        )
        .looking_at(Vec3::ZERO, Vec3::Y);
    }
}

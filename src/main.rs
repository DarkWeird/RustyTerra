// mod camera_rotation;

use bevy::diagnostic::LogDiagnosticsPlugin;
use bevy::{asset::LoadState, prelude::*};
use bevy_fly_camera::{FlyCamera, FlyCameraPlugin};

use crate::chunk::Relative;

mod blocks;
mod chunk;
mod generation;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AppState {
    Loading,
    Run,
}

struct Loading(Vec<HandleUntyped>);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(State::new(AppState::Loading))
        .insert_resource(Loading(Vec::new()))
        .add_state(AppState::Loading)
        .add_system_set(SystemSet::on_update(AppState::Loading).with_system(wait_loading))
        .add_system_set(SystemSet::on_update(AppState::Run).with_system(cursor_grab_system))
        .add_system_set(SystemSet::on_enter(AppState::Run).with_system(setup))
        .add_plugin(chunk::ChunkPlugin)
        .add_plugin(FlyCameraPlugin)
        .add_plugin(blocks::BlockPlugin)
        .add_plugin(LogDiagnosticsPlugin::default())
        .run();
}

fn wait_loading(
    mut state: ResMut<State<AppState>>,
    loading: Res<Loading>,
    asset_server: Res<AssetServer>,
) {
    if let LoadState::Loaded =
        asset_server.get_group_load_state(loading.0.iter().map(|handle| handle.id))
    {
        state.set(AppState::Run).unwrap(); // TODO!
    }
}

fn setup(mut commands: Commands) {
    commands.spawn_bundle(DirectionalLightBundle {
        ..Default::default()
    });
    commands
        .spawn_bundle(PerspectiveCameraBundle::new_3d())
        .insert(FlyCamera::default())
        .insert(crate::chunk::Relative([5, 5, 5]));
}

fn cursor_grab_system(
    mut windows: ResMut<Windows>,
    btn: Res<Input<MouseButton>>,
    key: Res<Input<KeyCode>>,
) {
    let window = windows.get_primary_mut().unwrap();

    if btn.just_pressed(MouseButton::Left) {
        window.set_cursor_lock_mode(true);
        window.set_cursor_visibility(false);
    }

    if key.just_pressed(KeyCode::Escape) {
        window.set_cursor_lock_mode(false);
        window.set_cursor_visibility(true);
    }
}

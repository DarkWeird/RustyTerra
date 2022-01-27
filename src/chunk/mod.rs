use std::collections::HashMap;
use std::default::Default;
use std::ops::{Add, Mul};

use bevy::prelude::*;
use bevy::render::render_resource::{
    AddressMode, Extent3d, PrimitiveTopology, SamplerDescriptor, TextureDimension,
};
use bevy::tasks::{ComputeTaskPool, Task};
use building_blocks::core::num::Zero;
use building_blocks::mesh::{
    greedy_quads, GreedyQuadsBuffer, IsOpaque, MergeVoxel, OrientedCubeFace, UnorientedQuad,
    RIGHT_HANDED_Y_UP_CONFIG,
};
use building_blocks::prelude::*;
use building_blocks::storage::{Array, Channel};
use futures_lite::future;
use noise::{NoiseFn, Perlin};

use rendering::UV_SCALE;

use crate::blocks::{Block, BlockId, Blocks};
use crate::{AppState, LoadState};

mod generation;
mod rendering;

pub struct ChunkPlugin;

impl Plugin for ChunkPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        let perlin = noise::Perlin::new();
        app.insert_resource(perlin)
            .insert_resource(ChunkWorld::default())
            .add_event::<ChunkEvent>()
            .add_system_set(
                SystemSet::on_update(AppState::Run)
                    .with_system(rendering::build_mesh)
                    .with_system(rendering::update_chunk)
                    .with_system(rendering::build_mesh_done)
                    .with_system(generation::generate_chunk)
                    .with_system(remove_from_world)
                    .with_system(add_to_world)
                    .with_system(relative_update)
                    .with_system(generation::chunk_is_done)
                    .with_system(chunk_location_add)
                    .with_system(chunk_location_transition),
            );
    }
}

pub struct ChunkWorld {
    world: HashMap<Point3i, Entity>,
}

impl Default for ChunkWorld {
    fn default() -> Self {
        ChunkWorld {
            world: HashMap::new(),
        }
    }
}

pub fn add_to_world(mut world: ResMut<ChunkWorld>, query: Query<(Entity, &Chunk), Changed<Chunk>>) {
    for (e, chunk) in query.iter() {
        // TODO remove
        world.world.insert(chunk.pos, e);
    }
}

pub fn remove_from_world(mut world: ResMut<ChunkWorld>, removals: RemovedComponents<Chunk>) {
    for e in removals.iter() {
        let possible_keys = world
            .world
            .iter()
            .filter(|(_, v)| *v == &e)
            .map(|(k, _)| k)
            .collect::<Vec<&Point3i>>();
        let first = possible_keys.first().map(|pos| **pos);

        if let Some(key) = first {
            world.world.remove(&key);
        }
    }
}

pub enum ChunkEvent {
    Generate(ChunkLocation),
    Update(ChunkLocation),
    Remove(ChunkLocation),
}

#[derive(Component)]
pub struct Relative(pub [i32; 3]); // TODO: Choose better name

pub fn chunk_location_add(
    mut commands: Commands,
    query: Query<(Entity, &Transform), (With<Relative>, Without<ChunkLocation>)>,
) {
    for (e, t) in query.iter() {
        let [x, y, z] = t.translation.to_array();
        commands.entity(e).insert(ChunkLocation(PointN([
            x as i32 / 32,
            y as i32 / 32,
            z as i32 / 32,
        ])));
    }
}

pub fn chunk_location_transition(
    mut query: Query<(&mut ChunkLocation, &Transform), Changed<Transform>>,
) {
    for (mut chunk_location, &transform) in query.iter_mut() {
        let [x, y, z] = transform.translation.to_array();
        let pos = PointN([x as i32 / 32, y as i32 / 32, z as i32 / 32]);
        if pos != chunk_location.0 {
            chunk_location.0 = pos
        }
    }
}

pub fn relative_update(
    world: Res<ChunkWorld>,
    query: Query<(&ChunkLocation, &Relative), Changed<ChunkLocation>>,
    mut event: EventWriter<ChunkEvent>,
) {
    for (chunk_location, relative) in query.iter() {
        let relative: &Relative = relative;
        let extends = Extent3i::from_min_and_max(chunk_location.0, chunk_location.0)
            .add(-to_bb(relative.0))
            .add_to_shape(to_bb(relative.0) * 2);

        for point in extends.iter_points() {
            if !world.world.contains_key(&point) {
                event.send(ChunkEvent::Generate(ChunkLocation(point)));
            }
        }
    }
}

fn to_bb<T: Into<[i32; 3]>>(pos: T) -> Point3i {
    PointN(pos.into())
}

fn chunk_location<T: Into<[i32; 3]>>(pos: T) -> ChunkLocation {
    ChunkLocation(to_bb(pos))
}

/// Basic voxel type with one byte of texture layers
#[derive(Default, Clone, Copy)]
pub struct Voxel(u8);

impl MergeVoxel for Voxel {
    type VoxelValue = u8;

    fn voxel_merge_value(&self) -> Self::VoxelValue {
        self.0
    }
}

impl IsOpaque for Voxel {
    fn is_opaque(&self) -> bool {
        true
    }
}

impl IsEmpty for Voxel {
    fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

#[derive(Debug, Default, Clone, Component)]
pub struct MeshBuf {
    data: HashMap<BlockId, BlockMesh>,
}

#[derive(Debug, Default, Clone, Component)]
pub struct BlockMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tex_coords: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl MeshBuf {
    fn add_quad(
        &mut self,
        face: &OrientedCubeFace,
        quad: &UnorientedQuad,
        u_flip_face: Axis3,
        block_id: BlockId,
    ) {
        let voxel_size = 1.0;
        let block_mesh = self.data.entry(block_id).or_insert(BlockMesh::default());

        let start_index = block_mesh.positions.len() as u32;
        block_mesh
            .positions
            .extend_from_slice(&face.quad_mesh_positions(quad, voxel_size));
        block_mesh
            .normals
            .extend_from_slice(&face.quad_mesh_normals());
        let flip_v = true;
        let mut uvs = face.tex_coords(u_flip_face, flip_v, quad);
        for uv in uvs.iter_mut() {
            for c in uv.iter_mut() {
                *c *= UV_SCALE;
            }
        }
        block_mesh.tex_coords.extend_from_slice(&uvs);
        block_mesh
            .indices
            .extend_from_slice(&face.quad_mesh_indices(start_index));
    }
}

#[derive(Clone, Component)]
pub struct Chunk {
    pos: Point3i,
    data: Array<[i32; 3], Channel<Voxel>>,
}

impl Default for Chunk {
    fn default() -> Self {
        let extent = Extent3i::from_min_and_shape(PointN::default(), PointN([32; 3])).padded(1);
        let voxels = Array3x1::fill(extent, Voxel::default());
        Chunk {
            pos: Point3i::zero(),
            data: voxels,
        }
    }
}

#[derive(Component)]
pub struct ChunkLocation(Point3i);

impl From<IVec3> for ChunkLocation {
    fn from(vec: IVec3) -> Self {
        ChunkLocation(PointN(vec.to_array()))
    }
}

impl From<Point3i> for ChunkLocation {
    fn from(point: Point3i) -> Self {
        ChunkLocation(point)
    }
}

impl From<ChunkLocation> for IVec3 {
    fn from(location: ChunkLocation) -> Self {
        IVec3::from(location.0 .0)
    }
}

impl From<ChunkLocation> for Point3i {
    fn from(location: ChunkLocation) -> Self {
        location.0
    }
}

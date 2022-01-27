use bevy::render::mesh::Indices;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::tasks::{ComputeTaskPool, Task};
use building_blocks::mesh::{greedy_quads, GreedyQuadsBuffer, RIGHT_HANDED_Y_UP_CONFIG};
use building_blocks::prelude::Get;
use futures_lite::future;

use crate::{
    Assets, AssetServer, BuildChildren, Changed, Commands, Entity, GlobalTransform, Mesh,
    PbrBundle, Query, Res, ResMut, StandardMaterial, Transform,
};
use crate::blocks::{Block, Blocks};
use crate::chunk::{BlockMesh, Chunk, MeshBuf};

pub const UV_SCALE: f32 = 0.1;

pub fn update_chunk(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    blocks: Res<Blocks>,
    assets: Res<Assets<Block>>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(Entity, &MeshBuf, &Transform), Changed<MeshBuf>>,
) {
    for (e, mesh_buf, transform) in query.iter() {
        let data = mesh_buf.clone().data;
        for (block_id, block_meshes) in data.iter() {
            let BlockMesh {
                positions,
                tex_coords,
                normals,
                indices,
            } = block_meshes.clone();
            let mut render_mesh = Mesh::new(PrimitiveTopology::TriangleList);
            let block = blocks.get_block(&assets, block_id).unwrap();
            let texture_handle = asset_server.get_handle(&*block.texture_name);

            render_mesh.set_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            render_mesh.set_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            render_mesh.set_attribute(Mesh::ATTRIBUTE_UV_0, tex_coords);
            render_mesh.set_indices(Some(Indices::U32(indices)));

            commands.entity(e).with_children(|builder| {
                let mut material: StandardMaterial = texture_handle.clone().into();
                material.reflectance = 0.25 * (*block_id as f32);
                builder.spawn_bundle(PbrBundle {
                    mesh: meshes.add(render_mesh),
                    material: materials.add(material),
                    global_transform: GlobalTransform::from(*transform), // TODO investigate why don't using parent's transform
                    ..Default::default()
                });
            });
        }
    }
}

pub fn build_mesh(
    mut commands: Commands,
    pool: Res<ComputeTaskPool>,
    query: Query<(Entity, &Chunk), Changed<Chunk>>,
) {
    for (e, chunk) in query.iter() {
        let chunk: Chunk = chunk.clone();
        let task = pool.spawn(async move {
            let extent = *chunk.data.extent();
            let mut greedy_buffer =
                GreedyQuadsBuffer::new(extent, RIGHT_HANDED_Y_UP_CONFIG.quad_groups());
            greedy_quads(&chunk.data, &extent, &mut greedy_buffer);

            let mut mesh_buf = MeshBuf::default();
            for group in greedy_buffer.quad_groups.iter() {
                for quad in group.quads.iter() {
                    let mat = &chunk.data.get(quad.minimum);
                    mesh_buf.add_quad(
                        &group.face,
                        quad,
                        RIGHT_HANDED_Y_UP_CONFIG.u_flip_face,
                        mat.0 as u32 - 1,
                    );
                }
            }
            mesh_buf
        });
        commands.entity(e).insert(task);
    }
}

pub fn build_mesh_done(mut commands: Commands, mut query: Query<(Entity, &mut Task<MeshBuf>)>) {
    for (e, mut task) in query.iter_mut() {
        if let Some(mesh_buf) = future::block_on(future::poll_once(&mut *task)) {
            commands.entity(e).insert(mesh_buf);
            commands.entity(e).remove::<Task<MeshBuf>>();
        }
    }
}

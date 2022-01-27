use crate::chunk::{Chunk, ChunkEvent, Voxel};
use crate::{Commands, Entity, EventReader, Query, Res, Transform, Vec3};
use bevy::tasks::{ComputeTaskPool, Task};
use building_blocks::core::num::abs;
use building_blocks::core::{Point3i, PointN};
use building_blocks::prelude::GetMut;
use futures_lite::future;
use noise::{NoiseFn, Perlin};
use std::ops::Mul;

pub fn generate_chunk(
    mut commands: Commands,
    noise: Res<Perlin>,
    pool: Res<ComputeTaskPool>,
    mut reader: EventReader<ChunkEvent>,
) {
    for event in reader.iter() {
        let event: &ChunkEvent = event;
        match event {
            ChunkEvent::Generate(pos) => {
                let pos = pos.0;
                let noise = noise.clone();
                let task = pool.spawn(async move {
                    let mut chunk = Chunk::default();
                    let global_pos = pos.mul(PointN([32; 3]) as Point3i);
                    for point in chunk.data.extent().padded(-1).iter_points() {
                        let pos = global_pos + point;
                        let pos_f64 = [
                            pos.x() as f64 / 32.0,
                            pos.y() as f64 / 32.0,
                            pos.z() as f64 / 32.0,
                        ];

                        let val = abs(noise.get(pos_f64) * 4.0) as u8;

                        let voxel = chunk.data.get_mut(point);
                        *voxel = Voxel(val);
                    }
                    chunk.pos = pos;
                    let transform = Transform::from_translation(Vec3::from([
                        pos.x() as f32 * 32.0,
                        pos.y() as f32 * 32.0,
                        pos.z() as f32 * 32.0,
                    ]));
                    (chunk, transform)
                });
                commands.spawn().insert(task);
            }
            ChunkEvent::Update(_) => {}
            ChunkEvent::Remove(_) => {}
        }
    }
}

pub fn chunk_is_done(
    mut commands: Commands,
    mut query: Query<(Entity, &mut Task<(Chunk, Transform)>)>,
) {
    for (e, mut task) in query.iter_mut() {
        if let Some((chunk, transform)) = future::block_on(future::poll_once(&mut *task)) {
            commands.entity(e).insert(chunk).insert(transform);
            commands.entity(e).remove::<Task<(Chunk, Transform)>>();
        }
    }
}

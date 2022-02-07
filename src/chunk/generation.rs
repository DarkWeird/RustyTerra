use std::cmp::max;
use bevy::prelude::*;
use building_blocks::core::{Extent2i, Extent3i, Point3i, PointN};
use building_blocks::prelude::{Array2x1, Array3x1, Get, GetMut };
use noise::{
    Fbm, MultiFractal, NoiseFn, Point2, Point3, ScaleBias, Seedable, SuperSimplex,
};

use crate::chunk::{Chunk, ChunkEvent, ChunkWorld, Voxel};
use crate::{
    App, AppState, Commands, Entity, EventReader, NonSendMut, Plugin, Query, Res, ResMut,
    SystemSet, Transform, Vec3,
};

pub struct ChunkGeneratorPlugin;

type Seed = i64;

impl Plugin for ChunkGeneratorPlugin {
    fn build(&self, app: &mut App) {
        app // TODO add labels
            .add_system_set(
                SystemSet::new()
                    .label("facets")
                    .with_system(provide_sealevel_facet)
                    .with_system(provide_noise_elevation_facet)
                    .with_system(provide_surface_facet)
                    .with_system(provide_density_facet)
                    .with_system(provide_roughness_facet)
                    .with_system(provide_density_noise),
            )
            .add_system_set(
                SystemSet::new()
                    .label("rasterizers")
                    .after("facets")
                    .with_system(generate_chunk_system),
            );
    }
}

#[derive(Component)]
struct Facet<T>(T);

struct SeaLevel(i32);

struct ElevationFacet(Facet2D<i32>);

struct SurfaceRoughnessFacet(Facet2D<f32>);

struct SurfaceFacet(Facet3D<bool>);

struct DensityFacet(Facet3D<f32>);

fn provide_sealevel_facet(
    mut commands: Commands,
    mut query: Query<(Entity, &GeneratingArea), Without<Facet<SeaLevel>>>,
) {
    for (e, generatingArea) in query.iter() {
        commands.entity(e).insert(Facet(SeaLevel(32)));
    }
}

fn provide_flat_elevation_facet(
    mut commands: Commands,
    query: Query<(Entity, &GeneratingArea), Without<Facet<ElevationFacet>>>,
) {
    for (e, generatingArea) in query.iter() {
        let mut facet = Facet2D::new(generatingArea.0);
        for val in facet.data.channels_mut().store_mut().iter_mut() {
            *val = 40;
        }
        commands.entity(e).insert(Facet(ElevationFacet(facet)));
    }
}

fn provide_noise_elevation_facet(
    mut commands: Commands,
    query: Query<(Entity, &GeneratingArea, &Facet<SeaLevel>), Without<Facet<ElevationFacet>>>,
) {
    for (e, generatingArea, sea_level) in query.iter() {
        let sea_level = sea_level.0 .0 as f64;
        let fbm = Fbm::new().set_octaves(8).set_seed(124235);
        let noise = SubSampleNoise::new(&fbm)
            .set_scale([0.004, 0.004, 1.0])
            .set_sample_rate(4);

        let mut facet = Facet2D::new(generatingArea.0);
        for pos in facet.data.extent().iter_points() {
            let mut val = facet.data.get_mut(pos);
            let mut x = noise.get([pos.x() as f64 + 0.1, pos.y() as f64 + 0.1]);
            x = sea_level + sea_level * ((x * 2.11 + 1.0) / 2.0);
            *val = x as i32;
        }
        commands.entity(e).insert(Facet(ElevationFacet(facet)));
    }
}

fn provide_roughness_facet(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &GeneratingArea,
            &Facet<SeaLevel>,
            &Facet<ElevationFacet>,
        ),
        Without<Facet<SurfaceRoughnessFacet>>,
    >,
) {
    for (e, area, sea_level, elevation) in query.iter() {
        let fbm = Fbm::new().set_octaves(8).set_seed(124235 + 92658);
        let noise = ScaleBias::new(&fbm).set_scale(0.0004); // TODO add sample rate :/

        let mut facet = SurfaceRoughnessFacet(Facet2D::new(area.0));
        let sea_level = sea_level.0 .0;
        let elevation = &elevation.0 .0;
        for pos in facet.0.data.extent().iter_points() {
            let value = facet.0.data.get_mut(pos);
            let height = elevation.data.get(pos) - sea_level;
            *value = (0.25
                + height as f64 * 0.007
                + noise.get([pos.x() as f64 / 500.0, pos.y() as f64 / 500.0]) * 1.5)
                as f32;
        }
        commands.entity(e).insert(Facet(facet));
    }
}

fn provide_surface_facet(
    mut commands: Commands,
    query: Query<(Entity, &GeneratingArea, &Facet<ElevationFacet>), Without<Facet<SurfaceFacet>>>,
) {
    for (e, area, elevation) in query.iter() {
        let elevation = &elevation.0;
        let mut surface = SurfaceFacet(Facet3D::new(area.0));
        for pos in elevation.0.data.extent().iter_points() {
            let height = elevation.0.data.get(pos);
            let pos = PointN([pos.x(), height, pos.y()]);
            if surface.0.data.contains(pos) {
                let value = surface.0.data.get_mut(pos);
                *value = true;
            }
        }
        commands.entity(e).insert(Facet(surface));
    }
}

fn provide_density_facet(
    mut commands: Commands,
    query: Query<(Entity, &GeneratingArea, &Facet<ElevationFacet>), Without<Facet<DensityFacet>>>,
) {
    for (e, area, elevation) in query.iter() {
        let elevation = &elevation.0;
        let mut density = DensityFacet(Facet3D::new(area.0));
        for pos in elevation.0.data.extent().iter_points() {
            let height = elevation.0.data.get(pos);
            let min_y = density.0.data.extent().minimum.y();
            let max_y = min_y + density.0.data.extent().shape.y();
            for y in min_y..max_y {
                let pos = PointN([pos.x(), y, pos.y()]);
                let value = density.0.data.get_mut(pos);
                *value = (height - y) as f32;
            }
        }
        commands.entity(e).insert(Facet(density));
    }
}

fn provide_density_noise(
    mut query: Query<
        (
            &Facet<SurfaceRoughnessFacet>,
            &mut Facet<DensityFacet>,
            &mut Facet<SurfaceFacet>,
        ),
        With<GeneratingArea>,
    >,
) {
    for (roughness, mut density, mut surface) in query.iter_mut() {
        let density = &mut density.0 .0;
        let surface = &mut surface.0 .0;
        let roughness = &roughness.0 .0;
        let fbm = Fbm::new()
            .set_octaves(8)
            .set_seed(124235)
            .set_persistence(1.0);
        let large_noise = SubSampleNoise::new(&fbm)
            .set_scale([0.015, 0.02, 0.015])
            .set_sample_rate(4);
        let small_noise = SubSampleNoise::new(&fbm)
            .set_scale([0.005, 0.007, 0.005])
            .set_sample_rate(4);
        for pos in density.data.extent().iter_points() {
            let value = density.data.get_mut(pos);
            let intensity = f32::max(0.0, roughness.data.get(pos.xy()));
            let small_intensity = f32::min(intensity, (1.0 + intensity) / 2.0);
            let large_intensity = intensity - small_intensity;

            *value = *value
                + small_noise.get([pos.x() as f64, pos.y() as f64, pos.z() as f64]) as f32
                    * intensity
                    * 20.0
                + large_noise.get([pos.x() as f64, pos.y() as f64, pos.z() as f64]) as f32
                    * large_intensity
                    * 60.0;
        }

        for pos in surface.data.extent().iter_points() {
            if density.data.contains(pos) && density.data.contains(pos + PointN([0, 1, 0])) {
                let value = surface.data.get_mut(pos);
                *value =
                    density.data.get(pos) > 0.0 && density.data.get(pos + PointN([0, 1, 0])) <= 0.0;
            }
        }
    }
}

fn generate_chunk_system(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &mut Chunk,
        &Facet<SurfaceFacet>,
        &Facet<DensityFacet>,
        &Facet<SeaLevel>,
    ), With<GeneratingArea>>,
) {
    const DIRT: i32 = 1;
    const STONE: i32 = 2;
    const WATER: i32 = 4;
    for (e, mut chunk, surface, solidity, sealevel) in query.iter_mut() {
        let solidity = &solidity.0;
        let surface = &surface.0;
        let sealevel = &sealevel.0;
        let sea_level = sealevel.0;

        for pos in chunk.data.extent().iter_points() {
            let value = chunk.data.get_mut(pos);
            let density = solidity.0.data.get(pos);
            let pos_y = pos.y() + max(0, density as i32);

            if pos.y() < sea_level && pos_y > sea_level {
                *value = Voxel(WATER as u8);
            } else if density > 0.0 && surface.0.data.get(pos) {
                *value = Voxel(DIRT as u8);
            } else if density > 0.0 {
                if density > 32.0 {
                    *value = Voxel(STONE as u8);
                } else {
                    *value = Voxel(DIRT as u8);
                }
            } else if pos_y <= sea_level {
                *value = Voxel(WATER as u8);
            }
        }
        commands.entity(e).remove::<GeneratingArea>();
    }
}

#[derive(Component)]
pub struct GeneratingArea(Extent3i);

pub struct Facet2D<T> {
    data: Array2x1<T>,
}

impl<T: Default + Clone> Facet2D<T> {
    fn new(region: Extent3i) -> Self {
        let pos = region.minimum.xz();
        let shape = region.shape.xz();
        let area = Extent2i::from_min_and_shape(pos, shape);
        Facet2D {
            data: Array2x1::fill(area, T::default()),
        }
    }
}

//
pub struct Facet3D<T> {
    data: Array3x1<T>,
}

impl<T: Default + Clone> Facet3D<T> {
    fn new(region: Extent3i) -> Self {
        Facet3D {
            data: Array3x1::fill(region, T::default()),
        }
    }
}

//// TODO OLD CODE BELOW

pub fn generate_chunk(
    mut commands: Commands,
    mut reader: EventReader<ChunkEvent>,
    world: Res<ChunkWorld>,
) {
    for event in reader.iter() {
        let event: &ChunkEvent = event;
        match event {
            ChunkEvent::Generate(pos) => {
                let pos = pos.0;
                if !world.world.contains_key(&pos) {
                    let transform = Transform::from_translation(Vec3::from([
                        pos.x() as f32 * 32.0,
                        pos.y() as f32 * 32.0,
                        pos.z() as f32 * 32.0,
                    ]));
                    let extent =
                        Extent3i::from_min_and_shape(pos * PointN([32; 3]), PointN([32; 3]));

                    let mut chunk = Chunk {
                        data: Array3x1::fill(extent, Voxel::default()),
                        pos,
                    };
                    chunk.pos = pos;

                    commands
                        .spawn()
                        .insert(GeneratingArea(extent))
                        .insert(transform)
                        .insert(chunk);
                }
            }
            ChunkEvent::Update(_) => {}
            ChunkEvent::Remove(_) => {}
        }
    }
}

pub struct SubSampleNoise<'a, T: 'a> {
    /// Outputs a value.
    pub source: &'a dyn NoiseFn<T>,

    /// Scaling factor to apply to the output value from the source function.
    /// The default value is 1.0.
    pub scale: [f64; 3],
    pub sample_rate: i32,
}

impl<'a, T: 'a> SubSampleNoise<'a, T> {
    pub fn new(source: &'a dyn NoiseFn<T>) -> Self {
        Self {
            source,
            scale: [1f64; 3],
            sample_rate: 1,
        }
    }

    pub fn set_scale(self, scale: [f64; 3]) -> Self {
        Self { scale, ..self }
    }

    pub fn set_sample_rate(self, sample_rate: i32) -> Self {
        Self {
            sample_rate,
            ..self
        }
    }
}

impl<'a> NoiseFn<Point2<f64>> for SubSampleNoise<'a, Point2<f64>> {
    fn get(&self, point: Point2<f64>) -> f64 {
        let [x, y] = point;
        let x_mod = math::modulus(x, self.sample_rate as f64);
        let y_mod = math::modulus(y, self.sample_rate as f64);

        let x0 = x - x_mod;
        let x1 = x0 + self.sample_rate as f64;
        let y0 = y - y_mod;
        let y1 = y0 + self.sample_rate as f64;

        let q00 = self.source.get([x0 * self.scale[0], y0 * self.scale[1]]);
        let q10 = self.source.get([x1 * self.scale[0], y0 * self.scale[1]]);
        let q01 = self.source.get([x0 * self.scale[0], y1 * self.scale[1]]);
        let q11 = self.source.get([x1 * self.scale[0], y1 * self.scale[1]]);
        math::bi_lerp(
            q00,
            q10,
            q01,
            q11,
            x_mod / self.sample_rate as f64,
            y_mod / self.sample_rate as f64,
        )
    }
}

impl<'a> NoiseFn<Point3<f64>> for SubSampleNoise<'a, Point3<f64>> {
    fn get(&self, point: Point3<f64>) -> f64 {
        let [x, y, z] = point;
        let x_mod = math::modulus(x, self.sample_rate as f64);
        let y_mod = math::modulus(y, self.sample_rate as f64);
        let z_mod = math::modulus(z, self.sample_rate as f64);

        let x0 = x - x_mod;
        let x1 = x0 + self.sample_rate as f64;
        let y0 = y - y_mod;
        let y1 = y0 + self.sample_rate as f64;
        let z0 = z - z_mod;
        let z1 = z0 + self.sample_rate as f64;

        let q000 = self
            .source
            .get([x0 * self.scale[0], y0 * self.scale[1], z0 * self.scale[2]]);
        let q100 = self
            .source
            .get([x1 * self.scale[0], y0 * self.scale[1], z0 * self.scale[2]]);
        let q010 = self
            .source
            .get([x0 * self.scale[0], y1 * self.scale[1], z0 * self.scale[2]]);
        let q110 = self
            .source
            .get([x1 * self.scale[0], y1 * self.scale[1], z0 * self.scale[2]]);
        let q001 = self
            .source
            .get([x0 * self.scale[0], y0 * self.scale[1], z1 * self.scale[2]]);
        let q101 = self
            .source
            .get([x1 * self.scale[0], y0 * self.scale[1], z1 * self.scale[2]]);
        let q011 = self
            .source
            .get([x0 * self.scale[0], y1 * self.scale[1], z1 * self.scale[2]]);
        let q111 = self
            .source
            .get([x1 * self.scale[0], y1 * self.scale[1], z1 * self.scale[2]]);
        math::tri_lerp(
            q000,
            q100,
            q010,
            q110,
            q001,
            q101,
            q011,
            q111,
            x_mod / self.sample_rate as f64,
            y_mod / self.sample_rate as f64,
            z_mod / self.sample_rate as f64,
        )
    }
}

mod math {
    pub(crate) fn modulus(dividend: f64, divisor: f64) -> f64 {
        ((dividend % divisor) + divisor) % divisor
    }

    pub(crate) fn bi_lerp(q00: f64, q10: f64, q01: f64, q11: f64, tx: f64, ty: f64) -> f64 {
        let lerp_x1 = lerp(q00, q10, tx);
        let lerp_x2 = lerp(q01, q11, tx);
        lerp(lerp_x1, lerp_x2, ty)
    }

    pub(crate) fn lerp(a: f64, b: f64, t: f64) -> f64 {
        a + t * (b - a)
    }

    pub(crate) fn tri_lerp(
        q000: f64,
        q100: f64,
        q010: f64,
        q110: f64,
        q001: f64,
        q101: f64,
        q011: f64,
        q111: f64,
        tx: f64,
        ty: f64,
        tz: f64,
    ) -> f64 {
        let x00 = lerp(q000, q100, tx);
        let x10 = lerp(q010, q110, tx);
        let x01 = lerp(q001, q101, tx);
        let x11 = lerp(q011, q111, tx);
        let y0 = lerp(x00, x10, ty);
        let y1 = lerp(x01, x11, ty);
        lerp(y0, y1, tz)
    }
}

use std::borrow::Borrow;

use bevy::asset::{Asset, AssetLoader, BoxedFuture, LoadContext, LoadedAsset};
use bevy::prelude::*;
use bevy::reflect::TypeUuid;
use bevy::render::render_resource::{AddressMode, SamplerDescriptor};
use bevy::utils::HashMap;
use serde_derive::Deserialize;

use crate::{AppState, Loading};

pub type BlockId = u32;

pub const AIR_BLOCK_ID: BlockId = 0;

pub struct BlockPlugin;

impl Plugin for BlockPlugin {
    fn build(&self, app: &mut App) {
        app.add_asset::<Block>()
            .init_asset_loader::<BlockLoader>()
            .init_resource::<Blocks>()
            .init_resource::<BlockLoading>()
            .add_system(block_materials)
            .add_system_set(SystemSet::on_enter(AppState::Loading).with_system(load_all))
            .add_system_set(SystemSet::on_exit(AppState::Loading).with_system(loaded));
    }
}

#[derive(Default)]
pub struct Blocks {
    next_id: BlockId,
    map: HashMap<BlockId, Handle<Block>>,
}

impl Blocks {
    pub fn add_block(&mut self, handle: Handle<Block>) {
        self.map.insert(self.next_id, handle);
        self.next_id += 1;
    }
    pub fn insert_block(&mut self, id: BlockId, handle: Handle<Block>) {
        self.map.insert(id, handle);
        self.next_id = id + 1;
    }
    pub fn get_block(&self, assets: &Assets<Block>, id: &BlockId) -> Option<Block> {
        let handle = self.map.get(id)?;
        assets.get(handle).cloned()
    }
}

#[derive(Default)]
struct BlockLoading(Vec<HandleUntyped>); // TODO made generic loading..

fn load_all(
    asset_server: Res<AssetServer>,
    mut loading: ResMut<Loading>,
    mut blocks: ResMut<BlockLoading>,
) {
    match asset_server.load_folder("block") {
        Ok(mut vec) => {
            debug!("Loading {} blocks", vec.len());
            loading.0.append(&mut vec.clone());
            blocks.0.append(&mut vec);
        }
        Err(e) => {
            error!("Blocks cannot be loaded {:?}", e);
        }
    }
}

fn loaded(
    assets_blocks: Res<Assets<Block>>,
    mut blocks: ResMut<Blocks>,
    blocks_loaded: Res<BlockLoading>,
) {
    for handle in &blocks_loaded.0 {
        let handle = assets_blocks.get_handle(handle);
        blocks.add_block(handle);
    }
}

#[derive(Default)]
struct BlockLoader;

impl AssetLoader for BlockLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, anyhow::Result<(), anyhow::Error>> {
        Box::pin(async move {
            let str = String::from_utf8_lossy(bytes);
            let block: Block = ron::from_str(str.borrow())?;
            let texture_name = &*block.texture_name.clone();
            let mut asset = LoadedAsset::new(block);
            asset.add_dependency(texture_name.into());
            load_context.set_default_asset(asset);
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["block.ron"]
    }
}

#[derive(Debug, Clone, TypeUuid, Deserialize)]
#[uuid = "8e70a904-cb5f-447d-98e7-d22d63c1a5e7"]
pub struct Block {
    pub texture_name: String,
    pub liquid: bool,
    pub opaque: bool,
}

pub fn block_materials(
    mut reader: EventReader<AssetEvent<Image>>,
    mut images: ResMut<Assets<Image>>,
) {
    for e in reader.iter() {
        let e: &AssetEvent<Image> = e;
        if let AssetEvent::Created { handle } = e {
            let image = images.get_mut(handle).unwrap();
            image.sampler_descriptor = SamplerDescriptor {
                address_mode_u: AddressMode::Repeat,
                address_mode_v: AddressMode::Repeat,
                ..Default::default()
            };
        }
    }
}

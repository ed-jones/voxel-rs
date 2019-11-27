pub mod vox;

use crate::{
    block::{Block, BlockMesh, BlockType},
    registry::Registry,
};

use crate::data::vox::{load_voxel_model, VoxelModel};
use anyhow::{Context, Result};
use image::{ImageBuffer, Rgba};
use log::info;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use texture_packer::{TexturePacker, TexturePackerConfig};

#[derive(Debug, Clone)]
pub struct Data {
    pub blocks: Registry<Block>,
    pub meshes: Vec<BlockMesh>,
    pub texture_atlas: ImageBuffer<Rgba<u8>, Vec<u8>>,
    pub models: Registry<VoxelModel>,
}

// TODO: decent error handling
pub fn load_data(data_directory: PathBuf) -> Result<Data> {
    info!("Loading data from directory {}", data_directory.display());

    // Load textures
    let mut textures: Vec<PathBuf> = Vec::new();
    let mut texture_registry: Registry<()> = Default::default();
    let textures_directory = data_directory.join("textures");
    info!(
        "Loading textures from directory {}",
        textures_directory.display()
    );
    for dir_entry in fs::read_dir(textures_directory).context("couldn't read textures directory")? {
        let dir_entry = dir_entry.context("failed to read directory entry")?;
        if dir_entry
            .file_type()
            .context("failed to get file type")?
            .is_file()
        {
            let file_path = dir_entry.path();

            texture_registry.register(
                file_path
                    .file_stem()
                    .context("failed to get file stem")?
                    .to_str()
                    .unwrap()
                    .to_owned(),
                (),
            )?;
            textures.push(file_path);
        }
    }

    let (texture_atlas, texture_rects) = load_textures(textures)?;
    dbg!(&texture_rects);

    //Load model
    let mut models = Registry::default();

    /*  let mut full = vec![false; 5*5*5];
    for i in 1..=3{
        for j in 1..=3{
            for k in 1..=3{
                full[i*5*5+j*5+k] = true;
            }
        }
    }

    full[0*5*5+2*5+2] = true;
    full[4*5*5+2*5+2] = true;
    full[2*5*5+0*5+2] = true;
    full[2*5*5+4*5+2] = true;
    full[2*5*5+2*5+0] = true;
    full[2*5*5+2*5+4] = true;


    let model_tree = VoxelModel{
        size_x: 5,
        size_y: 5,
        size_z: 5,
        voxels: vec![0x00FF0000; 5*5*5],
        full,
    };*/

    // TODO : load every .vox in the model folder
    let model_tree = load_voxel_model("data/model/tree.vox").unwrap();
    models.register("tree".to_owned(), model_tree)?;
    let model_knight = load_voxel_model("data/model/chr_knight.vox").unwrap();
    models.register("knight".to_owned(), model_knight)?;

    // Load blocks
    let mut block_datas: Vec<(String, BlockType)> = Vec::new();
    let blocks_directory = data_directory.join("blocks");
    info!(
        "Loading blocks from directory {}",
        blocks_directory.display()
    );
    for dir_entry in fs::read_dir(blocks_directory).context("couldn't read block directory")? {
        let dir_entry = dir_entry.context("failed to read directory entry")?;
        if dir_entry
            .file_type()
            .context("failed to get file type")?
            .is_file()
        {
            let file_path = dir_entry.path();

            match file_path.extension() {
                None => panic!("No file extension"),
                Some(ext) => {
                    if ext == "ron" {
                        let mut file = fs::File::open(file_path.clone())
                            .context("couldn't open .ron block file")?;
                        let mut buffer = String::new();
                        file.read_to_string(&mut buffer)?;
                        block_datas.push((
                            file_path
                                .file_stem()
                                .context("failed to get file stem")?
                                .to_str()
                                .unwrap()
                                .to_owned(),
                            ron::de::from_str(&buffer)
                                .context("failed to parse .ron block file")?,
                        ));
                    } else {
                        panic!("Unsupported file extension");
                    }
                }
            }
        }
    }

    info!("Processing collected block and texture data");
    let mut blocks = Registry::default();
    let mut meshes = Vec::new();
    // Add air
    blocks.register(
        "air".to_owned(),
        Block {
            name: "air".to_owned(),
            block_type: BlockType::Air,
        },
    )?;
    meshes.push(BlockMesh::Empty);

    for (name, block_type) in block_datas.into_iter() {
        let block = Block {
            name: name.clone(),
            block_type: block_type.clone(),
        };
        blocks.register(name, block)?;
        let mesh = match block_type {
            BlockType::Air => BlockMesh::Empty,
            // TODO: make sure there are exactly 6 face textures
            BlockType::NormalCube {
                face_textures: names,
            } => BlockMesh::FullCube {
                textures: [
                    texture_rects[texture_registry.get_id_by_name(&names[0]).unwrap() as usize],
                    texture_rects[texture_registry.get_id_by_name(&names[1]).unwrap() as usize],
                    texture_rects[texture_registry.get_id_by_name(&names[2]).unwrap() as usize],
                    texture_rects[texture_registry.get_id_by_name(&names[3]).unwrap() as usize],
                    texture_rects[texture_registry.get_id_by_name(&names[4]).unwrap() as usize],
                    texture_rects[texture_registry.get_id_by_name(&names[5]).unwrap() as usize],
                ],
            },
        };
        meshes.push(mesh);
    }

    info!("Data successfully loaded");
    Ok(Data {
        blocks,
        meshes,
        texture_atlas,
        models,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextureRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

const MAX_TEXTURE_SIZE: u32 = 2048;

const TEXTURE_PACKER_CONFIG: TexturePackerConfig = TexturePackerConfig {
    max_width: MAX_TEXTURE_SIZE,
    max_height: MAX_TEXTURE_SIZE,
    allow_rotation: false,
    border_padding: 0,
    texture_padding: 0,
    trim: false,
    texture_outlines: false,
};

/// Load given textures to a unique texture atlas
fn load_textures(
    textures: Vec<PathBuf>,
) -> Result<(ImageBuffer<Rgba<u8>, Vec<u8>>, Vec<TextureRect>)> {
    use image::GenericImage;
    use texture_packer::{exporter::ImageExporter, importer::ImageImporter};

    let mut packer = TexturePacker::new_skyline(TEXTURE_PACKER_CONFIG);
    for (i, path) in textures.iter().enumerate() {
        packer.pack_own(
            format!("{}", i),
            ImageImporter::import_from_file(path).expect("Failed to read texture to pack"),
        );
    }

    let mut texture_buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::new(MAX_TEXTURE_SIZE, MAX_TEXTURE_SIZE);
    texture_buffer.copy_from(
        &ImageExporter::export(&packer).expect("Failed to export texture from packer"),
        0,
        0,
    );
    texture_buffer.save("atlas.png");
    Ok((
        texture_buffer,
        (0..textures.len())
            .map(|i| {
                let frame = packer
                    .get_frame(&format!("{}", i))
                    .expect("Texture packer frame key doesn't exist")
                    .frame;
                TextureRect {
                    x: frame.x as f32 / MAX_TEXTURE_SIZE as f32,
                    y: frame.y as f32 / MAX_TEXTURE_SIZE as f32,
                    width: frame.w as f32 / MAX_TEXTURE_SIZE as f32,
                    height: frame.h as f32 / MAX_TEXTURE_SIZE as f32,
                }
            })
            .collect(),
    ))
}

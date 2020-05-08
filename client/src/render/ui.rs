//! Ui rendering

use super::{ buffer_from_slice, to_u8_slice };
use super::buffers::DynamicBuffer;
use super::init::ShaderStage;
use crate::ui::PrimitiveBuffer;
use crate::window::{WindowBuffers, WindowData};
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};
use std::collections::{BTreeMap, HashMap};
use wgpu_glyph::FontId;

pub struct UiRenderer {
    // Glyph rendering
    glyph_brush: wgpu_glyph::GlyphBrush<'static, ()>,
    fonts: HashMap<String, FontId>,
    // Rectangle rendering
    transform_buffer: wgpu::Buffer,
    uniforms_bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: DynamicBuffer<UiVertex>,
    index_buffer: DynamicBuffer<u32>,
}

impl<'a> UiRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        // Load fonts
        let default_font: &'static [u8] =
            include_bytes!("../../../assets/fonts/IBMPlexMono-Regular.ttf");
        let mut glyph_brush_builder = wgpu_glyph::GlyphBrushBuilder::using_font_bytes(default_font)
            .expect("Failed to load default font.");
        log::info!("Loading fonts from assets/fonts/list.toml");
        let mut fonts = HashMap::new();
        let font_list = std::fs::read_to_string("assets/fonts/list.toml")
            .expect("Couldn't read font list file");
        let font_files: BTreeMap<String, String> =
            toml::de::from_str(&font_list).expect("Couldn't parse font list file");
        for (font_name, font_file) in font_files.into_iter() {
            use std::io::Read;
            log::info!("Loading font {} from file {}", font_name, font_file);
            let mut font_bytes = Vec::new();
            let mut file = std::fs::File::open(font_file).expect("Couldn't open font file");
            file.read_to_end(&mut font_bytes)
                .expect("Couldn't read font file");
            fonts.insert(font_name, glyph_brush_builder.add_font_bytes(font_bytes));
        }
        log::info!("Fonts successfully loaded");
        let glyph_brush = glyph_brush_builder
            //.depth_stencil_state(DEFAULT_DEPTH_STENCIL_STATE_DESCRIPTOR)
            .build(device, crate::window::COLOR_FORMAT);

        // Create uniform buffer
        let transform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 64,
            usage: (wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST),
        });

        // Create bind group layout
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            bindings: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::UniformBuffer { dynamic: false },
            }],
        });

        // Create bind group
        let uniforms_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &uniform_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &transform_buffer,
                    range: 0..16,
                },
            }],
        });

        // Create shader modules
        let vertex_shader =
            super::init::load_glsl_shader(ShaderStage::Vertex, "assets/shaders/gui-rect.vert");
        let fragment_shader =
            super::init::load_glsl_shader(ShaderStage::Fragment, "assets/shaders/gui-rect.frag");

        let pipeline = super::init::create_default_pipeline(
            device,
            &uniform_layout,
            &vertex_shader,
            &fragment_shader,
            wgpu::PrimitiveTopology::TriangleList,
            wgpu::VertexBufferDescriptor {
                stride: std::mem::size_of::<UiVertex>() as u64,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &UI_VERTEX_ATTRIBUTES,
            },
            false,
        );

        Self {
            glyph_brush,
            fonts,
            transform_buffer,
            uniforms_bind_group,
            pipeline,
            vertex_buffer: DynamicBuffer::with_capacity(device, 64, wgpu::BufferUsage::VERTEX),
            index_buffer: DynamicBuffer::with_capacity(device, 64, wgpu::BufferUsage::INDEX),
        }
    }

    pub fn render<Message>(
        &mut self,
        buffers: WindowBuffers<'a>,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        data: &WindowData,
        ui: &quint::Ui<PrimitiveBuffer, Message>,
        gui: &mut crate::gui::Gui,
        draw_crosshair: bool,
    ) {
        // Render test dropdown
        let primitive_buffer = gui.drain_primitives();

        // ui.render(&mut primitive_buffer);

        // Render primitives
        let mut rect_vertices: Vec<UiVertex> = Vec::new();
        let mut rect_indices: Vec<u32> = Vec::new();

        use crate::ui::{RectanglePrimitive, TextPrimitive, TrianglesPrimitive};

        // Rectangles
        for RectanglePrimitive {
            layout: l,
            color,
            z,
        } in primitive_buffer.rectangle.into_iter()
        {
            let a = UiVertex {
                position: [l.x, l.y, z],
                color: color.clone(),
            };
            let b = UiVertex {
                position: [l.x + l.width, l.y, z],
                color: color.clone(),
            };
            let c = UiVertex {
                position: [l.x, l.y + l.height, z],
                color: color.clone(),
            };
            let d = UiVertex {
                position: [l.x + l.width, l.y + l.height, z],
                color: color.clone(),
            };
            let a_index = rect_vertices.len() as u32;
            let b_index = a_index + 1;
            let c_index = b_index + 1;
            let d_index = c_index + 1;
            rect_vertices.extend([a, b, c, d].iter());
            rect_indices.extend([b_index, a_index, c_index, b_index, c_index, d_index].iter());
        }
        // Triangles
        for TrianglesPrimitive {
            vertices,
            indices,
            color,
        } in primitive_buffer.triangles.into_iter()
        {
            let index_offset = rect_vertices.len() as u32;
            rect_vertices.extend(
                vertices
                    .into_iter()
                    .map(|v| UiVertex { position: v, color }),
            );
            rect_indices.extend(indices.into_iter().map(|id| id + index_offset));
        }
        // Text
        for TextPrimitive {
            x, y, w, h,
            mut parts,
            z,
            center_horizontally, center_vertically,
        } in primitive_buffer.text.into_iter()
        {
            let scale = data.hidpi_factor;

            // Apply DPI to font size
            for p in parts.iter_mut() {
                p.font_size.x *= scale as f32;
                p.font_size.y *= scale as f32;
            }
            // Get font IDs
            let Self { ref fonts, .. } = &self;
            let parts = parts
                .iter()
                .map(|part| wgpu_glyph::SectionText {
                    text: &part.text,
                    scale: part.font_size,
                    color: part.color,
                    font_id: part
                        .font
                        .clone()
                        .and_then(|f| fonts.get(&f).cloned())
                        .unwrap_or_default(),
                })
                .collect();

            // Calculate positions
            let physical_position: PhysicalPosition<f32> = PhysicalPosition::from_logical(LogicalPosition::new(x, y), scale);
            let mut x = physical_position.x;
            let mut y = physical_position.y;

            let w = match w {
                Some(w) => w as f32,
                None => std::f32::INFINITY,
            };
            let h = match h {
                Some(h) => h as f32,
                None => std::f32::INFINITY,
            };
            let physical_size: PhysicalSize<f32> = PhysicalSize::from_logical(LogicalSize::new(w, h), scale);
            let (w, h) = physical_size.into();

            if center_horizontally {
                x += w/2.0;
            }
            if center_vertically {
                y += h/2.0;
            }

            let v_align = if center_vertically {
                wgpu_glyph::VerticalAlign::Center
            } else {
                wgpu_glyph::VerticalAlign::Top
            };
            let h_align = if center_horizontally {
                wgpu_glyph::HorizontalAlign::Center
            } else {
                wgpu_glyph::HorizontalAlign::Left
            };
            let section = wgpu_glyph::VariedSection {
                text: parts,
                screen_position: (x, y),
                bounds: (w, h),
                z,
                layout: wgpu_glyph::Layout::Wrap {
                    line_breaker: Default::default(),
                    v_align,
                    h_align,
                },
            };
            self.glyph_brush.queue(section);
        }
        // Crosshair
        if draw_crosshair {
            let (cx, cy) = (
                data.logical_window_size.width as f32 / 2.0,
                data.logical_window_size.height as f32 / 2.0,
            );
            const HALF_HEIGHT: f32 = 15.0;
            const HALF_WIDTH: f32 = 2.0;
            const COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.5];
            let v1 = UiVertex {
                position: [cx - HALF_WIDTH, cy - HALF_HEIGHT, -1.0],
                color: COLOR,
            };
            let v2 = UiVertex {
                position: [cx + HALF_WIDTH, cy - HALF_HEIGHT, -1.0],
                color: COLOR,
            };
            let v3 = UiVertex {
                position: [cx - HALF_WIDTH, cy + HALF_HEIGHT, -1.0],
                color: COLOR,
            };
            let v4 = UiVertex {
                position: [cx + HALF_WIDTH, cy + HALF_HEIGHT, -1.0],
                color: COLOR,
            };
            let v5 = UiVertex {
                position: [cx - HALF_HEIGHT, cy - HALF_WIDTH, -1.0],
                color: COLOR,
            };
            let v6 = UiVertex {
                position: [cx + HALF_HEIGHT, cy - HALF_WIDTH, -1.0],
                color: COLOR,
            };
            let v7 = UiVertex {
                position: [cx - HALF_HEIGHT, cy + HALF_WIDTH, -1.0],
                color: COLOR,
            };
            let v8 = UiVertex {
                position: [cx + HALF_HEIGHT, cy + HALF_WIDTH, -1.0],
                color: COLOR,
            };
            let voffset = rect_vertices.len() as u32;
            rect_vertices.extend([v1, v2, v3, v4, v5, v6, v7, v8].iter());
            rect_indices.extend(
                [0, 1, 2, 1, 2, 3, 4, 5, 6, 5, 6, 7]
                    .iter()
                    .map(|id| id + voffset),
            );
        }

        // Draw rectangles
        {
            let (win_w, win_h) = (
                data.logical_window_size.width,
                data.logical_window_size.height,
            );
            // Update the uniform buffer to map (w, h) coordinates to [-1, 1]
            let transformation_matrix = [
                2.0 / win_w, 0.0, 0.0, 0.0,
                0.0, -2.0 / win_h, 0.0, 0.0,
                0.0, 0.0, 0.5, 0.0,
                -1.0, 1.0, 0.5, 1.0,
            ];
            let src_buffer = buffer_from_slice(
                device,
                wgpu::BufferUsage::COPY_SRC,
                to_u8_slice(&transformation_matrix[..])
            );
            encoder.copy_buffer_to_buffer(&src_buffer, 0, &self.transform_buffer, 0, 16 * 4);
            // Update vertex buffer
            self.vertex_buffer.upload(device, encoder, &rect_vertices);
            // Update index buffer
            self.index_buffer.upload(device, encoder, &rect_indices);
            // Draw
            {
                let mut rpass = super::render::create_default_render_pass(encoder, buffers);
                rpass.set_pipeline(&self.pipeline);
                rpass.set_bind_group(0, &self.uniforms_bind_group, &[]);
                rpass.set_vertex_buffer(0, &self.vertex_buffer.get_buffer(), 0, 0);
                rpass.set_index_buffer(&self.index_buffer.get_buffer(), 0, 0);
                rpass.draw_indexed(0..(self.index_buffer.len() as u32), 0, 0..1);
            }
        }

        // Resolve !
        super::render::encode_resolve_render_pass(encoder, buffers);

        // Draw text
        // TODO: use depth buffer
        self.glyph_brush
            .draw_queued(
                device,
                encoder,
                buffers.texture_buffer,
                data.physical_window_size.width,
                data.physical_window_size.height,
            )
            .expect("couldn't draw queued glyphs");
    }
}

#[derive(Debug, Clone, Copy)]
struct UiVertex {
    position: [f32; 3],
    color: [f32; 4],
}

const UI_VERTEX_ATTRIBUTES: [wgpu::VertexAttributeDescriptor; 2] = [
    wgpu::VertexAttributeDescriptor {
        shader_location: 0,
        format: wgpu::VertexFormat::Float3,
        offset: 0,
    },
    wgpu::VertexAttributeDescriptor {
        shader_location: 1,
        format: wgpu::VertexFormat::Float4,
        offset: 12,
    },
];

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use winit::{dpi::PhysicalSize, window::Window};

use crate::editor::EditorBuffer;

pub const FONT_SIZE: f32 = 16.0;
pub const LINE_HEIGHT: f32 = 22.0;
pub const CHAR_WIDTH: f32 = 9.6;
const PAD_LEFT: f32 = 8.0;
const PAD_TOP: f32 = 8.0;

const QUAD_SHADER: &str = r#"
struct Vtx { @location(0) pos: vec2<f32>, @location(1) col: vec4<f32> }
struct Out { @builtin(position) pos: vec4<f32>, @location(0) col: vec4<f32> }
@vertex fn vs(v: Vtx) -> Out { return Out(vec4<f32>(v.pos, 0.0, 1.0), v.col); }
@fragment fn fs(o: Out) -> @location(0) vec4<f32> { return o.col; }
"#;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct QuadVtx {
    pos: [f32; 2],
    col: [f32; 4],
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: PhysicalSize<u32>,

    font_system: FontSystem,
    swash_cache: SwashCache,
    cache: Cache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,

    quad_pipeline: wgpu::RenderPipeline,
    quad_buf: wgpu::Buffer,
    quad_vtx_count: u32,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::None,
                force_fallback_adapter: false,
            })
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas = TextAtlas::new(&device, &queue, &cache, format);
        let text_renderer =
            TextRenderer::new(&mut atlas, &device, wgpu::MultisampleState::default(), None);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad"),
            source: wgpu::ShaderSource::Wgsl(QUAD_SHADER.into()),
        });
        let quad_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<QuadVtx>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let quad_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad_vb"),
            size: (std::mem::size_of::<QuadVtx>() * 6 * 64) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let _ = &mut font_system; // used below
        Self {
            surface,
            device,
            queue,
            config,
            size,
            font_system,
            swash_cache,
            cache,
            viewport,
            atlas,
            text_renderer,
            quad_pipeline,
            quad_buf,
            quad_vtx_count: 0,
        }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self, editor: &EditorBuffer) {
        let surface_tex = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(st)
            | wgpu::CurrentSurfaceTexture::Suboptimal(st) => st,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = surface_tex
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let w = self.size.width;
        let h = self.size.height;

        self.viewport
            .update(&self.queue, Resolution { width: w, height: h });

        // Cursor block quad (タブは4列分として計算)
        let visual_col = visual_col(&editor.lines[editor.cursor.line], editor.cursor.col);
        let cx = PAD_LEFT + visual_col as f32 * CHAR_WIDTH;
        let cy = PAD_TOP + editor.cursor.line as f32 * LINE_HEIGHT - editor.scroll_y;
        let verts = quad_vertices(cx, cy, CHAR_WIDTH, LINE_HEIGHT, [0.3, 0.6, 1.0, 0.45], w as f32, h as f32);
        self.queue
            .write_buffer(&self.quad_buf, 0, bytemuck::cast_slice(&verts));
        self.quad_vtx_count = verts.len() as u32;

        // Text buffer (recreated each frame for simplicity)
        let content = editor.lines.join("\n");
        let mut text_buf =
            Buffer::new(&mut self.font_system, Metrics::new(FONT_SIZE, LINE_HEIGHT));
        text_buf.set_size(
            &mut self.font_system,
            Some(w as f32 - PAD_LEFT),
            Some((editor.lines.len() as f32 + 1.0) * LINE_HEIGHT),
        );
        text_buf.set_text(
            &mut self.font_system,
            &content,
            &Attrs::new().family(Family::Monospace),
            Shaping::Advanced,
            None,
        );
        text_buf.shape_until_scroll(&mut self.font_system, false);

        let text_top = PAD_TOP - editor.scroll_y;
        {
            let font_system = &mut self.font_system;
            let swash_cache = &mut self.swash_cache;
            let atlas = &mut self.atlas;
            let viewport = &self.viewport;
            let device = &self.device;
            let queue = &self.queue;
            self.text_renderer
                .prepare(
                    device,
                    queue,
                    font_system,
                    atlas,
                    viewport,
                    [TextArea {
                        buffer: &text_buf,
                        left: PAD_LEFT,
                        top: text_top,
                        scale: 1.0,
                        bounds: TextBounds {
                            left: 0,
                            top: 0,
                            right: w as i32,
                            bottom: h as i32,
                        },
                        default_color: Color::rgb(220, 220, 220),
                        custom_glyphs: &[],
                    }],
                    swash_cache,
                )
                .unwrap();
        }

        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.12,
                            g: 0.12,
                            b: 0.14,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            rp.set_pipeline(&self.quad_pipeline);
            rp.set_vertex_buffer(0, self.quad_buf.slice(..));
            rp.draw(0..self.quad_vtx_count, 0..1);

            self.text_renderer
                .render(&self.atlas, &self.viewport, &mut rp)
                .unwrap();
        }

        self.queue.submit([encoder.finish()]);
        surface_tex.present();
        self.atlas.trim();
    }
}

/// タブを4列とした視覚的な列位置を返す
pub fn visual_col(line: &str, char_col: usize) -> usize {
    let mut vcol = 0;
    for (i, ch) in line.chars().enumerate() {
        if i == char_col {
            break;
        }
        if ch == '\t' {
            vcol = (vcol / 4 + 1) * 4;
        } else {
            vcol += 1;
        }
    }
    vcol
}

fn quad_vertices(
    x: f32, y: f32, w: f32, h: f32,
    col: [f32; 4],
    sw: f32, sh: f32,
) -> [QuadVtx; 6] {
    let l = x / sw * 2.0 - 1.0;
    let r = (x + w) / sw * 2.0 - 1.0;
    let t = 1.0 - y / sh * 2.0;
    let b = 1.0 - (y + h) / sh * 2.0;
    [
        QuadVtx { pos: [l, t], col },
        QuadVtx { pos: [r, t], col },
        QuadVtx { pos: [l, b], col },
        QuadVtx { pos: [r, t], col },
        QuadVtx { pos: [r, b], col },
        QuadVtx { pos: [l, b], col },
    ]
}

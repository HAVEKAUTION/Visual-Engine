//! wgpu renderer: draws full-screen WGSL scenes with feedback buffers.
//!
//! Every scene pass is a full-screen triangle with one bind group:
//!
//! | binding | WGSL name    | what                                        |
//! |---------|--------------|---------------------------------------------|
//! | 0       | `audio`      | all signals (see `scenes::uniforms`)        |
//! | 1       | `params`     | the scene's inputs and clocks               |
//! | 2       | `smp`        | linear, clamp-to-edge sampler               |
//! | 3       | `prev_frame` | the scene's previous output (feedback)      |
//! | 4, 5    | `buf_a/b`    | off-screen buffers (ping-pong when persistent) |
//!
//! Scenes render into 16-bit float textures in display (sRGB-like) space, the
//! way Shadertoy-style shaders expect. A final pass converts to linear and
//! writes an 8-bit sRGB "display" texture that the UI shows.

use std::num::NonZeroU64;

use scenes::{CompiledScene, Target, VERTEX_ENTRY, meta::MAX_BUFFERS};

/// Format of scene outputs and buffers.
pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Format of the final display texture.
pub const DISPLAY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Largest render size per side (keeps 4K+ windows safe).
pub const MAX_SIZE: u32 = 4096;

const PRESENT_WGSL: &str = r#"
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;

struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VsOut {
    let c = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VsOut;
    o.pos = vec4<f32>(c * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(c.x, 1.0 - c.y);
    return o;
}

// Scenes write display-space values; the sRGB target re-encodes on write,
// so decode here to show exactly what the scene wrote.
fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let c = clamp(textureSampleLevel(src, smp, in.uv, 0.0).rgb, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(to_linear(c), 1.0);
}
"#;

struct Tex {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl Tex {
    fn new(
        device: &wgpu::Device,
        label: &str,
        size: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { texture, view }
    }
}

/// Render targets at one size. New textures start out black.
struct Targets {
    size: (u32, u32),
    /// Scene output, ping-pong: `output[out_cur]` is the latest frame.
    output: [Tex; 2],
    out_cur: usize,
    /// Buffers A and B, ping-pong each.
    bufs: [[Tex; 2]; MAX_BUFFERS],
    buf_cur: [usize; MAX_BUFFERS],
    display: Tex,
}

impl Targets {
    fn new(device: &wgpu::Device, size: (u32, u32)) -> Self {
        let t = |label: &str| Tex::new(device, label, size, SCENE_FORMAT);
        Self {
            size,
            output: [t("output 0"), t("output 1")],
            out_cur: 0,
            bufs: [[t("buf_a 0"), t("buf_a 1")], [t("buf_b 0"), t("buf_b 1")]],
            buf_cur: [0; MAX_BUFFERS],
            display: Tex::new(device, "display", size, DISPLAY_FORMAT),
        }
    }
}

/// A compiled scene's GPU pipelines.
pub struct ScenePipeline {
    passes: Vec<(wgpu::RenderPipeline, Target)>,
    persistent: [bool; MAX_BUFFERS],
    params_buf: wgpu::Buffer,
    pub name: String,
}

/// The renderer. Owns its own copies of the wgpu device and queue handles.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    black: Tex,
    signal_buf: wgpu::Buffer,
    present_pipeline: wgpu::RenderPipeline,
    present_layout: wgpu::BindGroupLayout,
    targets: Option<Targets>,
    /// Bumped whenever the display texture is recreated.
    generation: u64,
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

fn fullscreen_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    vs: &str,
    fs: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene"),
            entries: &[
                uniform_entry(0),
                uniform_entry(1),
                sampler_entry(2),
                texture_entry(3),
                texture_entry(4),
                texture_entry(5),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let black = Tex::new(device, "black", (1, 1), SCENE_FORMAT);
        let signal_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("signals"),
            size: (scenes::uniforms::BLOCK_FLOATS * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let present_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("present"),
            entries: &[texture_entry(0), sampler_entry(1)],
        });
        let present_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("present"),
            bind_group_layouts: &[Some(&present_layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("present"),
            source: wgpu::ShaderSource::Wgsl(PRESENT_WGSL.into()),
        });
        let present_pipeline = fullscreen_pipeline(
            device,
            "present",
            &present_pl,
            &module,
            "vs",
            "fs",
            DISPLAY_FORMAT,
        );

        // Clear the 1×1 black texture (wgpu zero-fills it anyway; be explicit).
        let mut enc = device.create_command_encoder(&Default::default());
        clear_pass(&mut enc, &black.view);
        queue.submit([enc.finish()]);

        Self {
            device: device.clone(),
            queue: queue.clone(),
            layout,
            pipeline_layout,
            sampler,
            black,
            signal_buf,
            present_pipeline,
            present_layout,
            targets: None,
            generation: 0,
        }
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Build the GPU pipelines for a scene that passed [`scenes::compile`].
    /// Errors are returned as text, never panics, so a broken edit can't take
    /// the app down.
    pub fn create_scene(&self, scene: &CompiledScene) -> Result<ScenePipeline, String> {
        let device = &self.device;
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&scene.meta.name),
            source: wgpu::ShaderSource::Wgsl(scene.wgsl.as_str().into()),
        });
        let mut passes = Vec::new();
        for pass in &scene.meta.passes {
            let target = scene.meta.pass_target(pass)?;
            let pipeline = fullscreen_pipeline(
                device,
                &pass.entry,
                &self.pipeline_layout,
                &module,
                VERTEX_ENTRY,
                &pass.entry,
                SCENE_FORMAT,
            );
            passes.push((pipeline, target));
        }
        let params_len = scenes::params::ParamLayout::new(&scene.meta).len;
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: (params_len * 4) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(format!("GPU rejected the scene: {e}"));
        }
        Ok(ScenePipeline {
            passes,
            persistent: std::array::from_fn(|i| scene.meta.buffer_persistent(i)),
            params_buf,
            name: scene.meta.name.clone(),
        })
    }

    /// Forget all feedback (next frame starts from black).
    pub fn clear_feedback(&mut self) {
        self.targets = None;
    }

    /// The texture to show, if a frame has been rendered.
    pub fn display_view(&self) -> Option<&wgpu::TextureView> {
        self.targets.as_ref().map(|t| &t.display.view)
    }

    /// Changes whenever [`Self::display_view`] is a new texture (after a resize
    /// or [`Self::clear_feedback`]).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn size(&self) -> Option<(u32, u32)> {
        self.targets.as_ref().map(|t| t.size)
    }

    /// Render one frame of `scene` at `size` pixels.
    pub fn render(
        &mut self,
        scene: &ScenePipeline,
        signals: &[u8],
        params: &[f32],
        size: (u32, u32),
    ) {
        let size = (size.0.clamp(1, MAX_SIZE), size.1.clamp(1, MAX_SIZE));
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            self.targets = Some(Targets::new(&self.device, size));
            self.generation += 1;
        }
        let Some(t) = self.targets.as_mut() else {
            return;
        };

        self.queue.write_buffer(&self.signal_buf, 0, signals);
        let params_bytes: Vec<u8> = params.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let n = (params_bytes.len() as u64).min(scene.params_buf.size());
        self.queue
            .write_buffer(&scene.params_buf, 0, &params_bytes[..n as usize]);

        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene"),
            });

        // What each buffer reads as right now: last frame's contents if
        // persistent, black if not. Updated as passes write them.
        let mut latest: [Option<usize>; MAX_BUFFERS] =
            std::array::from_fn(|i| scene.persistent[i].then_some(t.buf_cur[i]));
        let prev_out = t.out_cur;
        let next_out = 1 - t.out_cur;

        for (pipeline, target) in &scene.passes {
            let buf_view = |i: usize| match latest[i] {
                Some(k) => &t.bufs[i][k].view,
                None => &self.black.view,
            };
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.signal_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &scene.params_buf,
                            offset: 0,
                            size: NonZeroU64::new(scene.params_buf.size()),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&t.output[prev_out].view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(buf_view(0)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(buf_view(1)),
                    },
                ],
            });
            // Write into the copy that isn't being read.
            let (dest, written) = match target {
                Target::Output => (&t.output[next_out].view, None),
                Target::Buffer(i) => {
                    let k = 1 - latest[*i].unwrap_or(t.buf_cur[*i]);
                    (&t.bufs[*i][k].view, Some((*i, k)))
                }
            };
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("scene pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: dest,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.draw(0..3, 0..1);
            }
            if let Some((i, k)) = written {
                latest[i] = Some(k);
                t.buf_cur[i] = k;
            }
        }
        t.out_cur = next_out;

        // Present: scene output → display texture.
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("present"),
            layout: &self.present_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&t.output[next_out].view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.display.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.present_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([enc.finish()]);
    }

    /// Copy the display texture back to the CPU as tightly packed RGBA8
    /// (sRGB). Blocks until the GPU is done; for tests and screenshots.
    pub fn read_display(&self) -> Option<(u32, u32, Vec<u8>)> {
        let t = self.targets.as_ref()?;
        let (w, h) = t.size;
        let row = w * 4;
        let padded =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &t.display.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        rx.recv().ok()?.ok()?;
        let data = buf.slice(..).get_mapped_range().ok()?;
        let mut out = Vec::with_capacity((row * h) as usize);
        for y in 0..h as usize {
            let start = y * padded as usize;
            out.extend_from_slice(&data[start..start + row as usize]);
        }
        Some((w, h, out))
    }
}

fn clear_pass(enc: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
}

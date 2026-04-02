use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}

const QUAD_VERTICES: &[Vertex] = &[
    Vertex { position: [-1.0, -1.0], tex_coords: [0.0, 1.0] },
    Vertex { position: [ 1.0, -1.0], tex_coords: [1.0, 1.0] },
    Vertex { position: [ 1.0,  1.0], tex_coords: [1.0, 0.0] },
    Vertex { position: [-1.0,  1.0], tex_coords: [0.0, 0.0] },
];

const QUAD_INDICES: &[u16] = &[0, 1, 2, 2, 3, 0];

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum FrameFormat {
    Yuv420p,
    Nv12,
    Rgba,
}

/// Color space for YUV→RGB matrix selection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorSpace {
    Bt601,
    Bt709,
    Bt2020,
}

/// Color range
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorRange {
    Limited, // TV: Y 16-235, UV 16-240
    Full,    // PC: 0-255
}

pub struct RawFrame {
    pub format: FrameFormat,
    pub width: u32,
    pub height: u32,
    pub planes: Vec<PlaneData>,
    pub pts_secs: f64,
    pub color_space: ColorSpace,
    pub color_range: ColorRange,
}

pub struct PlaneData {
    pub data: Vec<u8>,
    #[allow(dead_code)]
    pub stride: usize,
    pub width: u32,
    pub height: u32,
}

/// GPU-side color params (matches shader struct, 64 bytes)
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuColorParams {
    row0: [f32; 4],
    row1: [f32; 4],
    row2: [f32; 4],
    range: [f32; 4],
}

impl GpuColorParams {
    fn from_frame(cs: ColorSpace, cr: ColorRange) -> Self {
        // YUV→RGB matrix coefficients
        let (kr, kb) = match cs {
            ColorSpace::Bt601  => (0.299_f32,  0.114_f32),
            ColorSpace::Bt709  => (0.2126_f32, 0.0722_f32),
            ColorSpace::Bt2020 => (0.2627_f32, 0.0593_f32),
        };
        let kg = 1.0 - kr - kb;

        // Matrix: Y'CbCr → R'G'B' (after range normalization)
        // R = Y + (2-2*Kr)*V
        // G = Y - (2*Kb*(1-Kb)/Kg)*U - (2*Kr*(1-Kr)/Kg)*V
        // B = Y + (2-2*Kb)*U
        let rv = 2.0 * (1.0 - kr);
        let gu = -2.0 * kb * (1.0 - kb) / kg;
        let gv = -2.0 * kr * (1.0 - kr) / kg;
        let bu = 2.0 * (1.0 - kb);

        // Range normalization parameters
        let (y_off, uv_off, y_scale, uv_scale) = match cr {
            ColorRange::Limited => {
                // Y: 16..235 → 0..1, UV: 16..240 → -0.5..0.5
                (16.0 / 255.0, 128.0 / 255.0, 255.0 / (235.0 - 16.0), 255.0 / (240.0 - 16.0))
            }
            ColorRange::Full => {
                (0.0, 128.0 / 255.0, 1.0, 1.0)
            }
        };

        Self {
            row0: [1.0, 0.0, rv, 0.0],
            row1: [1.0, gu, gv, 0.0],
            row2: [1.0, bu, 0.0, 0.0],
            range: [y_off, uv_off, y_scale, uv_scale],
        }
    }
}

pub struct VideoRenderer {
    pipeline_yuv: wgpu::RenderPipeline,
    pipeline_nv12: wgpu::RenderPipeline,
    pipeline_rgba: wgpu::RenderPipeline,

    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    layout_yuv: wgpu::BindGroupLayout,
    layout_nv12: wgpu::BindGroupLayout,
    layout_rgba: wgpu::BindGroupLayout,

    sampler: wgpu::Sampler,
    color_buffer: Option<wgpu::Buffer>,

    bind_group: Option<wgpu::BindGroup>,
    current_format: Option<FrameFormat>,
    texture_size: (u32, u32),
    textures: Vec<wgpu::Texture>,
    texture_views: Vec<wgpu::TextureView>,
}

impl VideoRenderer {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("video_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("video_vb"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("video_ib"),
            contents: bytemuck::cast_slice(QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
        };

        let tex_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                multisampled: false,
                view_dimension: wgpu::TextureViewDimension::D2,
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
            },
            count: None,
        };
        let sampler_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let uniform_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        // YUV: Y(0) U(1) V(2) sampler(3) uniform(4)
        let layout_yuv = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("yuv_layout"),
            entries: &[tex_entry(0), tex_entry(1), tex_entry(2), sampler_entry(3), uniform_entry(4)],
        });
        // NV12: Y(0) UV(1) sampler(2) uniform(3)
        let layout_nv12 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nv12_layout"),
            entries: &[tex_entry(0), tex_entry(1), sampler_entry(2), uniform_entry(3)],
        });
        let layout_rgba = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rgba_layout"),
            entries: &[tex_entry(0), sampler_entry(1)],
        });

        let make_pipeline = |layout: &wgpu::BindGroupLayout, fs_entry: &str, label: &str| {
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[layout],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[vertex_layout.clone()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs_entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };

        Self {
            pipeline_yuv: make_pipeline(&layout_yuv, "fs_yuv", "yuv_pipeline"),
            pipeline_nv12: make_pipeline(&layout_nv12, "fs_nv12", "nv12_pipeline"),
            pipeline_rgba: make_pipeline(&layout_rgba, "fs_rgba", "rgba_pipeline"),
            vertex_buffer,
            index_buffer,
            layout_yuv, layout_nv12, layout_rgba,
            sampler,
            color_buffer: None,
            bind_group: None,
            current_format: None,
            texture_size: (0, 0),
            textures: Vec::new(),
            texture_views: Vec::new(),
        }
    }

    fn create_tex(device: &wgpu::Device, w: u32, h: u32, fmt: wgpu::TextureFormat, label: &str) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: fmt,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    /// Fast texture upload — data is already tightly packed by decoder
    fn write_plane_fast(queue: &wgpu::Queue, texture: &wgpu::Texture, plane: &PlaneData, bpp: u32) {
        let row_bytes = plane.width * bpp;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture, mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &plane.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_bytes),
                rows_per_image: Some(plane.height),
            },
            wgpu::Extent3d { width: plane.width, height: plane.height, depth_or_array_layers: 1 },
        );
    }

    /// Rebuild textures + views + bind_group only when format/size changes
    fn ensure_resources(&mut self, device: &wgpu::Device, frame: &RawFrame) {
        let size_ok = self.texture_size == (frame.width, frame.height);
        let fmt_ok = self.current_format == Some(frame.format);
        if size_ok && fmt_ok && self.bind_group.is_some() {
            return; // Reuse existing
        }

        self.current_format = Some(frame.format);
        self.texture_size = (frame.width, frame.height);
        self.textures.clear();
        self.texture_views.clear();

        let (w, h) = (frame.width, frame.height);
        let cw = w / 2;
        let ch = h / 2;

        match frame.format {
            FrameFormat::Yuv420p => {
                self.textures.push(Self::create_tex(device, w, h, wgpu::TextureFormat::R8Unorm, "Y"));
                self.textures.push(Self::create_tex(device, cw, ch, wgpu::TextureFormat::R8Unorm, "U"));
                self.textures.push(Self::create_tex(device, cw, ch, wgpu::TextureFormat::R8Unorm, "V"));
            }
            FrameFormat::Nv12 => {
                self.textures.push(Self::create_tex(device, w, h, wgpu::TextureFormat::R8Unorm, "Y_nv12"));
                self.textures.push(Self::create_tex(device, cw, ch, wgpu::TextureFormat::Rg8Unorm, "UV_nv12"));
            }
            FrameFormat::Rgba => {
                self.textures.push(Self::create_tex(device, w, h, wgpu::TextureFormat::Rgba8UnormSrgb, "rgba"));
            }
        }

        self.texture_views = self.textures.iter()
            .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
            .collect();

        self.rebuild_bind_group(device, frame);
    }

    fn rebuild_bind_group(&mut self, device: &wgpu::Device, frame: &RawFrame) {
        let views = &self.texture_views;
        let params = GpuColorParams::from_frame(frame.color_space, frame.color_range);

        // Create or update uniform buffer
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("color_params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        self.color_buffer = Some(buf);
        let color_buf = self.color_buffer.as_ref().unwrap();

        self.bind_group = Some(match frame.format {
            FrameFormat::Yuv420p => device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("yuv_bg"), layout: &self.layout_yuv,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&views[0]) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[1]) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&views[2]) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 4, resource: color_buf.as_entire_binding() },
                ],
            }),
            FrameFormat::Nv12 => device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("nv12_bg"), layout: &self.layout_nv12,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&views[0]) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[1]) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: color_buf.as_entire_binding() },
                ],
            }),
            FrameFormat::Rgba => device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("rgba_bg"), layout: &self.layout_rgba,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&views[0]) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            }),
        });
    }

    pub fn upload_frame(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &RawFrame) {
        self.ensure_resources(device, frame);

        match frame.format {
            FrameFormat::Yuv420p => {
                Self::write_plane_fast(queue, &self.textures[0], &frame.planes[0], 1);
                Self::write_plane_fast(queue, &self.textures[1], &frame.planes[1], 1);
                Self::write_plane_fast(queue, &self.textures[2], &frame.planes[2], 1);
            }
            FrameFormat::Nv12 => {
                Self::write_plane_fast(queue, &self.textures[0], &frame.planes[0], 1);
                Self::write_plane_fast(queue, &self.textures[1], &frame.planes[1], 2);
            }
            FrameFormat::Rgba => {
                Self::write_plane_fast(queue, &self.textures[0], &frame.planes[0], 4);
            }
        }
        // Update color params uniform
        if let Some(ref buf) = self.color_buffer {
            let params = GpuColorParams::from_frame(frame.color_space, frame.color_range);
            queue.write_buffer(buf, 0, bytemuck::bytes_of(&params));
        }
    }

    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if let (Some(ref bind_group), Some(fmt)) = (&self.bind_group, self.current_format) {
            let pipeline = match fmt {
                FrameFormat::Yuv420p => &self.pipeline_yuv,
                FrameFormat::Nv12 => &self.pipeline_nv12,
                FrameFormat::Rgba => &self.pipeline_rgba,
            };
            render_pass.set_pipeline(pipeline);
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..6, 0, 0..1);
        }
    }
}

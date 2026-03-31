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

pub struct RawFrame {
    pub format: FrameFormat,
    pub width: u32,
    pub height: u32,
    pub planes: Vec<PlaneData>,
    pub pts_secs: f64,
}

pub struct PlaneData {
    pub data: Vec<u8>,
    #[allow(dead_code)]
    pub stride: usize,
    pub width: u32,
    pub height: u32,
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

    // Cached state — only rebuild when format/size changes
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

        let layout_yuv = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("yuv_layout"),
            entries: &[tex_entry(0), tex_entry(1), tex_entry(2), sampler_entry(3)],
        });
        let layout_nv12 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nv12_layout"),
            entries: &[tex_entry(0), tex_entry(1), sampler_entry(2)],
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

        self.rebuild_bind_group(device, frame.format);
    }

    fn rebuild_bind_group(&mut self, device: &wgpu::Device, format: FrameFormat) {
        let views = &self.texture_views;
        self.bind_group = Some(match format {
            FrameFormat::Yuv420p => device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("yuv_bg"), layout: &self.layout_yuv,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&views[0]) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[1]) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&views[2]) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            }),
            FrameFormat::Nv12 => device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("nv12_bg"), layout: &self.layout_nv12,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&views[0]) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[1]) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.sampler) },
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
        // bind_group already points to these textures — no rebuild needed
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

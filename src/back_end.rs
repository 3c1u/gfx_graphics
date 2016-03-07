use graphics::{ Context, DrawState, Graphics, Viewport };
use graphics::BACK_END_MAX_VERTEX_COUNT as BUFFER_SIZE;
use graphics::draw_state;
use graphics::color::gamma_srgb_to_linear;
use { gfx, Texture };
use gfx::format::{DepthStencil, Srgb8};
use gfx::pso::PipelineState;
use shader_version::{ OpenGL, Shaders };
use shader_version::glsl::GLSL;

const POS_COMPONENTS: usize = 2;
const UV_COMPONENTS: usize = 2;

gfx_vertex_struct!( PositionFormat {
    pos: [f32; 2] = "pos",
});

gfx_vertex_struct!( ColorFormat {
    color: [f32; 4] = "color",
});

gfx_vertex_struct!( TexCoordsFormat {
    uv: [f32; 2] = "uv",
});

gfx_pipeline_base!( pipe_colored {
    pos: gfx::VertexBuffer<PositionFormat>,
    color: gfx::Global<[f32; 4]>,
    blend_target: gfx::BlendTarget<gfx::format::Srgb8>,
    stencil_target: gfx::StencilTarget<gfx::format::DepthStencil>,
    blend_ref: gfx::BlendRef,
    scissor: gfx::Scissor,
});

gfx_pipeline_base!( pipe_textured {
    pos: gfx::VertexBuffer<PositionFormat>,
    uv: gfx::VertexBuffer<TexCoordsFormat>,
    color: gfx::Global<[f32; 4]>,
    texture: gfx::TextureSampler<[f32; 4]>,
    blend_target: gfx::BlendTarget<gfx::format::Srgb8>,
    stencil_target: gfx::StencilTarget<gfx::format::DepthStencil>,
    blend_ref: gfx::BlendRef,
    scissor: gfx::Scissor,
});

// Stores one PSO per blend setting.
struct PsoBlend<T> {
    alpha: T,
    add: T,
    multiply: T,
    invert: T,
    none: T,
}

impl<T> PsoBlend<T> {
    fn blend(&mut self, blend: Option<draw_state::Blend>) -> &mut T {
        use graphics::draw_state::Blend;

        match blend {
            Some(Blend::Alpha) => &mut self.alpha,
            Some(Blend::Add) => &mut self.add,
            Some(Blend::Multiply) => &mut self.multiply,
            Some(Blend::Invert) => &mut self.invert,
            None => &mut self.none,
        }
    }
}

// Stores one `PsoBlend` per clip setting.
struct PsoStencil<T> {
    none: PsoBlend<T>,
    clip: PsoBlend<T>,
    inside: PsoBlend<T>,
    outside: PsoBlend<T>,
}

impl<T> PsoStencil<T> {
    fn new<Fact, F>(factory: &mut Fact, f: F) -> PsoStencil<T>
        where F: Fn(
            &mut Fact,
            gfx::state::Blend,
            gfx::state::Stencil,
            gfx::state::ColorMask
        ) -> T
    {
        use gfx::state::{Blend, BlendChannel, Comparison, Equation, Factor,
            Stencil, StencilOp};
        use gfx::preset::blend;

        let stencil = Stencil::new(Comparison::Always, 0,
            (StencilOp::Keep, StencilOp::Keep, StencilOp::Keep));
        let stencil_clip = Stencil::new(Comparison::Never, 255,
            (StencilOp::Replace, StencilOp::Keep, StencilOp::Keep));
        let stencil_inside = Stencil::new(Comparison::Equal, 255,
            (StencilOp::Keep, StencilOp::Keep, StencilOp::Keep));
        let stencil_outside = Stencil::new(Comparison::NotEqual, 255,
            (StencilOp::Keep, StencilOp::Keep, StencilOp::Keep));

        // Channel color masks.
        let mask_all = gfx::state::MASK_ALL;
        let mask_none = gfx::state::MASK_NONE;

        // Fake disabled blending using the same pipeline.
        let no_blend = Blend {
            color: BlendChannel {
                equation: Equation::Add,
                source: Factor::One,
                destination: Factor::Zero,
            },
            alpha: BlendChannel {
                equation: Equation::Add,
                source: Factor::One,
                destination: Factor::Zero,
            },
        };

        PsoStencil {
            none: PsoBlend {
                alpha: f(factory, blend::ALPHA, stencil, mask_all),
                add: f(factory, blend::ADD, stencil, mask_all),
                multiply: f(factory, blend::MULTIPLY, stencil, mask_all),
                invert: f(factory, blend::INVERT, stencil, mask_all),
                none: f(factory, no_blend, stencil, mask_all),
            },
            clip: PsoBlend {
                alpha: f(factory, blend::ALPHA, stencil_clip, mask_none),
                add: f(factory, blend::ADD, stencil_clip, mask_none),
                multiply: f(factory, blend::MULTIPLY, stencil_clip, mask_none),
                invert: f(factory, blend::INVERT, stencil_clip, mask_none),
                none: f(factory, no_blend, stencil_clip, mask_none),
            },
            inside: PsoBlend {
                alpha: f(factory, blend::ALPHA, stencil_inside, mask_all),
                add: f(factory, blend::ADD, stencil_inside, mask_all),
                multiply: f(factory, blend::MULTIPLY, stencil_inside, mask_all),
                invert: f(factory, blend::INVERT, stencil_inside, mask_all),
                none: f(factory, no_blend, stencil_inside, mask_all),
            },
            outside: PsoBlend {
                alpha: f(factory, blend::ALPHA, stencil_outside, mask_all),
                add: f(factory, blend::ADD, stencil_outside, mask_all),
                multiply: f(factory, blend::MULTIPLY, stencil_outside, mask_all),
                invert: f(factory, blend::INVERT, stencil_outside, mask_all),
                none: f(factory, no_blend, stencil_outside, mask_all),
            }
        }
    }

    // Returns a PSO and stencil reference given a stencil and blend setting.
    fn stencil_blend(
        &mut self,
        stencil: Option<draw_state::Stencil>,
        blend: Option<draw_state::Blend>
    ) -> (&mut T, u8) {
        use graphics::draw_state::Stencil;

        match stencil {
            None => (self.none.blend(blend), 0),
            Some(Stencil::Clip(val)) => (self.clip.blend(blend), val),
            Some(Stencil::Inside(val)) => (self.inside.blend(blend), val),
            Some(Stencil::Outside(val)) => (self.outside.blend(blend), val),
        }
    }
}

/// The data used for drawing 2D graphics.
///
/// Stores buffers and PSO objects needed for rendering 2D graphics.
pub struct Gfx2d<R: gfx::Resources> {
    buffer_pos: gfx::handle::Buffer<R, PositionFormat>,
    buffer_uv: gfx::handle::Buffer<R, TexCoordsFormat>,
    colored: PsoStencil<PipelineState<R, pipe_colored::Meta>>,
    textured: PsoStencil<PipelineState<R, pipe_textured::Meta>>,
    sampler: gfx::handle::Sampler<R>,
}

impl<R: gfx::Resources> Gfx2d<R> {
    /// Creates a new Gfx2d object.
    pub fn new<F>(opengl: OpenGL, factory: &mut F) -> Self
        where F: gfx::Factory<R>
    {
        use gfx::Primitive;
        use gfx::state::Rasterizer;
        use gfx::state::{Blend, Stencil};
        use gfx::traits::*;
        use shaders::{ colored, textured };

        let glsl = opengl.to_glsl();

        let colored_program = factory.link_program(
                Shaders::new()
                    .set(GLSL::V1_20, colored::VERTEX_GLSL_120)
                    .set(GLSL::V1_50, colored::VERTEX_GLSL_150_CORE)
                    .get(glsl).unwrap(),
                Shaders::new()
                    .set(GLSL::V1_20, colored::FRAGMENT_GLSL_120)
                    .set(GLSL::V1_50, colored::FRAGMENT_GLSL_150_CORE)
                    .get(glsl).unwrap(),
            ).unwrap();

        let colored_pipeline = |factory: &mut F,
                                blend_preset: Blend,
                                stencil: Stencil,
                                color_mask: gfx::state::ColorMask|
        -> PipelineState<R, pipe_colored::Meta> {
            factory.create_pipeline_from_program(
                &colored_program,
                Primitive::TriangleList,
                Rasterizer::new_fill(gfx::state::CullFace::Nothing),
                pipe_colored::Init {
                    pos: (),
                    color: "color",
                    blend_target: ("o_Color", color_mask, blend_preset),
                    stencil_target: stencil,
                    blend_ref: (),
                    scissor: (),
                }
            ).unwrap()
        };

        let colored = PsoStencil::new(factory, colored_pipeline);

        let textured_program = factory.link_program(
                Shaders::new()
                    .set(GLSL::V1_20, textured::VERTEX_GLSL_120)
                    .set(GLSL::V1_50, textured::VERTEX_GLSL_150_CORE)
                    .get(glsl).unwrap(),
                Shaders::new()
                    .set(GLSL::V1_20, textured::FRAGMENT_GLSL_120)
                    .set(GLSL::V1_50, textured::FRAGMENT_GLSL_150_CORE)
                    .get(glsl).unwrap()
            ).unwrap();

        let textured_pipeline = |factory: &mut F,
                                 blend_preset: Blend,
                                 stencil: Stencil,
                                 color_mask: gfx::state::ColorMask|
        -> PipelineState<R, pipe_textured::Meta> {
            factory.create_pipeline_from_program(
                &textured_program,
                Primitive::TriangleList,
                Rasterizer::new_fill(gfx::state::CullFace::Nothing),
                pipe_textured::Init {
                    pos: (),
                    uv: (),
                    color: "color",
                    texture: "s_texture",
                    blend_target: ("o_Color", color_mask, blend_preset),
                    stencil_target: stencil,
                    blend_ref: (),
                    scissor: (),
                }
            ).unwrap()
        };

        let textured = PsoStencil::new(factory, textured_pipeline);

        let buffer_pos = factory.create_buffer_dynamic(
            POS_COMPONENTS * BUFFER_SIZE,
            gfx::BufferRole::Vertex
        );
        let buffer_uv = factory.create_buffer_dynamic(
            UV_COMPONENTS * BUFFER_SIZE,
            gfx::BufferRole::Vertex
        );

        let sampler_info = gfx::tex::SamplerInfo::new(
            gfx::tex::FilterMethod::Bilinear,
            gfx::tex::WrapMode::Clamp
        );
        let sampler = factory.create_sampler(sampler_info);

        Gfx2d {
            buffer_pos: buffer_pos,
            buffer_uv: buffer_uv,
            colored: colored,
            textured: textured,
            sampler: sampler
        }
    }

    /// Renders graphics to a Gfx renderer.
    pub fn draw<C, F>(
        &mut self,
        encoder: &mut gfx::Encoder<R, C>,
        output_color: &gfx::handle::RenderTargetView<R, Srgb8>,
        output_stencil: &gfx::handle::DepthStencilView<R, DepthStencil>,
        viewport: Viewport,
        f: F
    )
        where C: gfx::CommandBuffer<R>,
              F: FnOnce(Context, &mut GfxGraphics<R, C>)
    {
        let ref mut g = GfxGraphics::new(
            encoder,
            output_color,
            output_stencil,
            self
        );
        let c = Context::new_viewport(viewport);
        f(c, g);
    }
}

/// Used for rendering 2D graphics.
pub struct GfxGraphics<'a, R, C>
    where R: gfx::Resources + 'a,
          C: gfx::CommandBuffer<R> + 'a,
          R::Buffer: 'a,
          R::Shader: 'a,
          R::Program: 'a,
          R::Texture: 'a,
          R::Sampler: 'a
{
    encoder: &'a mut gfx::Encoder<R, C>,
    output_color: &'a gfx::handle::RenderTargetView<R, Srgb8>,
    output_stencil: &'a gfx::handle::DepthStencilView<R, DepthStencil>,
    g2d: &'a mut Gfx2d<R>,
}

impl<'a, R, C> GfxGraphics<'a, R, C>
    where R: gfx::Resources,
          C: gfx::CommandBuffer<R>,
{
    /// Creates a new object for rendering 2D graphics.
    pub fn new(encoder: &'a mut gfx::Encoder<R, C>,
               output_color: &'a gfx::handle::RenderTargetView<R, Srgb8>,
               output_stencil: &'a gfx::handle::DepthStencilView<R, DepthStencil>,
               g2d: &'a mut Gfx2d<R>) -> Self {
        GfxGraphics {
            encoder: encoder,
            output_color: output_color,
            output_stencil: output_stencil,
            g2d: g2d,
        }
    }

    /// Returns true if texture has alpha channel.
    pub fn has_texture_alpha(&self, texture: &Texture<R>) -> bool {
        use gfx::format::SurfaceType::*;

        match texture.surface.get_info().format {
            R4_G4_B4_A4
            | R5_G5_B5_A1
            | R8_G8_B8_A8
            | R10_G10_B10_A2
            | R16_G16_B16_A16
            | R32_G32_B32_A32 => true,
            R3_G3_B2
            | R4_G4
            | R5_G6_B5
            | R8 | R8_G8 | R8_G8_B8
            | R11_G11_B10
            | R16 | R16_G16 | R16_G16_B16
            | R32 | R32_G32 | R32_G32_B32
            | D16 | D24 | D24_S8 | D32 => false,
        }
    }
}

impl<'a, R, C> Graphics for GfxGraphics<'a, R, C>
    where R: gfx::Resources,
          C: gfx::CommandBuffer<R>,
          R::Buffer: 'a,
          R::Shader: 'a,
          R::Program: 'a,
          R::Texture: 'a,
          R::Sampler: 'a
{
    type Texture = Texture<R>;

    fn clear_color(&mut self, color: [f32; 4]) {
        let color = gamma_srgb_to_linear(color);
        let &mut GfxGraphics {
            ref mut encoder,
            output_color,
            ..
        } = self;
        encoder.clear(output_color, [color[0], color[1], color[2]]);
    }

    fn clear_stencil(&mut self, value: u8) {
        let &mut GfxGraphics {
            ref mut encoder,
            output_stencil,
            ..
        } = self;
        encoder.clear_stencil(output_stencil, value);
    }

    fn tri_list<F>(
        &mut self,
        draw_state: &DrawState,
        color: &[f32; 4],
        mut f: F
    )
        where F: FnMut(&mut FnMut(&[f32]))
    {
        use gfx::core::target::Rect;
        use std::u16;

        let color = gamma_srgb_to_linear(*color);
        let &mut GfxGraphics {
            ref mut encoder,
            output_color,
            output_stencil,
            g2d: &mut Gfx2d {
                ref mut buffer_pos,
                ref mut colored,
                ..
            },
            ..
        } = self;

        let (pso_colored, stencil_val) = colored.stencil_blend(
            draw_state.stencil,
            draw_state.blend
        );

        let scissor = match draw_state.scissor {
            None => Rect { x: 0, y: 0, w: u16::MAX, h: u16::MAX },
            Some(r) => Rect { x: r[0] as u16, y: r[1] as u16,
                w: r[2] as u16, h: r[3] as u16 }
        };

        let data = pipe_colored::Data {
            pos: buffer_pos.clone(),
            color: color,
            blend_target: output_color.clone(),
            stencil_target: (output_stencil.clone(),
                             (stencil_val, stencil_val)),
            // Use white color for blend reference to make invert work.
            blend_ref: [1.0; 4],
            scissor: scissor,
        };

        f(&mut |vertices: &[f32]| {
            use std::mem::transmute;

            unsafe {
                encoder.update_buffer(&buffer_pos, transmute(vertices), 0)
                    .unwrap();
            }

            let n = vertices.len() / POS_COMPONENTS;
            let slice = gfx::Slice {
                    instances: None,
                    start: 0,
                    end: n as u32,
                    kind: gfx::SliceKind::Vertex
            };
            encoder.draw(&slice, pso_colored, &data);
        })
    }

    fn tri_list_uv<F>(
        &mut self,
        draw_state: &DrawState,
        color: &[f32; 4],
        texture: &<Self as Graphics>::Texture,
        mut f: F
    )
        where F: FnMut(&mut FnMut(&[f32], &[f32]))
    {
        use gfx::core::target::Rect;
        use std::u16;

        let color = gamma_srgb_to_linear(*color);
        let &mut GfxGraphics {
            ref mut encoder,
            output_color,
            output_stencil,
            g2d: &mut Gfx2d {
                ref mut buffer_pos,
                ref mut buffer_uv,
                ref mut textured,
                ref sampler,
                ..
            },
            ..
        } = self;

        let (pso_textured, stencil_val) = textured.stencil_blend(
            draw_state.stencil,
            draw_state.blend
        );

        let scissor = match draw_state.scissor {
            None => Rect { x: 0, y: 0, w: u16::MAX, h: u16::MAX },
            Some(r) => Rect { x: r[0] as u16, y: r[1] as u16,
                w: r[2] as u16, h: r[3] as u16 }
        };

        let data = pipe_textured::Data {
            pos: buffer_pos.clone(),
            uv: buffer_uv.clone(),
            color: color,
            texture: (texture.view.clone(), sampler.clone()),
            blend_target: output_color.clone(),
            stencil_target: (output_stencil.clone(),
                             (stencil_val, stencil_val)),
            blend_ref: [1.0; 4],
            scissor: scissor,
        };

        f(&mut |vertices: &[f32], texture_coords: &[f32]| {
            use std::mem::transmute;

            assert_eq!(
                vertices.len() * UV_COMPONENTS,
                texture_coords.len() * POS_COMPONENTS
            );
            unsafe {
                encoder.update_buffer(&buffer_pos, transmute(vertices), 0)
                    .unwrap();
                encoder.update_buffer(&buffer_uv, transmute(texture_coords), 0)
                    .unwrap();
            }

            let n = vertices.len() / POS_COMPONENTS;
            let slice = gfx::Slice {
                    instances: None,
                    start: 0,
                    end: n as u32,
                    kind: gfx::SliceKind::Vertex
            };
            encoder.draw(&slice, pso_textured, &data);
        })
    }
}

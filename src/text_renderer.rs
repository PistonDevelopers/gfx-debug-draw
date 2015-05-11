use std::default::Default;
use std::marker::PhantomData;
use std::mem;

use gfx;
use gfx::traits::*;

use bitmap_font::BitmapFont;
use utils::{grow_buffer, MAT4_ID};

pub struct TextRenderer<R: gfx::Resources> {
    program: gfx::handle::Program<R>,
    state: gfx::DrawState,
    bitmap_font: BitmapFont,
    vertex_data: Vec<Vertex>,
    index_data: Vec<u32>,
    vertex_buffer: gfx::handle::Buffer<R, Vertex>,
    index_buffer: gfx::handle::IndexBuffer<R, u32>,
    params: TextShaderParams<R>,
}

impl<R: gfx::Resources> TextRenderer<R> {

    pub fn new<F: gfx::Factory<R>> (
        device_capabilities: gfx::device::Capabilities,
        factory: &mut F,
        frame_size: [u32; 2],
        initial_buffer_size: usize,
        bitmap_font: BitmapFont,
        font_texture: gfx::handle::Texture<R>,
    ) -> Result<TextRenderer<R>, gfx::ProgramError> {

        let vertex = gfx::ShaderSource {
            glsl_120: Some(VERTEX_SRC[0]),
            glsl_150: Some(VERTEX_SRC[1]),
            .. gfx::ShaderSource::empty()
        };

        let fragment = gfx::ShaderSource {
            glsl_120: Some(FRAGMENT_SRC[0]),
            glsl_150: Some(FRAGMENT_SRC[1]),
            .. gfx::ShaderSource::empty()
        };

        let program = match factory.link_program_source(
            vertex, fragment, &device_capabilities
        ) {
            Ok(program_handle) => program_handle,
            Err(e) => return Err(e),
        };

        let vertex_buffer = factory.create_buffer::<Vertex>(initial_buffer_size, gfx::BufferUsage::Dynamic);
        let index_buffer = gfx::handle::IndexBuffer::from_raw(
            factory.create_buffer_raw(initial_buffer_size * mem::size_of::<u32>(), gfx::BufferUsage::Dynamic)
        );

        let sampler = factory.create_sampler(
           gfx::tex::SamplerInfo::new(
               gfx::tex::FilterMethod::Scale,
               gfx::tex::WrapMode::Clamp
            )
        );

        let state = gfx::DrawState::new().blend(gfx::BlendPreset::Alpha);

        Ok(TextRenderer {
            vertex_data: Vec::new(),
            index_data: Vec::new(),
            bitmap_font: bitmap_font,
            program: program,
            state: state,
            vertex_buffer: vertex_buffer,
            index_buffer: index_buffer,
            params: TextShaderParams {
                model_view_proj: MAT4_ID,
                screen_size: [frame_size[0] as f32, frame_size[1] as f32],
                tex_font: (font_texture, Some(sampler)),
                _r: PhantomData,
            },
        })
    }

    pub fn draw_text_at_position(
        &mut self,
        text: &str,
        world_position: [f32; 3],
        color: [f32; 4],
    ) {
        self.draw_text(text, [0, 0], world_position, 0, color);
    }

    pub fn draw_text_on_screen(
        &mut self,
        text: &str,
        screen_position: [i32; 2],
        color: [f32; 4],
    ) {
        self.draw_text(text, screen_position, [0.0, 0.0, 0.0], 1, color);
    }

    fn draw_text(
        &mut self,
        text: &str,
        screen_position: [i32; 2],
        world_position: [f32; 3],
        screen_relative: i32,
        color: [f32; 4],
    ) {
        let mut x = screen_position[0];
        let y = screen_position[1];

        let scale_w = self.bitmap_font.scale_w as f32;
        let scale_h = self.bitmap_font.scale_h as f32;

        // placeholder for characters missing from font
        let default_character = Default::default();

        for character in text.chars() {

            let bc = match self.bitmap_font.characters.get(&character) {
                Some(c) => c,
                None => &default_character,
            };

            // Push quad vertices in CCW direction
            let index = self.vertex_data.len();

            let x_offset = (bc.xoffset as i32 + x) as f32;
            let y_offset = (bc.yoffset as i32 + y) as f32;


            // 0 - top left
            self.vertex_data.push(Vertex {
                position: [
                    x_offset,
                    y_offset,
                ],
                color: color,
                texcoords: [
                    bc.x as f32 / scale_w,
                    bc.y as f32 / scale_h,
                ],
                world_position: world_position,
                screen_relative: screen_relative,
            });

            // 1 - bottom left
            self.vertex_data.push(Vertex{
                position: [
                    x_offset,
                    bc.height as f32 + y_offset
                ],
                color: color,
                texcoords: [
                    bc.x as f32 / scale_w,
                    (bc.y + bc.height) as f32 / scale_h,
                ],
                world_position: world_position,
                screen_relative: screen_relative,
            });

            // 2 - bottom right
            self.vertex_data.push(Vertex{
                position: [
                    bc.width as f32 + x_offset,
                    bc.height as f32 + y_offset,
                ],
                color: color,
                texcoords: [
                    (bc.x + bc.width) as f32 / scale_w,
                    (bc.y + bc.height) as f32 / scale_h,
                ],
                world_position: world_position,
                screen_relative: screen_relative,
            });


            // 3 - top right
            self.vertex_data.push(Vertex{
                position: [
                    bc.width as f32 + x_offset,
                    y_offset,
                ],
                color: color,
                texcoords: [
                    (bc.x + bc.width) as f32 / scale_w,
                    bc.y as f32 / scale_h,
                ],
                world_position: world_position,
                screen_relative: screen_relative,
            });


            // Top-left triangle
            self.index_data.push((index + 0) as u32);
            self.index_data.push((index + 1) as u32);
            self.index_data.push((index + 3) as u32);

            // Bottom-right triangle
            self.index_data.push((index + 3) as u32);
            self.index_data.push((index + 1) as u32);
            self.index_data.push((index + 2) as u32);

            x += bc.xadvance as i32;
        }
    }

    ///
    /// Draw and clear the current batch of text.
    ///
    pub fn render<
        C: gfx::CommandBuffer<R>,
        F: gfx::Factory<R>,
        O: gfx::render::target::Output<R>,
    > (
        &mut self,
        renderer: &mut gfx::Renderer<R, C>,
        factory: &mut F,
        output: &O,
        projection: [[f32; 4]; 4],
    ) {

        if self.vertex_data.len() > self.vertex_buffer.len() {
            self.vertex_buffer = gfx::handle::Buffer::from_raw(
                grow_buffer::<R, F, Vertex>(factory, self.vertex_buffer.raw(), self.vertex_data.len())
            );
        }

        if self.index_data.len() > self.index_buffer.len() {
            self.index_buffer = gfx::handle::IndexBuffer::from_raw(
                grow_buffer::<R, F, u32>(factory, self.index_buffer.raw(), self.index_data.len())
            );
        }

        factory.update_buffer(&self.vertex_buffer, &self.vertex_data[..], 0);
        factory.update_buffer_raw(&self.index_buffer.raw(), gfx::as_byte_slice(&self.index_data[..]), 0);

        self.params.screen_size = {
            let (w, h) = output.get_size();
            [w as f32, h as f32]
        };
        self.params.model_view_proj = projection;

        let mesh = gfx::Mesh::from_format(
            self.vertex_buffer.clone(),
            self.vertex_data.len() as gfx::VertexCount
        );

        let slice = gfx::Slice {
            start: 0,
            end: self.index_data.len() as u32,
            prim_type: gfx::PrimitiveType::TriangleList,
            kind: gfx::SliceKind::Index32(self.index_buffer.clone(), 0),
        };

        renderer.draw(
            &gfx::batch::bind(&self.state, &mesh, slice, &self.program, &self.params),
            output
        ).unwrap();

        self.vertex_data.clear();
        self.index_data.clear();
    }
}

static VERTEX_SRC: [&'static [u8]; 2] = [
b"
    #version 120

    uniform vec2 u_screen_size;
    uniform mat4 u_model_view_proj;
    uniform sampler2D u_tex_font;

    attribute vec2 at_position;
    attribute vec4 at_world_position;
    attribute int at_screen_relative;
    attribute vec4 at_color;
    attribute vec2 at_texcoords;
    varying vec4 v_color;
    varying vec2 v_TexCoord;

    void main() {

        // on-screen offset from text origin
        vec2 screen_offset = vec2(
            2 * at_position.x / u_screen_size.x - 1,
            1 - 2 * at_position.y / u_screen_size.y
        );

        vec4 screen_position = u_model_view_proj * at_world_position;

        // perspective divide to get normalized device coords
        vec2 world_offset = vec2(
            screen_position.x / screen_position.z + 1,
            screen_position.y / screen_position.z - 1
        );

        // on-screen offset accounting for world_position
        world_offset = at_screen_relative == 0 ? world_offset : vec2(0.0, 0.0);

        gl_Position = vec4(world_offset + screen_offset, 0, 1.0);

        v_TexCoord = at_texcoords;
        v_color = at_color;

    }
",
b"
    #version 150 core

    uniform vec2 u_screen_size;
    uniform mat4 u_model_view_proj;

    in vec2 at_position;
    in vec4 at_world_position;
    in int at_screen_relative;
    in vec4 at_color;
    in vec2 at_texcoords;
    out vec4 v_color;
    out vec2 v_TexCoord;

    void main() {

        // on-screen offset from text origin
        vec2 screen_offset = vec2(
            2 * at_position.x / u_screen_size.x - 1,
            1 - 2 * at_position.y / u_screen_size.y
        );

        vec4 screen_position = u_model_view_proj * at_world_position;

        // perspective divide to get normalized device coords
        vec2 world_offset = vec2(
            screen_position.x / screen_position.z + 1,
            screen_position.y / screen_position.z - 1
        );

        // on-screen offset accounting for world_position
        world_offset = at_screen_relative == 0 ? world_offset : vec2(0.0, 0.0);

        gl_Position = vec4(world_offset + screen_offset, 0, 1.0);

        v_TexCoord = at_texcoords;
        v_color = at_color;

    }
"];

static FRAGMENT_SRC: [&'static [u8]; 2] = [
b"
    #version 120

    uniform sampler2D u_tex_font;

    varying vec4 v_color;
    varying vec2 v_TexCoord;

    void main() {
        vec4 font_color = texture2D(u_tex_font, v_TexCoord);
        gl_FragColor = vec4(v_color.xyz, font_color.a * v_color.a);
    }
",
b"
    #version 150 core

    uniform sampler2D u_tex_font;

    in vec4 v_color;
    in vec2 v_TexCoord;
    out vec4 out_color;

    void main() {
        vec4 font_color = texture(u_tex_font, v_TexCoord);
        out_color = vec4(v_color.xyz, font_color.a * v_color.a);
    }
"];

gfx_vertex!( Vertex {
    at_position@ position: [f32; 2],
    at_texcoords@ texcoords: [f32; 2],
    at_world_position@ world_position: [f32; 3],
    at_screen_relative@ screen_relative: i32,
    at_color@ color: [f32; 4],
});

gfx_parameters!( TextShaderParams/Link {
    u_model_view_proj@ model_view_proj: [[f32; 4]; 4],
    u_screen_size@ screen_size: [f32; 2],
    u_tex_font@ tex_font: gfx::shade::TextureParam<R>,
});

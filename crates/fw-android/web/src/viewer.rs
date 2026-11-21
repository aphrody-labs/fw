//! The WebGPU viewer: decodes fwa-stream v1 from a WebSocket (or from the built-in demo compositor), keeps one texture
//! per guest window, composites all windows into one canvas with one textured quad each and forwards pointer, key and
//! resize events back.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use fw_android_stream::Message;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    BinaryType, CanvasRenderingContext2d, CloseEvent, Event, HtmlCanvasElement, KeyboardEvent, MessageEvent, PointerEvent,
    WebSocket,
};

use crate::demo::Demo;
use crate::layout::{self, Mode, Placement, Region};
use crate::{keymap, palette};

const TITLE_HEIGHT: f32 = 32.0;
const GAP: f32 = 12.0;
const RESIZE_QUIET_MS: f64 = 250.0;
const SAMPLES: usize = 240;
const DEFAULT_WEBSOCKET: &str = "ws://127.0.0.1:7650/";

const QUAD_BYTES: u64 = 48;

pub struct Options {
    pub websocket: String,
    pub demo: bool,
    pub mode: Mode,
    pub resize: bool,
    pub hud: bool,
}

impl Options {
    fn from_location() -> Result<Self, JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let search = window.location().search()?;
        let params = web_sys::UrlSearchParams::new_with_str(&search)?;
        let flag = |name: &str, default: bool| params.get(name).map_or(default, |value| value != "0" && value != "false");
        Ok(Self {
            websocket: params.get("ws").unwrap_or_else(|| DEFAULT_WEBSOCKET.to_string()),
            demo: flag("demo", false),
            mode: params.get("layout").and_then(|value| Mode::parse(&value)).unwrap_or(Mode::Tile),
            resize: flag("resize", true),
            hud: flag("hud", false),
        })
    }
}

fn quad_bytes(rect: [f32; 4], uv: [f32; 4], tint: [f32; 4]) -> [u8; QUAD_BYTES as usize] {
    let mut bytes = [0u8; QUAD_BYTES as usize];
    for (slot, value) in bytes.chunks_exact_mut(4).zip(rect.into_iter().chain(uv).chain(tint)) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

struct Quad {
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
    white: wgpu::TextureView,
    max_dimension: u32,
    adapter_info: String,
}

impl Gpu {
    fn quad(&self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) -> Quad {
        let uniform = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad"),
            size: QUAD_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("quad"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        });
        Quad { uniform, bind_group }
    }

    fn place(&self, quad: &Quad, rect: &layout::Rect, uv: [f32; 4], tint: [f32; 4]) {
        let (width, height) = (self.config.width as f32, self.config.height as f32);
        let clip = [
            rect.x / width * 2.0 - 1.0,
            1.0 - rect.y / height * 2.0,
            (rect.x + rect.width) / width * 2.0 - 1.0,
            1.0 - (rect.y + rect.height) / height * 2.0,
        ];
        self.queue.write_buffer(&quad.uniform, 0, &quad_bytes(clip, uv, tint));
    }

    fn texture(&self, width: u32, height: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("window"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }
}

struct Window {
    id: u32,
    slot: u32,
    app_id: String,
    title: String,
    size: (u32, u32),
    texture: wgpu::Texture,
    content: Quad,
    title_texture: wgpu::Texture,
    title_quad: Quad,
    ring: Quad,
    /// What the title texture currently shows: (width in pixels, focused, title).
    title_drawn: Option<(u32, bool, String)>,
    requested: Option<(u32, u32)>,
}

enum Transport {
    Demo(Demo),
    Socket(WebSocket),
    Closed,
}

#[derive(Default)]
struct Counters {
    frames: u64,
    messages: u64,
    bytes: u64,
    frame_messages: u64,
    damage_bytes: u64,
    pointer_sent: u64,
    key_sent: u64,
    resize_sent: u64,
    decode_errors: u64,
    dropped_frames: u64,
}

pub struct App {
    gpu: Gpu,
    options: Options,
    canvas: HtmlCanvasElement,
    scratch: HtmlCanvasElement,
    windows: Vec<Window>,
    next_slot: u32,
    focus: Option<u32>,
    capture: Option<u32>,
    closing: Option<u32>,
    transport: Transport,
    placements: Vec<Placement>,
    pixel_ratio: f32,
    last_canvas: (u32, u32),
    resize_due: Option<f64>,
    deltas: VecDeque<f64>,
    cpu: VecDeque<f64>,
    last_frame: f64,
    counters: Counters,
    status: String,
    last_hud: f64,
    reconnect_at: Option<f64>,
}

fn now() -> f64 {
    web_sys::window().and_then(|window| window.performance()).map_or(0.0, |performance| performance.now())
}

fn push_sample(samples: &mut VecDeque<f64>, value: f64) {
    if samples.len() == SAMPLES {
        samples.pop_front();
    }
    samples.push_back(value);
}

fn summary(samples: &VecDeque<f64>) -> String {
    if samples.is_empty() {
        return "null".into();
    }
    let mut sorted: Vec<f64> = samples.iter().copied().collect();
    sorted.sort_by(f64::total_cmp);
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let pick = |fraction: f64| sorted[((sorted.len() - 1) as f64 * fraction) as usize];
    format!(
        "{{\"n\":{},\"mean\":{:.3},\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}}",
        sorted.len(),
        mean,
        pick(0.5),
        pick(0.95),
        pick(1.0)
    )
}

fn css_color(hex: u32) -> String {
    format!("#{hex:06x}")
}

fn linear_tint(hex: u32) -> [f32; 4] {
    [(hex >> 16 & 0xff) as f32 / 255.0, (hex >> 8 & 0xff) as f32 / 255.0, (hex & 0xff) as f32 / 255.0, 1.0]
}

impl App {
    fn metrics(&self) -> layout::Metrics {
        layout::Metrics { title_height: (TITLE_HEIGHT * self.pixel_ratio).round(), gap: (GAP * self.pixel_ratio).round() }
    }

    fn set_status(&mut self, text: &str) {
        self.status = text.to_string();
    }

    fn open_window(&mut self, id: u32, app_id: String, title: String, width: u32, height: u32) {
        self.windows.retain(|window| window.id != id);
        let limit = self.gpu.max_dimension;
        let (width, height) = (width.clamp(1, limit), height.clamp(1, limit));
        let texture = self.gpu.texture(
            width,
            height,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        let title_width = limit.min(4096);
        let title_texture = self.gpu.texture(
            title_width,
            (TITLE_HEIGHT * 4.0) as u32,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let content = self.gpu.quad(&texture.create_view(&Default::default()), &self.gpu.linear);
        let title_quad = self.gpu.quad(&title_texture.create_view(&Default::default()), &self.gpu.nearest);
        let ring = self.gpu.quad(&self.gpu.white, &self.gpu.nearest);
        let slot = self.next_slot;
        self.next_slot += 1;
        self.windows.push(Window {
            id,
            slot,
            app_id,
            title,
            size: (width, height),
            texture,
            content,
            title_texture,
            title_quad,
            ring,
            title_drawn: None,
            requested: None,
        });
        self.focus = Some(id);
        self.resize_due = Some(now() + RESIZE_QUIET_MS);
    }

    /// Server to client message.
    pub fn handle(&mut self, message: Message) {
        self.counters.messages += 1;
        match message {
            Message::WindowOpen { window, app_id, title, width, height } => self.open_window(window, app_id, title, width, height),
            Message::WindowTitle { window, title } => {
                if let Some(target) = self.windows.iter_mut().find(|candidate| candidate.id == window) {
                    target.title = title;
                }
            }
            Message::WindowSize { window, width, height } => {
                let limit = self.gpu.max_dimension;
                let (width, height) = (width.clamp(1, limit), height.clamp(1, limit));
                let Some(index) = self.windows.iter().position(|candidate| candidate.id == window) else { return };
                let texture = self.gpu.texture(
                    width,
                    height,
                    wgpu::TextureFormat::Bgra8Unorm,
                    wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                );
                let content = self.gpu.quad(&texture.create_view(&Default::default()), &self.gpu.linear);
                let target = &mut self.windows[index];
                target.texture = texture;
                target.content = content;
                target.size = (width, height);
            }
            Message::Frame { window, x, y, width, height, pixels } => {
                let Some(target) = self.windows.iter().find(|candidate| candidate.id == window) else {
                    self.counters.dropped_frames += 1;
                    return;
                };
                if width == 0 || height == 0 || x.saturating_add(width) > target.size.0 || y.saturating_add(height) > target.size.1 {
                    self.counters.dropped_frames += 1;
                    return;
                }
                self.counters.frame_messages += 1;
                self.counters.damage_bytes += pixels.len() as u64;
                self.gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &target.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x, y, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &pixels,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) },
                    wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                );
            }
            Message::WindowClose { window } => {
                self.windows.retain(|candidate| candidate.id != window);
                if self.focus == Some(window) {
                    self.focus = self.windows.last().map(|candidate| candidate.id);
                }
                if self.capture == Some(window) {
                    self.capture = None;
                }
                self.resize_due = Some(now() + RESIZE_QUIET_MS);
            }
            Message::Pointer { .. } | Message::Key { .. } | Message::ResizeRequest { .. } | Message::CloseRequest { .. } => {}
        }
    }

    /// Client to server message.
    fn send(&mut self, message: Message) {
        match &message {
            Message::Pointer { .. } => self.counters.pointer_sent += 1,
            Message::Key { .. } => self.counters.key_sent += 1,
            Message::ResizeRequest { .. } => self.counters.resize_sent += 1,
            _ => {}
        }
        match &mut self.transport {
            Transport::Demo(demo) => {
                for reply in demo.input(&message) {
                    self.handle(reply);
                }
            }
            Transport::Socket(socket) => {
                if socket.ready_state() == WebSocket::OPEN {
                    if let Err(error) = socket.send_with_u8_array(&message.encode()) {
                        web_sys::console::error_2(&"fwa: websocket send failed".into(), &error);
                    }
                }
            }
            Transport::Closed => {}
        }
    }

    fn receive_bytes(&mut self, bytes: &[u8]) {
        self.counters.bytes += bytes.len() as u64;
        let mut offset = 0;
        while offset < bytes.len() {
            match Message::decode(&bytes[offset..]) {
                Ok((message, used)) => {
                    offset += used;
                    self.handle(message);
                }
                Err(error) => {
                    self.counters.decode_errors += 1;
                    web_sys::console::error_1(&format!("fwa: {error}").into());
                    return;
                }
            }
        }
    }

    fn sync_canvas(&mut self) {
        let ratio = web_sys::window().map_or(1.0, |window| window.device_pixel_ratio() as f32).max(1.0);
        let width = ((self.canvas.client_width() as f32 * ratio).round() as u32).clamp(1, self.gpu.max_dimension);
        let height = ((self.canvas.client_height() as f32 * ratio).round() as u32).clamp(1, self.gpu.max_dimension);
        if (width, height) != self.last_canvas || ratio != self.pixel_ratio {
            self.pixel_ratio = ratio;
            self.last_canvas = (width, height);
            self.canvas.set_width(width);
            self.canvas.set_height(height);
            self.gpu.config.width = width;
            self.gpu.config.height = height;
            self.gpu.surface.configure(&self.gpu.device, &self.gpu.config);
            self.resize_due = Some(now() + RESIZE_QUIET_MS);
        }
    }

    fn layout(&mut self) {
        let metrics = self.metrics();
        let mut windows: Vec<_> = self.windows.iter().map(|window| (window.slot, window.id, window.size)).collect();
        windows.sort_by_key(|(slot, _, _)| *slot);
        let sizes: Vec<_> = windows.iter().map(|(_, id, size)| (*id, *size)).collect();
        let mut placements = layout::place(self.options.mode, (self.last_canvas.0 as f32, self.last_canvas.1 as f32), &sizes, &metrics);
        if let Some(focus) = self.focus {
            placements.sort_by_key(|placement| placement.window == focus);
        }
        self.placements = placements;
    }

    fn request_fit(&mut self, time: f64) {
        let Some(due) = self.resize_due else { return };
        if time < due {
            return;
        }
        self.resize_due = None;
        if !self.options.resize {
            return;
        }
        let wanted: Vec<_> = self
            .placements
            .iter()
            .map(|placement| (placement.window, (placement.available.0.floor() as u32 & !1, placement.available.1.floor() as u32 & !1)))
            .collect();
        for (id, (width, height)) in wanted {
            let Some(window) = self.windows.iter_mut().find(|candidate| candidate.id == id) else { continue };
            if width < 96 || height < 96 || window.requested == Some((width, height)) || window.size == (width, height) {
                continue;
            }
            window.requested = Some((width, height));
            self.send(Message::ResizeRequest { window: id, width, height });
        }
    }

    fn draw_title(&self, window: &Window, width: u32, focused: bool) -> Result<(), JsValue> {
        let height = (TITLE_HEIGHT * self.pixel_ratio).round() as u32;
        self.scratch.set_width(width);
        self.scratch.set_height(height);
        let context = self
            .scratch
            .get_context("2d")?
            .ok_or("no 2d context")?
            .dyn_into::<CanvasRenderingContext2d>()?;
        let ratio = self.pixel_ratio as f64;
        context.set_fill_style_str(&css_color(if focused { palette::SURFACE_CONTAINER_HIGHEST } else { palette::SURFACE_CONTAINER_HIGH }));
        context.fill_rect(0.0, 0.0, width as f64, height as f64);
        context.set_font(&format!("500 {}px {}", 13.0 * ratio, palette::FONT_FAMILY));
        context.set_text_baseline("middle");
        context.set_fill_style_str(&css_color(if focused { palette::ON_SURFACE } else { palette::ON_SURFACE_VARIANT }));
        let available = width as f64 - (TITLE_HEIGHT as f64 + 24.0) * ratio;
        let mut label = window.title.clone();
        while context.measure_text(&label)?.width() > available && label.chars().count() > 1 {
            label.pop();
            label.pop();
            label.push('…');
        }
        context.fill_text(&label, 14.0 * ratio, height as f64 / 2.0)?;
        context.set_text_align("center");
        context.fill_text("×", width as f64 - height as f64 / 2.0, height as f64 / 2.0)?;
        self.gpu.queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo {
                source: wgpu::ExternalImageSource::HTMLCanvasElement(self.scratch.clone()),
                origin: wgpu::Origin2d::ZERO,
                flip_y: false,
            },
            wgpu::CopyExternalImageDestInfo {
                texture: &window.title_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
                color_space: wgpu::PredefinedColorSpace::Srgb,
                premultiplied_alpha: false,
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        Ok(())
    }

    fn refresh_titles(&mut self) {
        let max_width = self.gpu.max_dimension.min(4096);
        for index in 0..self.windows.len() {
            let Some(placement) = self.placements.iter().find(|placement| placement.window == self.windows[index].id) else { continue };
            let width = (placement.title.width as u32).clamp(1, max_width);
            let focused = self.focus == Some(self.windows[index].id);
            let wanted = (width, focused, self.windows[index].title.clone());
            if self.windows[index].title_drawn.as_ref() == Some(&wanted) {
                continue;
            }
            match self.draw_title(&self.windows[index], width, focused) {
                Ok(()) => self.windows[index].title_drawn = Some(wanted),
                Err(error) => web_sys::console::error_2(&"fwa: title draw failed".into(), &error),
            }
        }
    }

    fn render(&mut self) {
        let frame = match self.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                drop(frame);
                self.gpu.surface.configure(&self.gpu.device, &self.gpu.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.gpu.surface.configure(&self.gpu.device, &self.gpu.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Validation => {
                web_sys::console::error_1(&"fwa: surface validation error".into());
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        {
            let [red, green, blue, _] = linear_tint(palette::SURFACE);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("windows"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: red as f64, g: green as f64, b: blue as f64, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.gpu.pipeline);
            let border = (self.pixel_ratio).round().max(1.0);
            for placement in &self.placements {
                let Some(window) = self.windows.iter().find(|candidate| candidate.id == placement.window) else { continue };
                let focused = self.focus == Some(window.id);
                let outline = if focused { palette::PRIMARY } else { palette::OUTLINE_VARIANT };
                let ring = layout::Rect {
                    x: placement.title.x - border,
                    y: placement.title.y - border,
                    width: placement.title.width + 2.0 * border,
                    height: placement.title.height + placement.content.height + 2.0 * border,
                };
                self.gpu.place(&window.ring, &ring, [0.0, 0.0, 1.0, 1.0], linear_tint(outline));
                let title_uv = [0.0, 0.0, placement.title.width / window.title_texture.width() as f32, placement.title.height / window.title_texture.height() as f32];
                self.gpu.place(&window.title_quad, &placement.title, title_uv, [1.0; 4]);
                self.gpu.place(&window.content, &placement.content, [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
                for quad in [&window.ring, &window.title_quad, &window.content] {
                    pass.set_bind_group(0, &quad.bind_group, &[]);
                    pass.draw(0..6, 0..1);
                }
            }
        }
        self.gpu.queue.submit([encoder.finish()]);
        self.gpu.queue.present(frame);
    }

    fn tick(&mut self, time: f64) {
        let started = now();
        if self.last_frame > 0.0 {
            push_sample(&mut self.deltas, time - self.last_frame);
        }
        self.last_frame = time;
        if let Transport::Demo(demo) = &mut self.transport {
            for message in demo.step(time) {
                self.handle(message);
            }
        }
        if let Some(at) = self.reconnect_at {
            if time >= at {
                self.reconnect_at = None;
                self.status = "reconnecting".into();
            }
        }
        self.sync_canvas();
        self.layout();
        self.request_fit(time);
        self.refresh_titles();
        self.render();
        self.counters.frames += 1;
        push_sample(&mut self.cpu, now() - started);
        if time - self.last_hud > 500.0 {
            self.last_hud = time;
            self.update_status();
        }
    }

    fn update_status(&self) {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else { return };
        let Some(element) = document.get_element_by_id("status") else { return };
        let fps = if self.deltas.is_empty() { 0.0 } else { 1000.0 / (self.deltas.iter().sum::<f64>() / self.deltas.len() as f64) };
        let text = format!("{} · {} windows · {:.1} fps", self.status, self.windows.len(), fps);
        element.set_text_content(Some(&text));
        let visible = self.options.hud || self.windows.is_empty();
        if let Some(html) = element.dyn_ref::<web_sys::HtmlElement>() {
            html.set_hidden(!visible);
        }
    }

    fn stats(&self) -> String {
        let windows: Vec<String> = self
            .windows
            .iter()
            .map(|window| {
                format!(
                    "{{\"id\":{},\"app_id\":{:?},\"title\":{:?},\"width\":{},\"height\":{}}}",
                    window.id, window.app_id, window.title, window.size.0, window.size.1
                )
            })
            .collect();
        let counters = &self.counters;
        format!(
            "{{\"adapter\":{:?},\"status\":{:?},\"canvas\":[{},{}],\"pixel_ratio\":{},\"format\":\"{:?}\",\"frames\":{},\"frame_interval_ms\":{},\"cpu_ms\":{},\"messages\":{},\"bytes\":{},\"frame_messages\":{},\"damage_bytes\":{},\"dropped_frames\":{},\"decode_errors\":{},\"pointer_sent\":{},\"key_sent\":{},\"resize_sent\":{},\"windows\":[{}]}}",
            self.gpu.adapter_info,
            self.status,
            self.last_canvas.0,
            self.last_canvas.1,
            self.pixel_ratio,
            self.gpu.config.format,
            counters.frames,
            summary(&self.deltas),
            summary(&self.cpu),
            counters.messages,
            counters.bytes,
            counters.frame_messages,
            counters.damage_bytes,
            counters.dropped_frames,
            counters.decode_errors,
            counters.pointer_sent,
            counters.key_sent,
            counters.resize_sent,
            windows.join(",")
        )
    }

    fn device_point(&self, event: &PointerEvent) -> (f32, f32) {
        (event.offset_x() as f32 * self.pixel_ratio, event.offset_y() as f32 * self.pixel_ratio)
    }

    fn pointer(&mut self, kind: &str, event: &PointerEvent) {
        let (x, y) = self.device_point(event);
        let buttons = (event.buttons() & 7) as u8;
        let metrics = self.metrics();
        let hit = layout::hit_test(&self.placements, x, y, &metrics);
        if kind == "down" {
            let _ = self.canvas.focus();
            if let Some(hit) = &hit {
                self.focus = Some(hit.window);
                match hit.region {
                    Region::Content => {
                        self.capture = Some(hit.window);
                        let _ = self.canvas.set_pointer_capture(event.pointer_id());
                    }
                    Region::Close => self.closing = Some(hit.window),
                    Region::Title => {}
                }
            }
        }
        let target = match (self.capture, &hit) {
            (Some(window), _) => Some(window),
            (None, Some(hit)) if hit.region == Region::Content => Some(hit.window),
            _ => None,
        };
        if let Some(window) = target {
            if let Some(placement) = self.placements.iter().find(|placement| placement.window == window) {
                let (guest_x, guest_y) = layout::to_guest(placement, x, y);
                self.send(Message::Pointer { window, x: guest_x.round() as i32, y: guest_y.round() as i32, buttons });
            }
        }
        if kind == "up" {
            if let (Some(window), Some(hit)) = (self.closing.take(), &hit) {
                if hit.window == window && hit.region == Region::Close {
                    self.send(Message::CloseRequest { window });
                }
            }
            if buttons == 0 {
                self.capture = None;
                let _ = self.canvas.release_pointer_capture(event.pointer_id());
            }
        }
    }

    fn key(&mut self, event: &KeyboardEvent, down: bool) {
        if event.repeat() {
            return;
        }
        let (Some(window), Some(code)) = (self.focus, keymap::evdev(&event.code())) else { return };
        if !matches!(event.code().as_str(), "F5" | "F11" | "F12") {
            event.prevent_default();
        }
        self.send(Message::Key { window, code, down });
    }
}

type Shared = Rc<RefCell<App>>;

#[wasm_bindgen]
pub struct Viewer {
    app: Shared,
}

#[wasm_bindgen]
impl Viewer {
    /// JSON with adapter, canvas, frame interval and cpu time percentiles, counters and the window list.
    pub fn stats(&self) -> String {
        self.app.borrow().stats()
    }

    /// Feeds raw fwa-stream bytes as if they arrived from the server.
    pub fn inject(&self, bytes: &[u8]) {
        self.app.borrow_mut().receive_bytes(bytes);
    }
}

fn listen<E: JsCast + wasm_bindgen::convert::FromWasmAbi + 'static>(
    target: &web_sys::EventTarget,
    name: &str,
    handler: impl FnMut(E) + 'static,
) -> Result<(), JsValue> {
    let closure = Closure::<dyn FnMut(E)>::new(handler);
    target.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())?;
    closure.forget();
    Ok(())
}

fn connect(app: &Shared, url: &str) -> Result<WebSocket, JsValue> {
    let socket = WebSocket::new(url)?;
    socket.set_binary_type(BinaryType::Arraybuffer);
    let target: &web_sys::EventTarget = socket.as_ref();
    {
        let app = app.clone();
        listen(target, "open", move |_: Event| app.borrow_mut().set_status("connected"))?;
    }
    {
        let app = app.clone();
        listen(target, "message", move |event: MessageEvent| {
            let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() else { return };
            let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
            app.borrow_mut().receive_bytes(&bytes);
        })?;
    }
    {
        let app = app.clone();
        let url = url.to_string();
        listen(target, "close", move |event: CloseEvent| {
            let mut app = app.borrow_mut();
            app.windows.clear();
            app.focus = None;
            app.set_status(&format!("disconnected from {url} (code {})", event.code()));
            app.transport = Transport::Closed;
            app.reconnect_at = Some(now() + 1000.0);
        })?;
    }
    Ok(socket)
}

#[wasm_bindgen]
pub async fn start() -> Result<Viewer, JsValue> {
    std::panic::set_hook(Box::new(|info| web_sys::console::error_1(&format!("fwa panic: {info}").into())));
    let options = Options::from_location()?;
    let document = web_sys::window().and_then(|window| window.document()).ok_or("no document")?;
    let canvas = document
        .get_element_by_id("fwa")
        .ok_or("canvas #fwa missing")?
        .dyn_into::<HtmlCanvasElement>()?;
    let scratch = document.create_element("canvas")?.dyn_into::<HtmlCanvasElement>()?;

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::BROWSER_WEBGPU, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone())).map_err(|error| error.to_string())?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await
        .map_err(|error| format!("no WebGPU adapter: {error}"))?;
    let info = adapter.get_info();
    let adapter_info = format!("{} | {} | {} | {:?}", info.name, info.vendor, info.driver, info.backend);
    let limits = adapter.limits();
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("fw-android-web"),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(limits.clone()),
            ..Default::default()
        })
        .await
        .map_err(|error| error.to_string())?;

    let capabilities = surface.get_capabilities(&adapter);
    let format = capabilities.formats.first().copied().ok_or("surface has no formats")?.remove_srgb_suffix();
    let ratio = web_sys::window().map_or(1.0, |window| window.device_pixel_ratio() as f32).max(1.0);
    let width = ((canvas.client_width() as f32 * ratio).round() as u32).max(1);
    let height = ((canvas.client_height() as f32 * ratio).round() as u32).max(1);
    canvas.set_width(width);
    canvas.set_height(height);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        color_space: wgpu::SurfaceColorSpace::Auto,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("quad"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("quad"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("quad"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("quad"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vertex_main"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment_main"),
            targets: &[Some(format.into())],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    let sampler = |filter: wgpu::FilterMode| {
        device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: filter, min_filter: filter, ..Default::default() })
    };
    let (linear, nearest) = (sampler(wgpu::FilterMode::Linear), sampler(wgpu::FilterMode::Nearest));
    let white = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("white"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        white.as_image_copy(),
        &[255; 4],
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    let white = white.create_view(&Default::default());

    let gpu = Gpu {
        surface,
        device,
        queue,
        config,
        pipeline,
        layout,
        linear,
        nearest,
        white,
        max_dimension: limits.max_texture_dimension_2d,
        adapter_info,
    };
    let demo = options.demo;
    let websocket = options.websocket.clone();
    let app = Rc::new(RefCell::new(App {
        gpu,
        options,
        canvas: canvas.clone(),
        scratch,
        windows: Vec::new(),
        next_slot: 0,
        focus: None,
        capture: None,
        closing: None,
        transport: Transport::Closed,
        placements: Vec::new(),
        pixel_ratio: ratio,
        last_canvas: (width, height),
        resize_due: None,
        deltas: VecDeque::new(),
        cpu: VecDeque::new(),
        last_frame: 0.0,
        counters: Counters::default(),
        status: String::new(),
        last_hud: 0.0,
        reconnect_at: None,
    }));

    if demo {
        let mut app = app.borrow_mut();
        let demo = Demo::new();
        let opening = demo.open();
        app.transport = Transport::Demo(demo);
        app.set_status("demo");
        for message in opening {
            app.handle(message);
        }
    } else {
        let socket = connect(&app, &websocket)?;
        let mut app = app.borrow_mut();
        app.transport = Transport::Socket(socket);
        app.set_status(&format!("connecting to {websocket}"));
    }

    let target: &web_sys::EventTarget = canvas.as_ref();
    for (name, kind) in [("pointerdown", "down"), ("pointermove", "move"), ("pointerup", "up")] {
        let app = app.clone();
        listen(target, name, move |event: PointerEvent| app.borrow_mut().pointer(kind, &event))?;
    }
    listen(target, "contextmenu", |event: Event| event.prevent_default())?;
    let window = web_sys::window().ok_or("no window")?;
    let window_target: &web_sys::EventTarget = window.as_ref();
    for (name, down) in [("keydown", true), ("keyup", false)] {
        let app = app.clone();
        listen(window_target, name, move |event: KeyboardEvent| app.borrow_mut().key(&event, down))?;
    }
    let _ = canvas.focus();

    let frame: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
    {
        let (app, frame_again) = (app.clone(), frame.clone());
        *frame.borrow_mut() = Some(Closure::new(move |time: f64| {
            {
                let mut app = app.borrow_mut();
                if matches!(app.transport, Transport::Closed) && app.reconnect_at.is_none() && !app.options.demo {
                    app.reconnect_at = Some(time + 1000.0);
                }
                app.tick(time);
            }
            reconnect_if_due(&app);
            if let (Some(window), Some(callback)) = (web_sys::window(), frame_again.borrow().as_ref()) {
                if let Err(error) = window.request_animation_frame(callback.as_ref().unchecked_ref()) {
                    web_sys::console::error_2(&"fwa: requestAnimationFrame failed".into(), &error);
                }
            }
        }));
    }
    if let Some(callback) = frame.borrow().as_ref() {
        window.request_animation_frame(callback.as_ref().unchecked_ref())?;
    }
    Ok(Viewer { app })
}

fn reconnect_if_due(app: &Shared) {
    let (due, url) = {
        let app = app.borrow();
        (matches!(app.transport, Transport::Closed) && app.status == "reconnecting", app.options.websocket.clone())
    };
    if !due {
        return;
    }
    match connect(app, &url) {
        Ok(socket) => {
            let mut app = app.borrow_mut();
            app.transport = Transport::Socket(socket);
            app.set_status(&format!("connecting to {url}"));
        }
        Err(error) => {
            web_sys::console::error_2(&"fwa: reconnect failed".into(), &error);
            app.borrow_mut().reconnect_at = Some(now() + 2000.0);
        }
    }
}

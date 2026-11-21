//! `?probe=1`: measures what the wgpu WebGPU backend does on the viewer's upload paths, one JSON report. Every step
//! stores its result in `window.fwaProbePartial` before the next one runs, so a panic (the point of `?probe=1&panic=1`)
//! cannot hide what was measured before it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use js_sys::{Array, Object, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlCanvasElement;

const BGRA: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

fn set(object: &Object, key: &str, value: impl Into<JsValue>) {
    if let Err(error) = Reflect::set(object, &key.into(), &value.into()) {
        web_sys::console::error_2(&"fwa probe: set failed".into(), &error);
    }
}

fn now() -> f64 {
    web_sys::window().and_then(|window| window.performance()).map_or(0.0, |performance| performance.now())
}

async fn sleep(milliseconds: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            if let Err(error) = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, milliseconds) {
                web_sys::console::error_2(&"fwa probe: setTimeout failed".into(), &error);
            }
        }
    });
    if let Err(error) = JsFuture::from(promise).await {
        web_sys::console::error_2(&"fwa probe: sleep failed".into(), &error);
    }
}

struct Context {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    bgra_storage: bool,
}

fn texture(context: &Context, label: &str, width: u32, height: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> wgpu::Texture {
    context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

async fn read_back(context: &Context, texture: &wgpu::Texture) -> Result<Vec<u8>, String> {
    let (width, height) = (texture.width(), texture.height());
    let stride = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (stride * height) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = context.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(height) },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    context.queue.submit([encoder.finish()]);
    let outcome: Arc<Mutex<Option<Result<(), wgpu::BufferAsyncError>>>> = Arc::new(Mutex::new(None));
    let slot = outcome.clone();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
        if let Ok(mut slot) = slot.lock() {
            *slot = Some(result);
        }
    });
    for _ in 0..2000 {
        if outcome.lock().is_ok_and(|outcome| outcome.is_some()) {
            break;
        }
        sleep(1).await;
    }
    let finished = outcome.lock().map_err(|error| error.to_string())?.take();
    match finished {
        Some(Ok(())) => {}
        Some(Err(error)) => return Err(format!("map_async: {error}")),
        None => return Err("map_async did not complete in 2 s".into()),
    }
    let mapped = buffer.slice(..).get_mapped_range().map_err(|error| error.to_string())?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height as usize {
        let start = row * stride as usize;
        pixels.extend_from_slice(&mapped[start..start + width as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}

async fn settle(context: &Context) {
    let done = Arc::new(AtomicBool::new(false));
    let flag = done.clone();
    context.queue.on_submitted_work_done(move || flag.store(true, Ordering::Release));
    for _ in 0..5000 {
        if done.load(Ordering::Acquire) {
            return;
        }
        sleep(1).await;
    }
}

async fn scoped<T>(context: &Context, work: impl FnOnce() -> T) -> (T, Option<String>) {
    let guard = context.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let value = work();
    let error = guard.pop().await.map(|error| error.to_string());
    (value, error)
}

fn limits_object(limits: &wgpu::Limits) -> Object {
    let object = Object::new();
    set(&object, "max_texture_dimension_2d", limits.max_texture_dimension_2d as f64);
    set(&object, "max_texture_array_layers", limits.max_texture_array_layers as f64);
    set(&object, "max_bind_groups", limits.max_bind_groups as f64);
    set(&object, "max_buffer_size", limits.max_buffer_size as f64);
    set(&object, "max_uniform_buffer_binding_size", limits.max_uniform_buffer_binding_size as f64);
    set(&object, "max_storage_buffer_binding_size", limits.max_storage_buffer_binding_size as f64);
    set(&object, "max_sampled_textures_per_shader_stage", limits.max_sampled_textures_per_shader_stage as f64);
    set(&object, "max_compute_invocations_per_workgroup", limits.max_compute_invocations_per_workgroup as f64);
    set(&object, "min_uniform_buffer_offset_alignment", limits.min_uniform_buffer_offset_alignment);
    limits_extra(&object, limits);
    object
}

fn limits_extra(object: &Object, limits: &wgpu::Limits) {
    set(object, "max_color_attachment_bytes_per_sample", limits.max_color_attachment_bytes_per_sample as f64);
}

/// Window sized rectangles written with `queue.write_texture` at non-zero origins and unaligned row strides must land
/// exactly where a CPU model of the texture says they do.
async fn partial_writes(context: &Context) -> Object {
    let report = Object::new();
    let (width, height) = (97u32, 61u32);
    let target = texture(context, "partial", width, height, BGRA, wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC);
    let mut model = vec![0u8; (width * height * 4) as usize];
    context.queue.write_texture(
        target.as_image_copy(),
        &model,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    let rectangles = [(0u32, 0u32, 97u32, 61u32), (3, 5, 1, 1), (10, 7, 33, 2), (60, 20, 37, 41), (0, 60, 97, 1), (96, 0, 1, 61), (13, 13, 7, 9)];
    for (index, (x, y, w, h)) in rectangles.into_iter().enumerate() {
        let pixels: Vec<u8> = (0..w * h * 4).map(|byte| (byte as u8).wrapping_mul(7).wrapping_add(index as u8 * 31 + 1)).collect();
        // The slice starts 12 bytes into a larger buffer to exercise `offset`.
        let mut data = vec![0xee; 12];
        data.extend_from_slice(&pixels);
        context.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
            &data,
            wgpu::TexelCopyBufferLayout { offset: 12, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        for row in 0..h {
            let destination = (((y + row) * width + x) * 4) as usize;
            let source = (row * w * 4) as usize;
            model[destination..destination + w as usize * 4].copy_from_slice(&pixels[source..source + w as usize * 4]);
        }
    }
    match read_back(context, &target).await {
        Ok(actual) => {
            set(&report, "rectangles", rectangles.len() as u32);
            set(&report, "identical_to_cpu_model", actual == model);
            set(&report, "bytes_per_row_alignment_needed", false);
        }
        Err(error) => set(&report, "error", error),
    }
    report
}

async fn upload_benchmark(context: &Context) -> Object {
    let report = Object::new();
    for (width, height) in [(64u32, 64u32), (256, 256), (1080, 1920)] {
        let target = texture(context, "upload", width, height, BGRA, wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING);
        let pixels = vec![0x55u8; (width * height * 4) as usize];
        let rounds = if width * height > 1_000_000 { 20 } else { 200 };
        let started = now();
        for _ in 0..rounds {
            context.queue.write_texture(
                target.as_image_copy(),
                &pixels,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
        }
        let call_ms = (now() - started) / rounds as f64;
        settle(context).await;
        let total_ms = (now() - started) / rounds as f64;
        let entry = Object::new();
        set(&entry, "bytes", pixels.len() as u32);
        set(&entry, "call_ms", call_ms);
        set(&entry, "until_gpu_done_ms", total_ms);
        set(&report, &format!("{width}x{height}"), entry);
    }
    report
}

fn paint_canvas(canvas: &HtmlCanvasElement, width: u32, height: u32) -> Result<(), JsValue> {
    canvas.set_width(width);
    canvas.set_height(height);
    let context = canvas.get_context("2d")?.ok_or("no 2d context")?.dyn_into::<web_sys::CanvasRenderingContext2d>()?;
    context.set_fill_style_str("#ff0000");
    context.fill_rect(0.0, 0.0, width as f64, height as f64);
    context.set_fill_style_str("#0000ff");
    context.fill_rect(0.0, 0.0, 1.0, 1.0);
    Ok(())
}

async fn external_copy(context: &Context, name: &str, source: wgpu::ExternalImageSource, width: u32, height: u32) -> Object {
    let report = Object::new();
    for format in [wgpu::TextureFormat::Rgba8Unorm, BGRA] {
        let usage = wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING;
        let target = texture(context, "external", width, height, format, usage);
        let started = now();
        let (_, error) = scoped(context, || {
            context.queue.copy_external_image_to_texture(
                &wgpu::CopyExternalImageSourceInfo { source: source.clone(), origin: wgpu::Origin2d::ZERO, flip_y: false },
                wgpu::CopyExternalImageDestInfo {
                    texture: &target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                    color_space: wgpu::PredefinedColorSpace::Srgb,
                    premultiplied_alpha: false,
                },
                wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            );
        })
        .await;
        let call_ms = now() - started;
        let entry = Object::new();
        set(&entry, "call_ms", call_ms);
        match error {
            Some(error) => set(&entry, "validation_error", error),
            None => match read_back(context, &target).await {
                Ok(pixels) if pixels.len() >= 4 => {
                    // The canvas is red with one blue pixel at the origin; the logical channels are read back
                    // in memory order, so BGRA textures show them swapped.
                    set(&entry, "first_pixel_memory_order", format!("{:?}", &pixels[0..4]));
                    let last = pixels.len() - 4;
                    set(&entry, "last_pixel_memory_order", format!("{:?}", &pixels[last..]));
                }
                Ok(_) => set(&entry, "error", "empty readback"),
                Err(error) => set(&entry, "error", error),
            },
        }
        set(&report, &format!("{format:?}"), entry);
    }
    let _ = name;
    report
}

async fn external_images(context: &Context) -> Result<Object, JsValue> {
    let report = Object::new();
    let document = web_sys::window().and_then(|window| window.document()).ok_or("no document")?;
    let (width, height) = (64u32, 48u32);
    let canvas = document.create_element("canvas")?.dyn_into::<HtmlCanvasElement>()?;
    paint_canvas(&canvas, width, height)?;
    set(&report, "html_canvas", external_copy(context, "canvas", wgpu::ExternalImageSource::HTMLCanvasElement(canvas.clone()), width, height).await);

    let offscreen = web_sys::OffscreenCanvas::new(width, height)?;
    let offscreen_context = offscreen.get_context("2d")?.ok_or("no offscreen 2d")?.dyn_into::<web_sys::OffscreenCanvasRenderingContext2d>()?;
    offscreen_context.set_fill_style_str("#ff0000");
    offscreen_context.fill_rect(0.0, 0.0, width as f64, height as f64);
    set(&report, "offscreen_canvas", external_copy(context, "offscreen", wgpu::ExternalImageSource::OffscreenCanvas(offscreen), width, height).await);

    let window = web_sys::window().ok_or("no window")?;
    let bitmap = JsFuture::from(window.create_image_bitmap_with_html_canvas_element(&canvas)?).await?.dyn_into::<web_sys::ImageBitmap>()?;
    set(&report, "image_bitmap", external_copy(context, "bitmap", wgpu::ExternalImageSource::ImageBitmap(bitmap), width, height).await);

    let init = web_sys::VideoFrameInit::new();
    init.set_timestamp(0);
    let frame = web_sys::VideoFrame::new_with_html_canvas_element_and_video_frame_init(&canvas, &init)?;
    set(&report, "video_frame", external_copy(context, "video_frame", wgpu::ExternalImageSource::VideoFrame(<web_sys::VideoFrame as Clone>::clone(&frame)), width, height).await);
    frame.close();

    let (big_width, big_height) = (1080u32, 1920u32);
    let big = document.create_element("canvas")?.dyn_into::<HtmlCanvasElement>()?;
    paint_canvas(&big, big_width, big_height)?;
    let target = texture(
        context,
        "big",
        big_width,
        big_height,
        BGRA,
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let rounds = 20;
    let started = now();
    for _ in 0..rounds {
        context.queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo { source: wgpu::ExternalImageSource::HTMLCanvasElement(big.clone()), origin: wgpu::Origin2d::ZERO, flip_y: false },
            wgpu::CopyExternalImageDestInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
                color_space: wgpu::PredefinedColorSpace::Srgb,
                premultiplied_alpha: false,
            },
            wgpu::Extent3d { width: big_width, height: big_height, depth_or_array_layers: 1 },
        );
    }
    let call_ms = (now() - started) / rounds as f64;
    settle(context).await;
    let bench = Object::new();
    set(&bench, "call_ms", call_ms);
    set(&bench, "until_gpu_done_ms", (now() - started) / rounds as f64);
    set(&report, "canvas_1080x1920_bgra_bench", bench);

    let (_, error) = scoped(context, || {
        let no_render_attachment = texture(context, "no-render-attachment", width, height, BGRA, wgpu::TextureUsages::COPY_DST);
        context.queue.copy_external_image_to_texture(
            &wgpu::CopyExternalImageSourceInfo { source: wgpu::ExternalImageSource::HTMLCanvasElement(canvas.clone()), origin: wgpu::Origin2d::ZERO, flip_y: false },
            wgpu::CopyExternalImageDestInfo {
                texture: &no_render_attachment,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
                color_space: wgpu::PredefinedColorSpace::Srgb,
                premultiplied_alpha: false,
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
    })
    .await;
    set(&report, "destination_without_render_attachment", error.unwrap_or_else(|| "accepted".into()));
    Ok(report)
}

async fn formats(context: &Context) -> Object {
    let report = Object::new();
    let features = context.adapter.get_texture_format_features(BGRA);
    set(&report, "bgra8unorm_allowed_usages", format!("{:?}", features.allowed_usages));
    set(&report, "bgra8unorm_flags", format!("{:?}", features.flags));
    set(&report, "bgra8unorm_storage_feature", context.bgra_storage);
    let (_, error) = scoped(context, || {
        texture(context, "storage", 16, 16, BGRA, wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_DST);
    })
    .await;
    set(&report, "bgra8unorm_storage_texture", error.unwrap_or_else(|| "accepted".into()));
    let (_, error) = scoped(context, || {
        texture(context, "render", 16, 16, BGRA, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
    })
    .await;
    set(&report, "bgra8unorm_render_and_sample", error.unwrap_or_else(|| "accepted".into()));
    report
}

fn surface_report(instance: &wgpu::Instance, context: &Context, document: &web_sys::Document) -> Result<Object, JsValue> {
    let report = Object::new();
    let canvas = document.create_element("canvas")?.dyn_into::<HtmlCanvasElement>()?;
    canvas.set_width(64);
    canvas.set_height(64);
    let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas)).map_err(|error| error.to_string())?;
    let capabilities = surface.get_capabilities(&context.adapter);
    set(&report, "formats", format!("{:?}", capabilities.formats));
    set(&report, "present_modes", format!("{:?}", capabilities.present_modes));
    set(&report, "alpha_modes", format!("{:?}", capabilities.alpha_modes));
    set(&report, "usages", format!("{:?}", capabilities.usages));
    Ok(report)
}

#[wasm_bindgen]
pub async fn probe() -> Result<String, JsValue> {
    std::panic::set_hook(Box::new(|info| web_sys::console::error_1(&format!("fwa probe panic: {info}").into())));
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let report = Object::new();
    let publish = |report: &Object| {
        if let Err(error) = Reflect::set(&window, &"fwaProbePartial".into(), report) {
            web_sys::console::error_2(&"fwa probe: publish failed".into(), &error);
        }
    };

    let environment = Object::new();
    set(&environment, "user_agent", window.navigator().user_agent()?);
    set(&environment, "cross_origin_isolated", Reflect::get(&window, &"crossOriginIsolated".into()).unwrap_or(JsValue::FALSE));
    set(&environment, "device_pixel_ratio", window.device_pixel_ratio());
    for name in ["SharedArrayBuffer", "VideoFrame", "createImageBitmap", "OffscreenCanvas", "VideoDecoder"] {
        set(&environment, &format!("has_{name}"), Reflect::has(&window, &name.into()).unwrap_or(false));
    }
    set(&report, "environment", environment);
    publish(&report);

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::BROWSER_WEBGPU, ..wgpu::InstanceDescriptor::new_without_display_handle() });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })
        .await
        .map_err(|error| format!("no WebGPU adapter: {error}"))?;
    let info = adapter.get_info();
    let adapter_report = Object::new();
    set(&adapter_report, "name", info.name.clone());
    set(&adapter_report, "vendor", info.vendor);
    set(&adapter_report, "device", info.device);
    set(&adapter_report, "device_type", format!("{:?}", info.device_type));
    set(&adapter_report, "driver", info.driver.clone());
    set(&adapter_report, "driver_info", info.driver_info.clone());
    set(&adapter_report, "backend", format!("{:?}", info.backend));
    set(&adapter_report, "features", format!("{:?}", adapter.features()));
    set(&adapter_report, "limits", limits_object(&adapter.limits()));
    set(&adapter_report, "downlevel_flags", format!("{:?}", adapter.get_downlevel_capabilities().flags));
    set(&report, "adapter", adapter_report);
    publish(&report);

    let bgra_storage = adapter.features().contains(wgpu::Features::BGRA8UNORM_STORAGE);
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("fw-android-web probe"),
            required_features: adapter.features() & wgpu::Features::BGRA8UNORM_STORAGE,
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        })
        .await
        .map_err(|error| error.to_string())?;
    let context = Context { adapter, device, queue, bgra_storage };

    match surface_report(&instance, &context, &document) {
        Ok(surface) => set(&report, "surface", surface),
        Err(error) => set(&report, "surface_error", error),
    }
    publish(&report);
    set(&report, "formats", formats(&context).await);
    publish(&report);
    set(&report, "partial_write_texture", partial_writes(&context).await);
    publish(&report);
    set(&report, "write_texture_bench", upload_benchmark(&context).await);
    publish(&report);
    match external_images(&context).await {
        Ok(external) => set(&report, "copy_external_image_to_texture", external),
        Err(error) => set(&report, "copy_external_image_error", error),
    }
    publish(&report);

    let search = window.location().search()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search)?;
    let panics = Array::new();
    if params.has("panic") {
        // Each of these panics inside wgpu's WebGPU backend (an `unwrap` on a JS exception, or `unimplemented!`), which
        // aborts the wasm instance, so the report is published first and only one of them runs per page load.
        let which = params.get("panic").unwrap_or_default();
        panics.push(&which.clone().into());
        set(&report, "about_to_panic", panics);
        publish(&report);
        match which.as_str() {
            "short-data" => {
                let target = texture(&context, "short", 16, 16, BGRA, wgpu::TextureUsages::COPY_DST);
                context.queue.write_texture(
                    target.as_image_copy(),
                    &[0u8; 16],
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(64), rows_per_image: Some(16) },
                    wgpu::Extent3d { width: 16, height: 16, depth_or_array_layers: 1 },
                );
            }
            "closed-bitmap" => {
                let canvas = document.create_element("canvas")?.dyn_into::<HtmlCanvasElement>()?;
                paint_canvas(&canvas, 8, 8)?;
                let bitmap = JsFuture::from(window.create_image_bitmap_with_html_canvas_element(&canvas)?).await?.dyn_into::<web_sys::ImageBitmap>()?;
                bitmap.close();
                let target = texture(&context, "closed", 8, 8, BGRA, wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT);
                context.queue.copy_external_image_to_texture(
                    &wgpu::CopyExternalImageSourceInfo { source: wgpu::ExternalImageSource::ImageBitmap(bitmap), origin: wgpu::Origin2d::ZERO, flip_y: false },
                    wgpu::CopyExternalImageDestInfo {
                        texture: &target,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                        color_space: wgpu::PredefinedColorSpace::Srgb,
                        premultiplied_alpha: false,
                    },
                    wgpu::Extent3d { width: 8, height: 8, depth_or_array_layers: 1 },
                );
            }
            "external-texture" => {
                let plane = texture(&context, "plane", 8, 8, wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::TEXTURE_BINDING);
                let view = plane.create_view(&Default::default());
                let _external = context.device.create_external_texture(
                    &wgpu::ExternalTextureDescriptor {
                        label: Some("external"),
                        width: 8,
                        height: 8,
                        format: wgpu::ExternalTextureFormat::Rgba,
                        yuv_conversion_matrix: [0.0; 16],
                        gamut_conversion_matrix: [0.0; 9],
                        src_transfer_function: Default::default(),
                        dst_transfer_function: Default::default(),
                        sample_transform: [0.0; 6],
                        load_transform: [0.0; 6],
                    },
                    &[&view],
                );
            }
            _ => {}
        }
        set(&report, "survived_panic_case", true);
    }
    JsValue::from(js_sys::JSON::stringify(&report)?).as_string().ok_or_else(|| JsValue::from_str("report is not a string"))
}

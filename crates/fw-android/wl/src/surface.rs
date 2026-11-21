use std::cell::RefCell;

use smithay::{
    backend::allocator::{
        Buffer, Fourcc, Modifier,
        dmabuf::{DmabufMappingMode, DmabufSyncFlags},
    },
    reexports::wayland_server::protocol::{wl_buffer::WlBuffer, wl_shm},
    utils::{Logical, Point, Size},
    wayland::{
        compositor::{SurfaceAttributes, SurfaceData},
        dmabuf::get_dmabuf,
        shm::{BufferAccessError, with_buffer_contents},
        viewporter::ViewportCachedState,
    },
};

#[derive(Default)]
pub struct SurfaceImage {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u32>,
}

pub type SurfaceImageSlot = RefCell<SurfaceImage>;

#[derive(Clone, Copy)]
pub enum PixelFormat {
    Argb,
    Xrgb,
    Abgr,
    Xbgr,
}

impl PixelFormat {
    pub fn from_shm(format: wl_shm::Format) -> Option<Self> {
        match format {
            wl_shm::Format::Argb8888 => Some(Self::Argb),
            wl_shm::Format::Xrgb8888 => Some(Self::Xrgb),
            wl_shm::Format::Abgr8888 => Some(Self::Abgr),
            wl_shm::Format::Xbgr8888 => Some(Self::Xbgr),
            _ => None,
        }
    }

    pub fn from_fourcc(code: Fourcc) -> Option<Self> {
        match code {
            Fourcc::Argb8888 => Some(Self::Argb),
            Fourcc::Xrgb8888 => Some(Self::Xrgb),
            Fourcc::Abgr8888 => Some(Self::Abgr),
            Fourcc::Xbgr8888 => Some(Self::Xbgr),
            _ => None,
        }
    }

    fn to_argb(self, raw: u32) -> u32 {
        let swapped = (raw & 0xFF00_FF00) | ((raw & 0xFF) << 16) | ((raw >> 16) & 0xFF);
        match self {
            Self::Argb => raw,
            Self::Xrgb => raw | 0xFF00_0000,
            Self::Abgr => swapped,
            Self::Xbgr => swapped | 0xFF00_0000,
        }
    }
}

fn copy_rows(
    bytes: &[u8],
    offset: usize,
    stride: usize,
    width: usize,
    height: usize,
    format: PixelFormat,
) -> Result<Vec<u32>, String> {
    if width == 0 || height == 0 || stride < width * 4 {
        return Err(format!("bad geometry {width}x{height} stride {stride}"));
    }
    let needed = offset + stride * (height - 1) + width * 4;
    if needed > bytes.len() {
        return Err(format!("buffer needs {needed} bytes, mapping has {}", bytes.len()));
    }
    let mut pixels = Vec::with_capacity(width * height);
    for row in 0..height {
        let start = offset + row * stride;
        pixels.extend(
            bytes[start..start + width * 4]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|chunk| format.to_argb(u32::from_le_bytes(*chunk))),
        );
    }
    Ok(pixels)
}

fn import_dmabuf(buffer: &WlBuffer) -> Result<SurfaceImage, String> {
    let dmabuf = get_dmabuf(buffer).map_err(|_| "buffer is neither shm nor dmabuf".to_string())?;
    let format = dmabuf.format();
    let pixel_format = PixelFormat::from_fourcc(format.code)
        .ok_or_else(|| format!("dmabuf format {:?} unsupported", format.code))?;
    if !matches!(format.modifier, Modifier::Linear | Modifier::Invalid) {
        return Err(format!("dmabuf modifier {:?} is not linear", format.modifier));
    }
    let size = dmabuf.size();
    let stride = dmabuf.strides().next().ok_or("dmabuf without plane")? as usize;
    let mapping = dmabuf
        .map_plane(0, DmabufMappingMode::READ)
        .map_err(|error| format!("dmabuf mmap failed: {error}"))?;
    if dmabuf.sync_plane(0, DmabufSyncFlags::START | DmabufSyncFlags::READ).is_err() {
        eprintln!("fw-android-wl: dmabuf sync start failed, reading unsynchronized");
    }
    // SAFETY: the mapping stays alive for the whole borrow and is read-only; the client may
    // still write concurrently, which only yields torn pixels, never invalid memory.
    let bytes = unsafe { std::slice::from_raw_parts(mapping.ptr().cast::<u8>(), mapping.length()) };
    let result = copy_rows(bytes, 0, stride, size.w as usize, size.h as usize, pixel_format);
    if dmabuf.sync_plane(0, DmabufSyncFlags::END | DmabufSyncFlags::READ).is_err() {
        eprintln!("fw-android-wl: dmabuf sync end failed");
    }
    Ok(SurfaceImage { width: size.w, height: size.h, pixels: result? })
}

pub fn import_buffer(buffer: &WlBuffer) -> Result<SurfaceImage, String> {
    let shm = with_buffer_contents(buffer, |pointer, length, data| {
        let format = PixelFormat::from_shm(data.format)
            .ok_or_else(|| format!("shm format {:?} unsupported", data.format))?;
        let (offset, stride, width, height) = (
            usize::try_from(data.offset).map_err(|_| "negative offset")?,
            usize::try_from(data.stride).map_err(|_| "negative stride")?,
            usize::try_from(data.width).map_err(|_| "negative width")?,
            usize::try_from(data.height).map_err(|_| "negative height")?,
        );
        // SAFETY: smithay guarantees `pointer` addresses `length` readable bytes for the duration
        // of the closure and guards the pool against truncation with a SIGBUS handler.
        let bytes = unsafe { std::slice::from_raw_parts(pointer, length) };
        let pixels = copy_rows(bytes, offset, stride, width, height, format)?;
        Ok(SurfaceImage { width: data.width, height: data.height, pixels })
    });
    match shm {
        Ok(result) => result,
        Err(BufferAccessError::NotManaged) => import_dmabuf(buffer),
        Err(error) => Err(format!("shm access failed: {error}")),
    }
}

pub struct Placement {
    pub size: Size<i32, Logical>,
    /// Source rectangle in buffer pixels as x, y, width, height.
    pub source: [f64; 4],
}

pub fn subsurface_offset(states: &SurfaceData) -> Point<i32, Logical> {
    if states.role == Some("subsurface") {
        states
            .cached_state
            .get::<smithay::wayland::compositor::SubsurfaceCachedState>()
            .current()
            .location
    } else {
        Point::from((0, 0))
    }
}

pub fn placement(states: &SurfaceData, origin: Point<i32, Logical>) -> Option<Placement> {
    let slot = states.data_map.get::<SurfaceImageSlot>()?;
    let image = slot.borrow();
    if image.pixels.is_empty() {
        return None;
    }
    let scale = f64::from(states.cached_state.get::<SurfaceAttributes>().current().buffer_scale.max(1));
    let (viewport_source, viewport_destination) = {
        let mut guard = states.cached_state.get::<ViewportCachedState>();
        let current = guard.current();
        (current.src, current.dst)
    };
    let source = match viewport_source {
        Some(rect) => [
            rect.loc.x * scale,
            rect.loc.y * scale,
            rect.size.w * scale,
            rect.size.h * scale,
        ],
        None => [0.0, 0.0, f64::from(image.width), f64::from(image.height)],
    };
    let size = viewport_destination.unwrap_or_else(|| {
        Size::from((
            ((source[2] / scale).ceil() as i32).max(1),
            ((source[3] / scale).ceil() as i32).max(1),
        ))
    });
    Some(Placement { size, source })
}

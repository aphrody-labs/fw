use std::time::{Duration, Instant};

use smithay::{
    backend::allocator::{Format, Fourcc, Modifier, dmabuf::Dmabuf},
    delegate_compositor, delegate_data_device, delegate_dmabuf, delegate_fractional_scale,
    delegate_idle_inhibit, delegate_output, delegate_pointer_constraints, delegate_presentation,
    delegate_relative_pointer, delegate_seat, delegate_shm, delegate_viewporter, delegate_xdg_shell,
    input::{
        Seat, SeatHandler, SeatState,
        keyboard::XkbConfig,
        pointer::{CursorImageStatus, MotionEvent, PointerHandle},
    },
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{
        wayland_protocols::{
            wp::presentation_time::server::wp_presentation_feedback,
            xdg::shell::server::xdg_toplevel,
        },
        wayland_server::{
            Client, DisplayHandle, Resource,
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::{wl_buffer::WlBuffer, wl_output::WlOutput, wl_seat::WlSeat, wl_surface::WlSurface},
        },
    },
    utils::{Logical, Point, Serial, Size, Transform, SERIAL_COUNTER},
    wayland::{
        buffer::BufferHandler,
        compositor::{
            BufferAssignment, CompositorClientState, CompositorHandler, CompositorState,
            SurfaceAttributes, TraversalAction, get_parent, is_sync_subsurface, with_states,
            with_surface_tree_downward, with_surface_tree_upward,
        },
        dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier},
        fractional_scale::{
            FractionalScaleHandler, FractionalScaleManagerState, with_fractional_scale,
        },
        idle_inhibit::{IdleInhibitHandler, IdleInhibitManagerState},
        output::{OutputHandler, OutputManagerState},
        pointer_constraints::{PointerConstraintsHandler, PointerConstraintsState, with_pointer_constraint},
        presentation::{PresentationFeedbackCachedState, PresentationState, Refresh},
        relative_pointer::RelativePointerManagerState,
        selection::{
            SelectionHandler,
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
                set_data_device_focus,
            },
        },
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
        },
        shm::{ShmHandler, ShmState},
        viewporter::ViewporterState,
    },
};

use crate::surface::{self, SurfaceImageSlot};

pub const BACKGROUND: u32 = 0xFF20_2124;

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, client_id: ClientId) {
        eprintln!("fw-android-wl: client {client_id:?} initialized");
    }

    fn disconnected(&self, client_id: ClientId, reason: DisconnectReason) {
        eprintln!("fw-android-wl: client {client_id:?} disconnected ({reason:?})");
    }
}

pub struct Window {
    pub id: u32,
    pub toplevel: ToplevelSurface,
    pub position: Point<i32, Logical>,
    pub announced: bool,
}

impl Window {
    pub fn surface(&self) -> &WlSurface {
        self.toplevel.wl_surface()
    }

    pub fn fills_canvas(&self) -> bool {
        self.toplevel.with_pending_state(|state| {
            state.states.contains(xdg_toplevel::State::Maximized)
                || state.states.contains(xdg_toplevel::State::Fullscreen)
        })
    }

    pub fn labels(&self) -> (Option<String>, Option<String>) {
        with_states(self.surface(), |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().ok().map(|data| (data.app_id.clone(), data.title.clone())))
                .unwrap_or((None, None))
        })
    }
}

pub struct State {
    pub display_handle: DisplayHandle,
    pub started: Instant,
    pub size: (i32, i32),
    pub scale: f64,
    pub quit: bool,
    pub windows: Vec<Window>,
    pub next_window_id: u32,
    pub pointer_down: bool,
    pub pointer_position: Point<f64, Logical>,
    pub presentation_sequence: u64,
    pub seat: Seat<State>,
    pub output: Output,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<State>,
    pub data_device_state: DataDeviceState,
    pub dmabuf_state: DmabufState,
}

impl State {
    pub fn new(display_handle: DisplayHandle, size: (i32, i32), scale: f64) -> Self {
        let compositor_state = CompositorState::new::<Self>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<Self>(&display_handle);
        let shm_state = ShmState::new::<Self>(
            &display_handle,
            [
                smithay::reexports::wayland_server::protocol::wl_shm::Format::Abgr8888,
                smithay::reexports::wayland_server::protocol::wl_shm::Format::Xbgr8888,
            ],
        );
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&display_handle);
        OutputManagerState::new_with_xdg_output::<Self>(&display_handle);

        let mut dmabuf_state = DmabufState::new();
        let formats = [Fourcc::Xrgb8888, Fourcc::Argb8888, Fourcc::Abgr8888, Fourcc::Xbgr8888]
            .into_iter()
            .map(|code| Format { code, modifier: Modifier::Linear });
        dmabuf_state.create_global::<Self>(&display_handle, formats);

        ViewporterState::new::<Self>(&display_handle);
        PresentationState::new::<Self>(&display_handle, libc::CLOCK_MONOTONIC as u32);
        FractionalScaleManagerState::new::<Self>(&display_handle);
        PointerConstraintsState::new::<Self>(&display_handle);
        RelativePointerManagerState::new::<Self>(&display_handle);
        IdleInhibitManagerState::new::<Self>(&display_handle);

        let output = Output::new(
            "fw-android-0".into(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "fw".into(),
                model: "android".into(),
            },
        );
        let mode = Mode { size: size.into(), refresh: 60_000 };
        output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(Scale::Fractional(scale)),
            Some((0, 0).into()),
        );
        output.set_preferred(mode);
        output.create_global::<Self>(&display_handle);

        let mut seat = seat_state.new_seat("seat0");
        seat.add_pointer();
        seat.add_touch();
        if let Err(error) = seat.add_keyboard(XkbConfig::default(), 200, 25) {
            eprintln!("fw-android-wl: keyboard capability unavailable ({error}); xkb data missing?");
        }

        Self {
            display_handle,
            started: Instant::now(),
            size,
            scale,
            quit: false,
            windows: Vec::new(),
            next_window_id: 1,
            pointer_down: false,
            pointer_position: Point::from((0.0, 0.0)),
            presentation_sequence: 0,
            seat,
            output,
            compositor_state,
            xdg_shell_state,
            shm_state,
            seat_state,
            data_device_state,
            dmabuf_state,
        }
    }

    pub fn describe_globals(&self) -> String {
        "wl_compositor(6) wl_subcompositor(1) xdg_wm_base(6) wl_seat(9: pointer keyboard touch) wl_shm(1) wl_output(4) \
         zxdg_output_manager_v1 wp_viewporter wp_presentation zwp_linux_dmabuf_v1(3, linear XRGB8888 ARGB8888 ABGR8888 XBGR8888) \
         wp_fractional_scale_manager_v1 wl_data_device_manager zwp_pointer_constraints_v1 zwp_relative_pointer_manager_v1 \
         zwp_idle_inhibit_manager_v1"
            .into()
    }

    pub fn now_ms(&self) -> u32 {
        self.started.elapsed().as_millis() as u32
    }

    pub fn canvas_logical(&self) -> Size<i32, Logical> {
        Size::from((
            ((f64::from(self.size.0) / self.scale).round() as i32).max(1),
            ((f64::from(self.size.1) / self.scale).round() as i32).max(1),
        ))
    }

    fn window_index(&self, surface: &WlSurface) -> Option<usize> {
        self.windows.iter().position(|window| window.surface() == surface)
    }

    fn refresh_position(&mut self, index: usize) {
        let cascade = (index % 12) as i32 * 32;
        if let Some(window) = self.windows.get_mut(index) {
            window.position = if window.fills_canvas() {
                Point::from((0, 0))
            } else {
                Point::from((cascade, cascade))
            };
        }
    }

    fn send_configure(&mut self, toplevel: &ToplevelSurface) {
        let size = self.canvas_logical();
        let fills = toplevel.with_pending_state(|state| {
            let fills = state.states.contains(xdg_toplevel::State::Maximized)
                || state.states.contains(xdg_toplevel::State::Fullscreen);
            state.size = fills.then_some(size);
            state.states.set(xdg_toplevel::State::Activated);
            fills
        });
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        } else {
            toplevel.send_configure();
        }
        if let Some(index) = self.window_index(toplevel.wl_surface()) {
            self.refresh_position(index);
            eprintln!(
                "fw-android-wl: toplevel #{} configure fills_canvas={fills} size={}x{}",
                self.windows[index].id, size.w, size.h
            );
        }
    }

    pub fn raise_and_focus(&mut self, index: usize) {
        if index >= self.windows.len() {
            return;
        }
        let window = self.windows.remove(index);
        let surface = window.surface().clone();
        self.windows.push(window);
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial());
        }
    }

    pub fn surface_at(&self, point: Point<f64, Logical>) -> Option<(usize, WlSurface, Point<f64, Logical>)> {
        for (index, window) in self.windows.iter().enumerate().rev() {
            let mut hit = None;
            with_surface_tree_downward(
                window.surface(),
                window.position,
                |_, states, parent| TraversalAction::DoChildren(*parent + surface::subsurface_offset(states)),
                |surface, states, parent| {
                    if hit.is_some() {
                        return;
                    }
                    let origin = *parent + surface::subsurface_offset(states);
                    let Some(placement) = surface::placement(states, origin) else { return };
                    let local = point - origin.to_f64();
                    let inside = local.x >= 0.0
                        && local.y >= 0.0
                        && local.x < f64::from(placement.size.w)
                        && local.y < f64::from(placement.size.h);
                    let accepts = states
                        .cached_state
                        .get::<SurfaceAttributes>()
                        .current()
                        .input_region
                        .as_ref()
                        .is_none_or(|region| region.contains((local.x as i32, local.y as i32)));
                    if inside && accepts {
                        hit = Some((surface.clone(), origin.to_f64()));
                    }
                },
                |_, _, _| true,
            );
            if let Some((surface, origin)) = hit {
                return Some((index, surface, origin));
            }
        }
        None
    }

    pub fn pointer_motion(&mut self, point: Point<f64, Logical>) {
        self.pointer_position = point;
        let focus = self.surface_at(point).map(|(_, surface, origin)| (surface, origin));
        if let Some(pointer) = self.seat.get_pointer() {
            let event = MotionEvent { location: point, serial: SERIAL_COUNTER.next_serial(), time: self.now_ms() };
            pointer.motion(self, focus, &event);
            pointer.frame(self);
        }
    }

    pub fn pointer_button(&mut self, pressed: bool) {
        if pressed == self.pointer_down {
            return;
        }
        self.pointer_down = pressed;
        if pressed && let Some((index, _, _)) = self.surface_at(self.pointer_position) {
            self.raise_and_focus(index);
        }
        if let Some(pointer) = self.seat.get_pointer() {
            let event = smithay::input::pointer::ButtonEvent {
                serial: SERIAL_COUNTER.next_serial(),
                time: self.now_ms(),
                button: 0x110,
                state: if pressed {
                    smithay::backend::input::ButtonState::Pressed
                } else {
                    smithay::backend::input::ButtonState::Released
                },
            };
            pointer.button(self, &event);
            pointer.frame(self);
        }
    }

    fn present_commit(&mut self, surface: &WlSurface) {
        let now = self.now_ms();
        let mut frames = Vec::new();
        let mut feedback = Vec::new();
        with_surface_tree_upward(
            surface,
            (),
            |_, _, _| TraversalAction::DoChildren(()),
            |surface, states, _| {
                let assignment;
                {
                    let mut guard = states.cached_state.get::<SurfaceAttributes>();
                    let attributes = guard.current();
                    assignment = attributes.buffer.take();
                    frames.append(&mut attributes.frame_callbacks);
                }
                feedback.append(&mut states.cached_state.get::<PresentationFeedbackCachedState>().current().callbacks);
                let slot = states.data_map.get_or_insert(SurfaceImageSlot::default);
                match assignment {
                    Some(BufferAssignment::NewBuffer(buffer)) => {
                        match surface::import_buffer(&buffer) {
                            Ok(image) => *slot.borrow_mut() = image,
                            Err(error) => eprintln!("fw-android-wl: buffer skipped on {:?}: {error}", surface.id()),
                        }
                        buffer.release();
                    }
                    Some(BufferAssignment::Removed) => *slot.borrow_mut() = surface::SurfaceImage::default(),
                    None => {}
                }
            },
            |_, _, _| true,
        );
        for callback in frames {
            callback.done(now);
        }
        if !feedback.is_empty() {
            let time = monotonic_now();
            self.presentation_sequence += 1;
            for callback in feedback {
                callback.presented(
                    &self.output,
                    time,
                    Refresh::fixed(Duration::from_micros(16_667)),
                    self.presentation_sequence,
                    wp_presentation_feedback::Kind::empty(),
                );
            }
        }
    }

    pub fn is_mapped(&self, window: &Window) -> bool {
        with_states(window.surface(), |states| {
            states
                .data_map
                .get::<SurfaceImageSlot>()
                .is_some_and(|slot| !slot.borrow().pixels.is_empty())
        })
    }

    pub fn window_size(&self, window: &Window) -> Option<Size<i32, Logical>> {
        with_states(window.surface(), |states| {
            surface::placement(states, window.position).map(|placement| placement.size)
        })
    }
}

fn monotonic_now() -> Duration {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Duration::new(now.tv_sec.max(0) as u64, now.tv_nsec.max(0) as u32)
}

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        match client.get_data::<ClientState>() {
            Some(state) => &state.compositor_state,
            None => unreachable!("every client is inserted with ClientState"),
        }
    }

    fn new_surface(&mut self, surface: &WlSurface) {
        self.output.enter(surface);
    }

    fn commit(&mut self, surface: &WlSurface) {
        if is_sync_subsurface(surface) {
            return;
        }
        self.present_commit(surface);

        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        let Some(index) = self.window_index(&root) else { return };
        let toplevel = self.windows[index].toplevel.clone();
        if !toplevel.is_initial_configure_sent() {
            self.send_configure(&toplevel);
        }
        let mapped = self.windows.get(index).is_some_and(|window| self.is_mapped(window));
        if mapped && !self.windows[index].announced {
            self.windows[index].announced = true;
            let (app_id, title) = self.windows[index].labels();
            let size = self.window_size(&self.windows[index]);
            eprintln!(
                "fw-android-wl: toplevel #{} mapped app_id={app_id:?} title={title:?} size={:?}",
                self.windows[index].id,
                size.map(|size| (size.w, size.h))
            );
            self.raise_and_focus(index);
        }
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let id = self.next_window_id;
        self.next_window_id += 1;
        eprintln!("fw-android-wl: toplevel #{id} created on {:?}", surface.wl_surface().id());
        self.windows.push(Window { id, toplevel: surface, position: Point::from((0, 0)), announced: false });
        let index = self.windows.len() - 1;
        self.refresh_position(index);
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        surface.with_pending_state(|state| state.geometry = positioner.get_geometry());
        if let Err(error) = surface.send_configure() {
            eprintln!("fw-android-wl: popup configure failed: {error}");
        }
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: WlSeat, _serial: Serial) {}

    fn reposition_request(&mut self, surface: PopupSurface, positioner: PositionerState, token: u32) {
        surface.with_pending_state(|state| state.geometry = positioner.get_geometry());
        surface.send_repositioned(token);
    }

    fn maximize_request(&mut self, surface: ToplevelSurface) {
        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Maximized);
        });
        if surface.is_initial_configure_sent() {
            self.send_configure(&surface);
        }
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        surface.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Maximized);
        });
        if surface.is_initial_configure_sent() {
            self.send_configure(&surface);
        }
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<WlOutput>) {
        surface.with_pending_state(|state| {
            state.states.set(xdg_toplevel::State::Fullscreen);
        });
        if surface.is_initial_configure_sent() {
            self.send_configure(&surface);
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        surface.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Fullscreen);
        });
        if surface.is_initial_configure_sent() {
            self.send_configure(&surface);
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if let Some(index) = self.window_index(surface.wl_surface()) {
            let window = self.windows.remove(index);
            eprintln!("fw-android-wl: toplevel #{} destroyed", window.id);
            if let Some(top) = self.windows.len().checked_sub(1) {
                self.raise_and_focus(top);
            } else if let Some(keyboard) = self.seat.get_keyboard() {
                keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
            }
        }
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        if let Some(index) = self.window_index(surface.wl_surface()) {
            eprintln!("fw-android-wl: toplevel #{} app_id={:?}", self.windows[index].id, self.windows[index].labels().0);
        }
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        if let Some(index) = self.window_index(surface.wl_surface()) {
            eprintln!("fw-android-wl: toplevel #{} title={:?}", self.windows[index].id, self.windows[index].labels().1);
        }
    }
}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        set_data_device_focus(&self.display_handle, seat, focused.and_then(Resource::client));
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {}
}

impl SelectionHandler for State {
    type SelectionUserData = ();
}

impl DataDeviceHandler for State {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for State {}
impl ServerDndGrabHandler for State {}

impl OutputHandler for State {}

impl FractionalScaleHandler for State {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self.scale;
        with_states(&surface, |states| with_fractional_scale(states, |state| state.set_preferred_scale(scale)));
    }
}

impl PointerConstraintsHandler for State {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        with_pointer_constraint(surface, pointer, |constraint| {
            if let Some(constraint) = constraint {
                constraint.activate();
            }
        });
        eprintln!("fw-android-wl: pointer constraint activated on {:?}", surface.id());
    }

    fn cursor_position_hint(&mut self, _surface: &WlSurface, _pointer: &PointerHandle<Self>, _location: Point<f64, Logical>) {}
}

impl IdleInhibitHandler for State {
    fn inhibit(&mut self, surface: WlSurface) {
        eprintln!("fw-android-wl: idle inhibited by {:?}", surface.id());
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        eprintln!("fw-android-wl: idle uninhibited by {:?}", surface.id());
    }
}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(&mut self, _global: &DmabufGlobal, dmabuf: Dmabuf, notifier: ImportNotifier) {
        use smithay::backend::allocator::Buffer;
        let format = dmabuf.format();
        let usable = surface::PixelFormat::from_fourcc(format.code).is_some()
            && matches!(format.modifier, Modifier::Linear | Modifier::Invalid)
            && dmabuf.num_planes() == 1;
        if usable {
            if notifier.successful::<Self>().is_err() {
                eprintln!("fw-android-wl: dmabuf import raced with client disconnect");
            }
        } else {
            eprintln!("fw-android-wl: dmabuf import refused ({:?}, planes {})", format, dmabuf.num_planes());
            notifier.failed();
        }
    }
}

delegate_compositor!(State);
delegate_shm!(State);
delegate_xdg_shell!(State);
delegate_seat!(State);
delegate_output!(State);
delegate_viewporter!(State);
delegate_presentation!(State);
delegate_fractional_scale!(State);
delegate_dmabuf!(State);
delegate_pointer_constraints!(State);
delegate_relative_pointer!(State);
delegate_idle_inhibit!(State);
delegate_data_device!(State);

//! Compositor "blur behind" for the transparent main window (sidebar glass).
//! Native Wayland only: X11 gets an opaque window (`create_main_window` in lib.rs).
//!
//! The window is transparent; the frontend paints the
//! sidebar semi-transparent and the display area opaque, so blurring the whole
//! surface only shows through the sidebar.
//! - Wayland: `ext_background_effect_manager_v1` (KWin 6.8+; standard staging
//!   protocol, its `capabilities` event says whether blur is on), otherwise
//!   KDE's older `org_kde_kwin_blur_manager` (only advertised while KWin's blur
//!   effect is loaded). Both are applied to GTK's own `wl_surface`.
//!
//! `enable` returns `Err` with the reason when blur cannot be requested; the
//! frontend then keeps the sidebar opaque.

use gtk::glib::object::ObjectExt;
use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::WidgetExt;

pub fn enable<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) -> Result<(), String> {
    let gtk_window = window.gtk_window().map_err(|e| e.to_string())?;
    let gdk_window = gtk_window.window().ok_or("GTK window is not realized")?;
    let display = gdk_window.display();
    match display.type_().name() {
        "GdkWaylandDisplay" => {
            // SAFETY: both objects are Wayland GDK objects (checked above) and
            // outlive this call; GDK returns borrowed pointers.
            unsafe {
                let wl_display = gdk_wayland_sys::gdk_wayland_display_get_wl_display(
                    ToGlibPtr::<*mut gtk::gdk::ffi::GdkDisplay>::to_glib_none(&display).0 as *mut _,
                );
                let wl_surface = gdk_wayland_sys::gdk_wayland_window_get_wl_surface(
                    ToGlibPtr::<*mut gtk::gdk::ffi::GdkWindow>::to_glib_none(&gdk_window).0
                        as *mut _,
                );
                wayland::blur(wl_display, wl_surface)
            }
        }
        other => Err(format!("unsupported display backend {other}")),
    }
}

mod wayland {
    use std::ffi::c_void;
    use wayland_backend::client::{Backend, ObjectId};
    use wayland_client::globals::{registry_queue_init, GlobalList, GlobalListContents};
    use wayland_client::protocol::{wl_compositor, wl_region, wl_registry, wl_surface::WlSurface};
    use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
    use wayland_protocols::ext::background_effect::v1::client::{
        ext_background_effect_manager_v1::{self as ext_manager, ExtBackgroundEffectManagerV1},
        ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
    };
    use wayland_protocols_plasma::blur::client::{
        org_kde_kwin_blur::OrgKdeKwinBlur, org_kde_kwin_blur_manager::OrgKdeKwinBlurManager,
    };

    #[derive(Default)]
    struct State {
        /// Last `capabilities` from the ext manager; `None` until received.
        ext_blur: Option<bool>,
    }

    impl Dispatch<ExtBackgroundEffectManagerV1, ()> for State {
        fn event(
            state: &mut Self,
            _: &ExtBackgroundEffectManagerV1,
            event: ext_manager::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let ext_manager::Event::Capabilities { flags } = event {
                state.ext_blur = Some(matches!(
                    flags,
                    WEnum::Value(f) if f.contains(ext_manager::Capability::Blur)
                ));
            }
        }
    }

    /// Objects whose events (if any) are irrelevant here.
    macro_rules! ignore_events {
        ($($t:ty => $data:ty),* $(,)?) => {$(
            impl Dispatch<$t, $data> for State {
                fn event(_: &mut Self, _: &$t, _: <$t as Proxy>::Event, _: &$data,
                         _: &Connection, _: &QueueHandle<Self>) {}
            }
        )*};
    }
    ignore_events!(
        wl_registry::WlRegistry => GlobalListContents,
        wl_compositor::WlCompositor => (),
        wl_region::WlRegion => (),
        ExtBackgroundEffectSurfaceV1 => (),
        OrgKdeKwinBlurManager => (),
        OrgKdeKwinBlur => (),
    );

    /// # Safety
    /// `display` and `surface` must be GTK's live `wl_display` / `wl_surface`.
    pub unsafe fn blur(display: *mut c_void, surface: *mut c_void) -> Result<(), String> {
        if display.is_null() || surface.is_null() {
            return Err("no Wayland display/surface".into());
        }
        let conn = Connection::from_backend(Backend::from_foreign_display(display.cast()));
        let (globals, mut queue) =
            registry_queue_init::<State>(&conn).map_err(|e| e.to_string())?;
        let id = ObjectId::from_ptr(WlSurface::interface(), surface.cast())
            .map_err(|e| e.to_string())?;
        let surface = WlSurface::from_id(&conn, id).map_err(|e| e.to_string())?;

        let result = request(&globals, &mut queue, &surface);
        // shortcut: the queue (and its registry) is leaked on every path so late
        // events (e.g. capability changes) never hit a dropped queue; one small
        // allocation per window, upgrade if blur must follow capability changes.
        std::mem::forget(queue);
        conn.flush().map_err(|e| e.to_string())?;
        result
    }

    /// Both requests are double-buffered and take effect on GTK's next
    /// wl_surface.commit (every repaint).
    fn request(
        globals: &GlobalList,
        queue: &mut EventQueue<State>,
        surface: &WlSurface,
    ) -> Result<(), String> {
        let qh = queue.handle();
        if let Ok(manager) = globals.bind::<ExtBackgroundEffectManagerV1, _, _>(&qh, 1..=1, ()) {
            let mut state = State::default();
            queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
            if state.ext_blur != Some(true) {
                return Err("compositor reports blur is disabled".into());
            }
            let compositor: wl_compositor::WlCompositor =
                globals.bind(&qh, 1..=6, ()).map_err(|e| e.to_string())?;
            // The region is clipped to the surface size by the compositor.
            let region = compositor.create_region(&qh, ());
            region.add(0, 0, i32::MAX, i32::MAX);
            manager
                .get_background_effect(surface, &qh, ())
                .set_blur_region(Some(&region));
            region.destroy();
            Ok(())
        } else if let Ok(manager) = globals.bind::<OrgKdeKwinBlurManager, _, _>(&qh, 1..=1, ()) {
            // No set_region: a null region blurs the whole surface.
            manager.create(surface, &qh, ()).commit();
            Ok(())
        } else {
            Err("compositor offers no blur protocol".into())
        }
    }
}

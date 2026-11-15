//! Compositor blur behind the transparent window (Wayland: ext-background-effect-v1, else KDE blur), using winit's raw handles.

pub mod wayland {
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

    /// The surface's blur object; `set_shape` changes which part is blurred.
    pub struct Blur {
        conn: Connection,
        qh: QueueHandle<State>,
        compositor: wl_compositor::WlCompositor,
        effect: Effect,
    }

    enum Effect {
        Ext(ExtBackgroundEffectSurfaceV1),
        Kde(OrgKdeKwinBlur),
    }

    impl Blur {
        /// Blur only a `w`×`h` rounded rectangle (surface-local logical px), so the
        /// blurred backdrop doesn't show square corners outside the window shape.
        /// Takes effect on the toolkit's next wl_surface.commit (every repaint).
        pub fn set_shape(&self, w: i32, h: i32, radius: i32) {
            let region = self.compositor.create_region(&self.qh, ());
            for (x, y, rw, rh) in super::rounded_rect_strips(w, h, radius) {
                region.add(x, y, rw, rh);
            }
            match &self.effect {
                Effect::Ext(e) => e.set_blur_region(Some(&region)),
                Effect::Kde(b) => {
                    b.set_region(Some(&region));
                    b.commit();
                }
            }
            region.destroy();
            let _ = self.conn.flush();
        }
    }

    /// # Safety
    /// `display` and `surface` must be the live `wl_display` / `wl_surface`.
    pub unsafe fn blur(display: *mut c_void, surface: *mut c_void) -> Result<Blur, String> {
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
        let qh = queue.handle();
        std::mem::forget(queue);
        let (compositor, effect) = result?;
        conn.flush().map_err(|e| e.to_string())?;
        Ok(Blur {
            conn,
            qh,
            compositor,
            effect,
        })
    }

    fn request(
        globals: &GlobalList,
        queue: &mut EventQueue<State>,
        surface: &WlSurface,
    ) -> Result<(wl_compositor::WlCompositor, Effect), String> {
        let qh = queue.handle();
        let compositor = || globals.bind(&qh, 1..=6, ()).map_err(|e| e.to_string());
        if let Ok(manager) = globals.bind::<ExtBackgroundEffectManagerV1, _, _>(&qh, 1..=1, ()) {
            let mut state = State::default();
            queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
            if state.ext_blur != Some(true) {
                return Err("compositor reports blur is disabled".into());
            }
            let effect = manager.get_background_effect(surface, &qh, ());
            Ok((compositor()?, Effect::Ext(effect)))
        } else if let Ok(manager) = globals.bind::<OrgKdeKwinBlurManager, _, _>(&qh, 1..=1, ()) {
            Ok((compositor()?, Effect::Kde(manager.create(surface, &qh, ()))))
        } else {
            Err("compositor offers no blur protocol".into())
        }
    }
}

/// Rectangles covering a `w`×`h` rectangle with rounded corners: 1 px rows for the
/// corner bands, one block in between. Each row starts at the pixel nearest the arc
/// at the row's vertical centre, so the blur edge sits within the window's
/// anti-aliased border.
pub fn rounded_rect_strips(w: i32, h: i32, radius: i32) -> Vec<(i32, i32, i32, i32)> {
    let r = radius.clamp(0, w.min(h) / 2);
    let mut out = Vec::new();
    for y in 0..r {
        let dy = r as f32 - y as f32 - 0.5;
        let inset = (r as f32 - (r as f32 * r as f32 - dy * dy).sqrt()).round() as i32;
        out.push((inset, y, w - 2 * inset, 1));
        out.push((inset, h - 1 - y, w - 2 * inset, 1));
    }
    if h > 2 * r {
        out.push((0, r, w, h - 2 * r));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::rounded_rect_strips;

    #[test]
    fn strips_cover_rounded_rect() {
        assert_eq!(rounded_rect_strips(100, 50, 0), [(0, 0, 100, 50)]);
        let s = rounded_rect_strips(100, 50, 16);
        // Middle block plus one top and one bottom row per radius px.
        assert_eq!(s.len(), 33);
        assert_eq!(s.last(), Some(&(0, 16, 100, 18)));
        let (x0, ..) = s[0]; // top row is the most inset
        assert!(x0 > 8 && x0 < 16);
        assert_eq!(s[0].2, 100 - 2 * x0);
        assert_eq!(s[1], (x0, 49, 100 - 2 * x0, 1)); // mirrored bottom row
        let (x_last, ..) = s[30]; // row just above the block reaches the edge
        assert_eq!(x_last, 0);
        // Radius larger than half the smaller side is clamped.
        assert_eq!(rounded_rect_strips(10, 4, 16).len(), 4);
    }
}

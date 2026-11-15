//! Chart geometry for the Slint `Path`s, in pixel units (the BarChart viewbox
//! equals its size). Same rules as the Tauri frontend's `barPath`: one bar per
//! sample, a missing sample leaves a gap, 0 keeps a short stub so the baseline
//! reads as a row of dots.

/// Bars grow from the bottom over the full height, or from the middle line
/// up / down for paired series (upload/read up, download/write down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dir {
    Full,
    Up,
    Down,
}

/// Dashboard: fixed 2 px bars with 1 px gaps, as many recent samples as fit.
/// Detail pages: the last `n` (historyPoints) samples spread over the width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    Fixed,
    Slots(usize),
}

/// Minimum bar height as a share of the bar's span (2 of 100, like the frontend).
const MIN_BAR: f32 = 0.02;

/// SVG path commands for `values` scaled to `max`; newest sample at the right edge.
pub fn bar_path(
    values: &[Option<f64>],
    max: f64,
    dir: Dir,
    layout: Layout,
    width: f32,
    height: f32,
) -> String {
    if width <= 0.0 || height <= 0.0 {
        return String::new();
    }
    let (slot, bar, count) = match layout {
        Layout::Fixed => (3.0, 2.0, (width / 3.0) as usize),
        Layout::Slots(n) => {
            let n = n.max(1);
            let slot = width / n as f32;
            (slot, slot.min(3.0), n)
        }
    };
    let shown = &values[values.len().saturating_sub(count)..];
    let left = width - shown.len() as f32 * slot; // right-aligned
    let (base, span, sign) = match dir {
        Dir::Full => (height, height, -1.0),
        Dir::Up => (height / 2.0, height / 2.0, -1.0),
        Dir::Down => (height / 2.0, height / 2.0, 1.0),
    };
    let upper = if max > 0.0 { max } else { 1.0 };
    let mut d = String::new();
    for (i, v) in shown.iter().enumerate() {
        let Some(v) = v.filter(|v| v.is_finite()) else {
            continue;
        };
        let h = ((v.max(0.0) / upper).min(1.0) as f32 * span).max(MIN_BAR * span);
        let x = left + i as f32 * slot + (slot - bar) / 2.0;
        let y = base + sign * h;
        d += &format!(
            "M{x:.2} {base:.2}L{x:.2} {y:.2}L{:.2} {y:.2}L{:.2} {base:.2}Z",
            x + bar,
            x + bar
        );
    }
    d
}

/// Shared scale for a paired (mirrored) chart: the larger peak of both series
/// plus 12 % headroom, at least 1 so an idle pair does not divide by zero.
pub fn pair_max(a: &[Option<f64>], b: &[Option<f64>]) -> f64 {
    let peak = a
        .iter()
        .chain(b)
        .flatten()
        .filter(|v| v.is_finite())
        .fold(1.0_f64, |m, &v| m.max(v));
    peak * 1.12
}

/// Start offsets and widths (fractions of the bar) for a stacked bar; missing
/// or negative parts count as 0 and the total is clipped to the full width.
pub fn stack(parts: &[Option<f64>], total: f64) -> Vec<(f32, f32)> {
    let mut start = 0.0_f32;
    parts
        .iter()
        .map(|p| {
            let frac = if total > 0.0 {
                (p.unwrap_or(0.0).max(0.0) / total) as f32
            } else {
                0.0
            };
            let width = frac.min(1.0 - start).max(0.0);
            let seg = (start, width);
            start += width;
            seg
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(d: &str) -> Vec<&str> {
        d.split('M').filter(|s| !s.is_empty()).collect()
    }

    #[test]
    fn fixed_bars_are_right_aligned_and_clipped_to_width() {
        let v = [Some(50.0), Some(100.0)];
        let d = bar_path(&v, 100.0, Dir::Full, Layout::Fixed, 10.0, 20.0);
        // 3 slots fit; two samples fill the right two: x 4.5..6.5 and 7.5..9.5.
        assert_eq!(
            bars(&d),
            [
                "4.50 20.00L4.50 10.00L6.50 10.00L6.50 20.00Z",
                "7.50 20.00L7.50 0.00L9.50 0.00L9.50 20.00Z"
            ]
        );
        let many = vec![Some(1.0); 100];
        assert_eq!(
            bars(&bar_path(
                &many,
                100.0,
                Dir::Full,
                Layout::Fixed,
                10.0,
                20.0
            ))
            .len(),
            3
        );
    }

    #[test]
    fn gaps_stubs_and_clamping() {
        let v = [None, Some(0.0), Some(f64::NAN), Some(500.0), Some(-3.0)];
        let d = bar_path(&v, 100.0, Dir::Full, Layout::Slots(5), 50.0, 100.0);
        let b = bars(&d);
        assert_eq!(b.len(), 3, "None and NaN leave gaps: {d}");
        assert!(b[0].contains("L13.50 98.00"), "0 → 2% stub: {}", b[0]);
        assert!(
            b[1].contains("L33.50 0.00"),
            "over max → clamped to full: {}",
            b[1]
        );
        assert!(b[2].contains("98.00"), "negative → stub: {}", b[2]);
    }

    #[test]
    fn slots_spread_and_mirror() {
        // 4 slots over 40 px: slot 10, bar 3 centred → x = 3.5 + 10 i.
        let v = [Some(1.0); 6];
        let d = bar_path(&v, 1.0, Dir::Up, Layout::Slots(4), 40.0, 20.0);
        let b = bars(&d);
        assert_eq!(b.len(), 4, "only the last historyPoints samples");
        assert_eq!(b[0], "3.50 10.00L3.50 0.00L6.50 0.00L6.50 10.00Z");
        let down = bar_path(&v, 1.0, Dir::Down, Layout::Slots(4), 40.0, 20.0);
        assert!(
            bars(&down)[0].contains("L3.50 20.00"),
            "down grows below the middle"
        );
        assert_eq!(bar_path(&v, 1.0, Dir::Full, Layout::Fixed, 0.0, 20.0), "");
    }

    #[test]
    fn pair_scale_and_stack() {
        assert_eq!(pair_max(&[Some(10.0), None], &[Some(50.0)]), 50.0 * 1.12);
        assert_eq!(
            pair_max(&[None], &[Some(f64::INFINITY)]),
            1.12,
            "idle → minimum 1"
        );
        let got = stack(&[Some(2.0), None, Some(6.0), Some(5.0)], 10.0);
        let want = [(0.0, 0.2), (0.2, 0.0), (0.2, 0.6), (0.8, 0.2)];
        for ((s, w), (es, ew)) in got.iter().zip(want) {
            assert!((s - es).abs() < 1e-6 && (w - ew).abs() < 1e-6, "{got:?}");
        }
        assert_eq!(got.len(), 4, "overflow clipped to the full width");
        assert_eq!(stack(&[Some(1.0)], 0.0), [(0.0, 0.0)]);
    }
}

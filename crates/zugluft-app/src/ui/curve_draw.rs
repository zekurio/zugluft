use super::*;

/// Everything the curve editor canvas paints, captured per frame.
pub(super) struct CurveEditorData {
    /// The curve kind being painted.
    pub(super) kind: CurveKind,
    pub(super) window: CurveWindow,
    pub(super) color: u32,
    /// Live `(input °C, output %)` marker of the curve's source.
    pub(super) live: Option<(f32, f32)>,
    /// Index of the point being dragged, drawn emphasized.
    pub(super) drag: Option<usize>,
}

/// Compact, non-interactive curve thumbnail for the curve cards: just the
/// shape and the live point.
pub(super) fn draw_curve_preview(
    bounds: Bounds<Pixels>,
    data: &CurveEditorData,
    window: &mut Window,
) {
    window.paint_quad(fill(bounds, rgb(GRID_CELL)));
    if bounds.size.width <= px(8.) || bounds.size.height <= px(8.) {
        return;
    }
    // Inset vertically so 0 %/100 % lines aren't clipped in half.
    let origin = point(bounds.origin.x, bounds.origin.y + px(4.));
    let plot = size(bounds.size.width, bounds.size.height - px(8.));
    let curve_window = data.window.sanitized();
    let to_px = |temp: f32, percent: f32| {
        point(
            origin.x + plot.width * curve_window.temp_fraction(temp),
            origin.y + plot.height * (1.0 - curve_window.duty_fraction(percent)),
        )
    };

    let shape = curve_shape_points(&data.kind, curve_window);
    if let Some(first) = shape.first().copied() {
        let mut builder = PathBuilder::stroke(px(1.5));
        builder.move_to(to_px(first.0, first.1));
        for &(temp, percent) in shape.iter().skip(1) {
            builder.line_to(to_px(temp, percent));
        }
        if let Ok(path) = builder.build() {
            window.paint_path(path, rgb(data.color));
        }
    }

    if let Some((input, output)) = data.live {
        let center = to_px(input, output);
        let radius = px(3.);
        window.paint_quad(quad(
            Bounds::new(
                point(center.x - radius, center.y - radius),
                size(radius * 2., radius * 2.),
            ),
            radius,
            rgb(ACCENT_OK),
            px(0.),
            gpui::transparent_black(),
            BorderStyle::default(),
        ));
    }
}

pub(super) fn draw_curve_editor(
    bounds: Bounds<Pixels>,
    data: &CurveEditorData,
    window: &mut Window,
) {
    window.paint_quad(fill(bounds, rgb(GRID_CELL)));
    if bounds.size.width <= px(8.) || bounds.size.height <= px(8.) {
        return;
    }
    let origin = bounds.origin;
    let plot = bounds.size;
    let curve_window = data.window.sanitized();
    let to_px = |temp: f32, percent: f32| {
        point(
            origin.x + plot.width * curve_window.temp_fraction(temp),
            origin.y + plot.height * (1.0 - curve_window.duty_fraction(percent)),
        )
    };

    // Gridlines divide the current editor window into quarters.
    for i in 0..=4 {
        let fraction = i as f32 / 4.0;
        let y = origin.y + plot.height * fraction;
        let mut builder = PathBuilder::stroke(px(1.)).dash_array(&[px(2.), px(4.)]);
        builder.move_to(point(origin.x, y));
        builder.line_to(point(origin.x + plot.width, y));
        if let Ok(path) = builder.build() {
            window.paint_path(path, rgb(GRID_LINE));
        }
        let x = origin.x + plot.width * fraction;
        let mut builder = PathBuilder::stroke(px(1.)).dash_array(&[px(2.), px(4.)]);
        builder.move_to(point(x, origin.y));
        builder.line_to(point(x, origin.y + plot.height));
        if let Ok(path) = builder.build() {
            window.paint_path(path, rgb(GRID_LINE));
        }
    }

    // The shape follows the same evaluator the service uses.
    let shape = curve_shape_points(&data.kind, curve_window);
    if let Some(first) = shape.first().copied() {
        let mut builder = PathBuilder::stroke(px(2.));
        builder.move_to(to_px(first.0, first.1));
        for &(temp, percent) in shape.iter().skip(1) {
            builder.line_to(to_px(temp, percent));
        }
        if let Ok(path) = builder.build() {
            window.paint_path(path, rgb(data.color));
        }
    }

    // Live marker: where the source temperature sits on the curve.
    if let Some((input, output)) = data.live {
        let x = origin.x + plot.width * curve_window.temp_fraction(input);
        let mut builder = PathBuilder::stroke(px(1.));
        builder.move_to(point(x, origin.y));
        builder.line_to(point(x, origin.y + plot.height));
        if let Ok(path) = builder.build() {
            window.paint_path(path, rgb(CROSSHAIR));
        }
        let center = to_px(input, output);
        let radius = px(4.);
        window.paint_quad(quad(
            Bounds::new(
                point(center.x - radius, center.y - radius),
                size(radius * 2., radius * 2.),
            ),
            radius,
            rgb(ACCENT_OK),
            px(0.),
            gpui::transparent_black(),
            BorderStyle::default(),
        ));
    }

    // Point handles on top, in stored order so graph drag indices match.
    if let CurveKind::Graph { points } = &data.kind {
        for (i, &(temp, percent)) in points.iter().enumerate() {
            let center = to_px(temp, percent);
            let radius = if data.drag == Some(i) {
                px(7.)
            } else {
                px(5.5)
            };
            window.paint_quad(quad(
                Bounds::new(
                    point(center.x - radius, center.y - radius),
                    size(radius * 2., radius * 2.),
                ),
                radius,
                rgb(data.color),
                px(1.5),
                rgb(GRID_CELL),
                BorderStyle::default(),
            ));
        }
    }
}

fn curve_shape_points(kind: &CurveKind, window: CurveWindow) -> Vec<(f32, f32)> {
    let window = window.sanitized();
    match kind.sanitized() {
        CurveKind::Graph { points } => {
            let graph = CurveKind::Graph {
                points: points.clone(),
            };
            let Some(start) = graph.evaluate(window.temp_min) else {
                return Vec::new();
            };
            let Some(end) = graph.evaluate(window.temp_max) else {
                return Vec::new();
            };
            let mut shape = vec![(window.temp_min, start)];
            shape.extend(
                points
                    .into_iter()
                    .filter(|(temp, _)| window.temp_min <= *temp && *temp <= window.temp_max),
            );
            shape.push((window.temp_max, end));
            shape
        }
        CurveKind::Trigger {
            threshold,
            before,
            after,
            ramp,
        } if ramp <= f32::EPSILON => {
            if threshold <= window.temp_min {
                vec![(window.temp_min, after), (window.temp_max, after)]
            } else if threshold >= window.temp_max {
                vec![(window.temp_min, before), (window.temp_max, before)]
            } else {
                vec![
                    (window.temp_min, before),
                    (threshold, before),
                    (threshold, after),
                    (window.temp_max, after),
                ]
            }
        }
        CurveKind::Trigger { .. } => {
            let mut temps = vec![window.temp_min, window.temp_max];
            if let CurveKind::Trigger {
                threshold, ramp, ..
            } = kind.sanitized()
            {
                temps.push(threshold);
                temps.push(threshold + ramp);
            }
            temps.sort_by(|a, b| a.total_cmp(b));
            temps.dedup_by(|a, b| (*a - *b).abs() < 0.05);
            temps
                .into_iter()
                .filter(|temp| window.temp_min <= *temp && *temp <= window.temp_max)
                .filter_map(|temp| kind.evaluate(temp).map(|percent| (temp, percent)))
                .collect()
        }
        CurveKind::Linear { .. } => [window.temp_min, window.temp_max]
            .into_iter()
            .filter_map(|temp| kind.evaluate(temp).map(|percent| (temp, percent)))
            .collect(),
    }
}

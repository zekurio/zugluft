use super::*;

impl Zugluft {
    pub(super) fn render_axis_labels(&self, axis: &AxisData, width: f32) -> Div {
        div()
            .w(px(width))
            .h_full()
            .flex()
            .flex_col()
            .justify_between()
            .items_end()
            .children(axis_ticks(axis).into_iter().map(|tick| {
                div()
                    .text_xs()
                    .font_family(FONT_MONO)
                    .text_color(rgb(TEXT_DIM))
                    .child(axis.unit.format_value(tick))
            }))
    }

    pub(super) fn render_sensor_graph(&self, mut graph: GraphData) -> Div {
        let tooltip = self.apply_graph_hover(&mut graph);
        let left_axis = graph.axes[0].clone();
        let right_axes: Vec<AxisData> = graph.axes.iter().skip(1).cloned().collect();
        let graph_for_canvas = graph.clone();
        let graph_bounds = self.graph_bounds.clone();
        let total_seconds = graph.history_len.saturating_sub(1) as f32 * SAMPLE_INTERVAL_SECONDS;

        // Wall-clock labels under the vertical gridlines (one per GRID_SPACING,
        // anchored to the right edge, which is "now").
        let plot_width = self
            .graph_bounds
            .borrow()
            .map(|bounds| f32::from(bounds.size.width))
            .unwrap_or(0.0);
        let mut time_labels: Vec<(f32, String)> = Vec::new();
        if plot_width > 0.0 && total_seconds > 0.0 {
            let seconds_per_px = total_seconds / plot_width;
            let count = (plot_width / GRID_SPACING) as usize;
            // Sample the clock once per frame: anchors every label to the same
            // instant and keeps the (non-trivial) timezone lookup out of the
            // per-label loop, so this path re-renders cheaper during resize.
            let now = chrono::Local::now();
            for k in 1..=count {
                let offset = k as f32 * GRID_SPACING;
                let back = chrono::Duration::milliseconds(
                    (offset * seconds_per_px * 1000.0).round() as i64,
                );
                let time = now - back;
                time_labels.push((offset, time.format("%H:%M:%S").to_string()));
            }
        }

        div()
            .flex_1()
            // Allow the graph to shrink with the window so its bottom axis
            // stays inside the panel instead of being clipped.
            .min_w(px(0.))
            .min_h(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .rounded_lg()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow(floating_shadow())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .gap_2()
                    .child(self.render_axis_labels(&left_axis, 52.0))
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .relative()
                            .child(
                                canvas(
                                    move |bounds, _, _| {
                                        *graph_bounds.borrow_mut() = Some(bounds);
                                    },
                                    move |bounds, _, window, _| {
                                        draw_sensor_graph(bounds, &graph_for_canvas, window);
                                    },
                                )
                                .size_full(),
                            )
                            .children(tooltip.map(|tip| {
                                div()
                                    .absolute()
                                    .left(tip.left)
                                    .top(tip.top)
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(PANEL))
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .shadow(subtle_shadow())
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .w(px(8.))
                                            .h(px(8.))
                                            .flex_none()
                                            .rounded_full()
                                            .bg(rgb(tip.color)),
                                    )
                                    .child(div().text_xs().child(tip.label))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family(FONT_MONO)
                                            .text_color(rgb(TEXT_DIM))
                                            .child(tip.value),
                                    )
                            })),
                    )
                    .children(
                        right_axes
                            .iter()
                            .map(|axis| self.render_axis_labels(axis, 70.0)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .w(px(52.))
                            .text_xs()
                            .text_color(rgb(TEXT_DIM))
                            .child(left_axis.unit.label()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .relative()
                            .h(px(16.))
                            .text_xs()
                            .font_family(FONT_MONO)
                            .text_color(rgb(TEXT_DIM))
                            .child(div().absolute().right_0().child("now"))
                            .children(time_labels.into_iter().map(|(offset, label)| {
                                div()
                                    .absolute()
                                    .right(px(offset - 34.))
                                    .w(px(68.))
                                    .text_center()
                                    .child(label)
                            })),
                    )
                    .children(right_axes.iter().map(|axis| {
                        div()
                            .w(px(70.))
                            .text_right()
                            .text_xs()
                            .text_color(rgb(TEXT_DIM))
                            .child(axis.unit.label())
                    })),
            )
    }
}

pub(super) fn draw_sensor_graph(bounds: Bounds<Pixels>, graph: &GraphData, window: &mut Window) {
    window.paint_quad(fill(bounds, rgb(GRID_CELL)));

    if graph.axes.is_empty() || bounds.size.width <= px(8.) || bounds.size.height <= px(8.) {
        return;
    }

    let plot_origin = bounds.origin;
    let plot_size = bounds.size;

    // Horizontal gridlines stay aligned with the axis tick labels.
    for i in 0..=4 {
        let fraction = i as f32 / 4.0;
        let y = plot_origin.y + plot_size.height * fraction;
        let mut builder = PathBuilder::stroke(px(1.)).dash_array(&[px(2.), px(4.)]);
        builder.move_to(point(plot_origin.x, y));
        builder.line_to(point(plot_origin.x + plot_size.width, y));
        if let Ok(path) = builder.build() {
            window.paint_path(path, rgb(GRID_LINE));
        }
    }

    // Vertical gridlines at fixed pixel spacing, anchored to the right edge
    // ("now"), so resizing the window never stretches the grid.
    let mut x = plot_origin.x + plot_size.width - px(GRID_SPACING);
    while x > plot_origin.x {
        let mut builder = PathBuilder::stroke(px(1.)).dash_array(&[px(2.), px(4.)]);
        builder.move_to(point(x, plot_origin.y));
        builder.line_to(point(x, plot_origin.y + plot_size.height));
        if let Ok(path) = builder.build() {
            window.paint_path(path, rgb(GRID_LINE));
        }
        x -= px(GRID_SPACING);
    }

    let mut axis = PathBuilder::stroke(px(1.));
    axis.move_to(point(plot_origin.x, plot_origin.y));
    axis.line_to(point(plot_origin.x, plot_origin.y + plot_size.height));
    axis.line_to(point(
        plot_origin.x + plot_size.width,
        plot_origin.y + plot_size.height,
    ));
    if let Ok(path) = axis.build() {
        window.paint_path(path, rgb(GRID_LINE));
    }

    // Crosshair at the hovered sample, under the series lines.
    if let Some(index) = graph.hover_index {
        let denom = graph.history_len.saturating_sub(1).max(1) as f32;
        let x = plot_origin.x + plot_size.width * (index as f32 / denom);
        let mut builder = PathBuilder::stroke(px(1.));
        builder.move_to(point(x, plot_origin.y));
        builder.line_to(point(x, plot_origin.y + plot_size.height));
        if let Ok(path) = builder.build() {
            window.paint_path(path, rgb(CROSSHAIR));
        }
    }

    for series in &graph.series {
        if series.values.len() < 2 {
            continue;
        }

        let Some(axis) = graph.axes.iter().find(|axis| axis.unit == series.unit) else {
            continue;
        };
        let range = (axis.max - axis.min).max(1.0);
        let stroke = if graph.hovered == Some(series.key) {
            px(2.5)
        } else {
            px(1.)
        };
        let mut builder = apply_line_style(PathBuilder::stroke(stroke), series.line_style);
        let mut started = false;
        let denom = graph.history_len.saturating_sub(1).max(1) as f32;

        for (index, value) in &series.values {
            let x = plot_origin.x + plot_size.width * (*index as f32 / denom);
            let y_fraction = ((*value - axis.min) / range).clamp(0.0, 1.0);
            let y = plot_origin.y + plot_size.height * (1.0 - y_fraction);
            let plot_point = point(x, y);

            if started {
                builder.line_to(plot_point);
            } else {
                builder.move_to(plot_point);
                started = true;
            }
        }

        if started && let Ok(path) = builder.build() {
            window.paint_path(path, rgb(series.color));
        }
    }
}

pub(super) fn apply_line_style(builder: PathBuilder, style: LineStyle) -> PathBuilder {
    match style {
        LineStyle::Solid => builder,
        LineStyle::Dashed => builder.dash_array(&[px(6.), px(4.)]),
        LineStyle::Dotted => builder.dash_array(&[px(1.), px(4.)]),
        LineStyle::DashDot => builder.dash_array(&[px(7.), px(3.), px(1.), px(3.)]),
    }
}

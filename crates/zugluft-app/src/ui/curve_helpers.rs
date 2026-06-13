use super::*;

pub(super) fn graph_kind_from(kind: &CurveKind) -> CurveKind {
    match kind.sanitized() {
        CurveKind::Graph { points } => CurveKind::Graph { points },
        CurveKind::Linear { start, end } => CurveKind::Graph {
            points: vec![start, end],
        },
        CurveKind::Trigger {
            threshold,
            before,
            after,
            ramp,
        } => {
            let end = threshold + ramp.max(10.0);
            CurveKind::Graph {
                points: vec![(threshold, before), (end.min(150.0), after)],
            }
        }
    }
    .sanitized()
}

pub(super) fn trigger_kind_from(kind: &CurveKind) -> CurveKind {
    match kind.sanitized() {
        CurveKind::Trigger {
            threshold,
            before,
            after,
            ramp,
        } => CurveKind::Trigger {
            threshold,
            before,
            after,
            ramp,
        },
        CurveKind::Graph { points } => {
            let threshold = points
                .get(points.len().saturating_sub(1) / 2)
                .map(|point| point.0)
                .unwrap_or(60.0);
            let before = points.first().map(|point| point.1).unwrap_or(30.0);
            let after = points.last().map(|point| point.1).unwrap_or(100.0);
            CurveKind::Trigger {
                threshold,
                before,
                after,
                ramp: 0.0,
            }
        }
        CurveKind::Linear { start, end } => CurveKind::Trigger {
            threshold: start.0,
            before: start.1,
            after: end.1,
            ramp: (end.0 - start.0).max(0.0),
        },
    }
    .sanitized()
}

pub(super) fn linear_kind_from(kind: &CurveKind) -> CurveKind {
    match kind.sanitized() {
        CurveKind::Linear { start, end } => CurveKind::Linear { start, end },
        CurveKind::Graph { points } => {
            let start = points.first().copied().unwrap_or((30.0, 20.0));
            let end = points.last().copied().unwrap_or((70.0, 100.0));
            CurveKind::Linear { start, end }
        }
        CurveKind::Trigger {
            threshold,
            before,
            after,
            ramp,
        } => CurveKind::Linear {
            start: (threshold, before),
            end: (threshold + ramp.max(10.0), after),
        },
    }
    .sanitized()
}

pub(super) fn curve_function_label(function: CurveFunction) -> String {
    match function.sanitized() {
        CurveFunction::Identity => "Identity".to_string(),
        CurveFunction::Standard { hysteresis } => {
            let hysteresis = hysteresis.sanitized();
            format!(
                "Standard {:.1}C/{:.1}s",
                hysteresis.degrees,
                hysteresis.delay_ms as f32 / 1000.0
            )
        }
        CurveFunction::Ema { alpha } => format!("EMA {:.0}%", alpha * 100.0),
    }
}

pub(super) fn curve_kind_label(kind: &CurveKind) -> &'static str {
    match kind {
        CurveKind::Graph { .. } => "Graph",
        CurveKind::Trigger { .. } => "Trigger",
        CurveKind::Linear { .. } => "Linear",
    }
}

pub(super) fn fmt_axis_value(value: f32) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

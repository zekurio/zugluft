use super::curve_helpers::{graph_kind_from, linear_kind_from, trigger_kind_from};
use super::*;

#[derive(Clone, Copy)]
struct TriggerFields {
    threshold: f32,
    before: f32,
    after: f32,
    ramp: f32,
}

#[derive(Clone, Copy)]
struct LinearFields {
    start: (f32, f32),
    end: (f32, f32),
}

struct CurveStepper {
    minus_id: (&'static str, usize),
    plus_id: (&'static str, usize),
    label: &'static str,
    value: String,
    unit: &'static str,
}

impl Zugluft {
    pub(super) fn render_curve_side_panel(
        &self,
        def: &CurveDef,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let kind_tabs = self.render_curve_kind_tabs(def, index, cx);
        let function_tabs = self.render_curve_function_tabs(def, index, cx);
        let function_details = self.render_curve_function_details(def, index, cx);

        div()
            .w(px(220.))
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Kind"))
                    .child(kind_tabs),
            )
            .child(self.render_curve_kind_controls(def, index, cx))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Function"))
                            .child(self.function_help_button(index)),
                    )
                    .child(function_tabs),
            )
            .child(function_details)
            .child(self.render_curve_window_controls(def, index, cx))
    }

    fn render_curve_kind_tabs(&self, def: &CurveDef, index: usize, cx: &mut Context<Self>) -> Div {
        let kind = def.kind.sanitized();
        let id_graph = def.id.clone();
        let id_trigger = def.id.clone();
        let id_linear = def.id.clone();
        let graph_kind = graph_kind_from(&def.kind);
        let trigger_kind = trigger_kind_from(&def.kind);
        let linear_kind = linear_kind_from(&def.kind);

        self.segmented([
            self.segment(
                ("curve-kind-graph", index),
                "Graph",
                matches!(&kind, CurveKind::Graph { .. }),
                cx,
                move |this, cx| this.set_curve_kind(&id_graph, graph_kind.clone(), cx),
            ),
            self.segment(
                ("curve-kind-trigger", index),
                "Trigger",
                matches!(&kind, CurveKind::Trigger { .. }),
                cx,
                move |this, cx| this.set_curve_kind(&id_trigger, trigger_kind.clone(), cx),
            ),
            self.segment(
                ("curve-kind-linear", index),
                "Linear",
                matches!(&kind, CurveKind::Linear { .. }),
                cx,
                move |this, cx| this.set_curve_kind(&id_linear, linear_kind.clone(), cx),
            ),
        ])
    }

    fn render_curve_function_tabs(
        &self,
        def: &CurveDef,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let function = def.primary_function();
        let id_identity = def.id.clone();
        let id_standard = def.id.clone();
        let id_ema = def.id.clone();

        self.segmented([
            self.segment(
                ("curve-fn-identity", index),
                "Identity",
                matches!(function, CurveFunction::Identity),
                cx,
                move |this, cx| {
                    this.set_curve_primary_function(&id_identity, CurveFunction::Identity, cx)
                },
            ),
            self.segment(
                ("curve-fn-standard", index),
                "Standard",
                matches!(function, CurveFunction::Standard { .. }),
                cx,
                move |this, cx| {
                    this.set_curve_primary_function(
                        &id_standard,
                        CurveFunction::Standard {
                            hysteresis: Default::default(),
                        },
                        cx,
                    )
                },
            ),
            self.segment(
                ("curve-fn-ema", index),
                "EMA",
                matches!(function, CurveFunction::Ema { .. }),
                cx,
                move |this, cx| {
                    this.set_curve_primary_function(&id_ema, CurveFunction::Ema { alpha: 0.25 }, cx)
                },
            ),
        ])
    }

    fn render_curve_function_details(
        &self,
        def: &CurveDef,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        match def.primary_function() {
            CurveFunction::Identity => div()
                .w_full()
                .h(px(24.))
                .flex()
                .items_center()
                .text_xs()
                .font_family(FONT_MONO)
                .text_color(rgb(TEXT_DIM))
                .child("graph output"),
            CurveFunction::Standard { hysteresis } => {
                self.render_standard_function_details(def, index, hysteresis, cx)
            }
            CurveFunction::Ema { alpha } => self.render_ema_function_details(def, index, alpha, cx),
        }
    }

    fn render_standard_function_details(
        &self,
        def: &CurveDef,
        index: usize,
        hysteresis: CurveHysteresis,
        cx: &mut Context<Self>,
    ) -> Div {
        let hysteresis = hysteresis.sanitized();
        let id_degrees_minus = def.id.clone();
        let id_degrees_plus = def.id.clone();
        let id_delay_minus = def.id.clone();
        let id_delay_plus = def.id.clone();
        let mut down = hysteresis;
        down.only_downward = true;
        let mut both = hysteresis;
        both.only_downward = false;
        let id_down = def.id.clone();
        let id_both = def.id.clone();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-fn-h-minus", index),
                    plus_id: ("curve-fn-h-plus", index),
                    label: "Hysteresis",
                    value: format!("{:.1}", hysteresis.degrees),
                    unit: "C",
                },
                cx,
                move |this, cx| this.adjust_curve_hysteresis(&id_degrees_minus, -0.5, 0, false, cx),
                move |this, cx| this.adjust_curve_hysteresis(&id_degrees_plus, 0.5, 0, false, cx),
            ))
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-fn-rt-minus", index),
                    plus_id: ("curve-fn-rt-plus", index),
                    label: "Response",
                    value: format!("{:.1}", hysteresis.delay_ms as f32 / 1000.0),
                    unit: "s",
                },
                cx,
                move |this, cx| this.adjust_curve_hysteresis(&id_delay_minus, 0.0, -500, false, cx),
                move |this, cx| this.adjust_curve_hysteresis(&id_delay_plus, 0.0, 500, false, cx),
            ))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Direction"))
                    .child(self.segmented([
                        self.segment(
                            ("curve-fn-down", index),
                            "Down",
                            hysteresis.only_downward,
                            cx,
                            move |this, cx| {
                                this.set_curve_primary_function(
                                    &id_down,
                                    CurveFunction::Standard { hysteresis: down },
                                    cx,
                                )
                            },
                        ),
                        self.segment(
                            ("curve-fn-both", index),
                            "Both",
                            !hysteresis.only_downward,
                            cx,
                            move |this, cx| {
                                this.set_curve_primary_function(
                                    &id_both,
                                    CurveFunction::Standard { hysteresis: both },
                                    cx,
                                )
                            },
                        ),
                    ])),
            )
    }

    fn render_ema_function_details(
        &self,
        def: &CurveDef,
        index: usize,
        alpha: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let id_alpha_minus = def.id.clone();
        let id_alpha_plus = def.id.clone();
        div().w_full().child(self.curve_function_stepper(
            CurveStepper {
                minus_id: ("curve-fn-alpha-minus", index),
                plus_id: ("curve-fn-alpha-plus", index),
                label: "Alpha",
                value: format!("{:.0}", alpha.clamp(0.01, 1.0) * 100.0),
                unit: "%",
            },
            cx,
            move |this, cx| this.adjust_curve_ema(&id_alpha_minus, -0.05, cx),
            move |this, cx| this.adjust_curve_ema(&id_alpha_plus, 0.05, cx),
        ))
    }

    fn render_curve_kind_controls(
        &self,
        def: &CurveDef,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        match def.kind.sanitized() {
            CurveKind::Graph { points } => div()
                .w_full()
                .h(px(24.))
                .flex()
                .items_center()
                .text_xs()
                .font_family(FONT_MONO)
                .text_color(rgb(TEXT_DIM))
                .child(format!("{} points", points.len())),
            CurveKind::Trigger {
                threshold,
                before,
                after,
                ramp,
            } => self.render_trigger_curve_controls(
                def,
                index,
                TriggerFields {
                    threshold,
                    before,
                    after,
                    ramp,
                },
                cx,
            ),
            CurveKind::Linear { start, end } => {
                self.render_linear_curve_controls(def, index, LinearFields { start, end }, cx)
            }
        }
    }

    fn render_trigger_curve_controls(
        &self,
        def: &CurveDef,
        index: usize,
        fields: TriggerFields,
        cx: &mut Context<Self>,
    ) -> Div {
        let id_threshold_minus = def.id.clone();
        let id_threshold_plus = def.id.clone();
        let id_before_minus = def.id.clone();
        let id_before_plus = def.id.clone();
        let id_after_minus = def.id.clone();
        let id_after_plus = def.id.clone();
        let id_ramp_minus = def.id.clone();
        let id_ramp_plus = def.id.clone();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-trigger-th-minus", index),
                    plus_id: ("curve-trigger-th-plus", index),
                    label: "Threshold",
                    value: fmt_setting(fields.threshold),
                    unit: "C",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_kind(
                        &id_threshold_minus,
                        CurveKindField::TriggerThreshold,
                        -1.0,
                        cx,
                    )
                },
                move |this, cx| {
                    this.adjust_curve_kind(
                        &id_threshold_plus,
                        CurveKindField::TriggerThreshold,
                        1.0,
                        cx,
                    )
                },
            ))
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-trigger-before-minus", index),
                    plus_id: ("curve-trigger-before-plus", index),
                    label: "Before",
                    value: fmt_setting(fields.before),
                    unit: "%",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_kind(
                        &id_before_minus,
                        CurveKindField::TriggerBefore,
                        -5.0,
                        cx,
                    )
                },
                move |this, cx| {
                    this.adjust_curve_kind(&id_before_plus, CurveKindField::TriggerBefore, 5.0, cx)
                },
            ))
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-trigger-after-minus", index),
                    plus_id: ("curve-trigger-after-plus", index),
                    label: "After",
                    value: fmt_setting(fields.after),
                    unit: "%",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_kind(&id_after_minus, CurveKindField::TriggerAfter, -5.0, cx)
                },
                move |this, cx| {
                    this.adjust_curve_kind(&id_after_plus, CurveKindField::TriggerAfter, 5.0, cx)
                },
            ))
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-trigger-ramp-minus", index),
                    plus_id: ("curve-trigger-ramp-plus", index),
                    label: "Ramp",
                    value: fmt_setting(fields.ramp),
                    unit: "C",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_kind(&id_ramp_minus, CurveKindField::TriggerRamp, -1.0, cx)
                },
                move |this, cx| {
                    this.adjust_curve_kind(&id_ramp_plus, CurveKindField::TriggerRamp, 1.0, cx)
                },
            ))
    }

    fn render_linear_curve_controls(
        &self,
        def: &CurveDef,
        index: usize,
        fields: LinearFields,
        cx: &mut Context<Self>,
    ) -> Div {
        let id_start_temp_minus = def.id.clone();
        let id_start_temp_plus = def.id.clone();
        let id_start_duty_minus = def.id.clone();
        let id_start_duty_plus = def.id.clone();
        let id_end_temp_minus = def.id.clone();
        let id_end_temp_plus = def.id.clone();
        let id_end_duty_minus = def.id.clone();
        let id_end_duty_plus = def.id.clone();

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-linear-start-t-minus", index),
                    plus_id: ("curve-linear-start-t-plus", index),
                    label: "Start temp",
                    value: fmt_setting(fields.start.0),
                    unit: "C",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_kind(
                        &id_start_temp_minus,
                        CurveKindField::LinearStartTemp,
                        -1.0,
                        cx,
                    )
                },
                move |this, cx| {
                    this.adjust_curve_kind(
                        &id_start_temp_plus,
                        CurveKindField::LinearStartTemp,
                        1.0,
                        cx,
                    )
                },
            ))
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-linear-start-duty-minus", index),
                    plus_id: ("curve-linear-start-duty-plus", index),
                    label: "Start duty",
                    value: fmt_setting(fields.start.1),
                    unit: "%",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_kind(
                        &id_start_duty_minus,
                        CurveKindField::LinearStartDuty,
                        -5.0,
                        cx,
                    )
                },
                move |this, cx| {
                    this.adjust_curve_kind(
                        &id_start_duty_plus,
                        CurveKindField::LinearStartDuty,
                        5.0,
                        cx,
                    )
                },
            ))
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-linear-end-t-minus", index),
                    plus_id: ("curve-linear-end-t-plus", index),
                    label: "End temp",
                    value: fmt_setting(fields.end.0),
                    unit: "C",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_kind(
                        &id_end_temp_minus,
                        CurveKindField::LinearEndTemp,
                        -1.0,
                        cx,
                    )
                },
                move |this, cx| {
                    this.adjust_curve_kind(
                        &id_end_temp_plus,
                        CurveKindField::LinearEndTemp,
                        1.0,
                        cx,
                    )
                },
            ))
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-linear-end-duty-minus", index),
                    plus_id: ("curve-linear-end-duty-plus", index),
                    label: "End duty",
                    value: fmt_setting(fields.end.1),
                    unit: "%",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_kind(
                        &id_end_duty_minus,
                        CurveKindField::LinearEndDuty,
                        -5.0,
                        cx,
                    )
                },
                move |this, cx| {
                    this.adjust_curve_kind(
                        &id_end_duty_plus,
                        CurveKindField::LinearEndDuty,
                        5.0,
                        cx,
                    )
                },
            ))
    }

    fn function_help_button(&self, index: usize) -> Div {
        div().child(
            div()
                .id(("curve-fn-help", index))
                .w(px(16.))
                .h(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(rgb(TRACK))
                .border_1()
                .border_color(rgb(BORDER))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT_DIM))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)).text_color(rgb(TEXT)))
                .tooltip(|_, cx| cx.new(|_| FunctionHelpTooltip).into())
                .child("?"),
        )
    }

    fn render_curve_window_controls(
        &self,
        def: &CurveDef,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let window = def.window.sanitized();
        let temp_min_minus = def.id.clone();
        let temp_min_plus = def.id.clone();
        let temp_max_minus = def.id.clone();
        let temp_max_plus = def.id.clone();
        let duty_min_minus = def.id.clone();
        let duty_min_plus = def.id.clone();
        let duty_max_minus = def.id.clone();
        let duty_max_plus = def.id.clone();

        div()
            .mt_2()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child("Window"))
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-window-tmin-minus", index),
                    plus_id: ("curve-window-tmin-plus", index),
                    label: "Temp min",
                    value: fmt_setting(window.temp_min),
                    unit: "C",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_window(&temp_min_minus, CurveWindowField::TempMin, -5.0, cx)
                },
                move |this, cx| {
                    this.adjust_curve_window(&temp_min_plus, CurveWindowField::TempMin, 5.0, cx)
                },
            ))
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-window-tmax-minus", index),
                    plus_id: ("curve-window-tmax-plus", index),
                    label: "Temp max",
                    value: fmt_setting(window.temp_max),
                    unit: "C",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_window(&temp_max_minus, CurveWindowField::TempMax, -5.0, cx)
                },
                move |this, cx| {
                    this.adjust_curve_window(&temp_max_plus, CurveWindowField::TempMax, 5.0, cx)
                },
            ))
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-window-dmin-minus", index),
                    plus_id: ("curve-window-dmin-plus", index),
                    label: "Duty min",
                    value: fmt_setting(window.duty_min),
                    unit: "%",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_window(&duty_min_minus, CurveWindowField::DutyMin, -5.0, cx)
                },
                move |this, cx| {
                    this.adjust_curve_window(&duty_min_plus, CurveWindowField::DutyMin, 5.0, cx)
                },
            ))
            .child(self.curve_function_stepper(
                CurveStepper {
                    minus_id: ("curve-window-dmax-minus", index),
                    plus_id: ("curve-window-dmax-plus", index),
                    label: "Duty max",
                    value: fmt_setting(window.duty_max),
                    unit: "%",
                },
                cx,
                move |this, cx| {
                    this.adjust_curve_window(&duty_max_minus, CurveWindowField::DutyMax, -5.0, cx)
                },
                move |this, cx| {
                    this.adjust_curve_window(&duty_max_plus, CurveWindowField::DutyMax, 5.0, cx)
                },
            ))
    }

    fn curve_function_stepper(
        &self,
        stepper: CurveStepper,
        cx: &mut Context<Self>,
        on_minus: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        on_plus: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_DIM))
                    .child(stepper.label),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(24.))
                    .rounded_md()
                    .bg(rgb(TRACK))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .overflow_hidden()
                    .child(self.curve_step_button(stepper.minus_id, "-", cx, on_minus))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_1()
                            .text_xs()
                            .font_family(FONT_MONO)
                            .text_color(rgb(TEXT))
                            .child(stepper.value)
                            .child(div().text_color(rgb(TEXT_DIM)).child(stepper.unit)),
                    )
                    .child(self.curve_step_button(stepper.plus_id, "+", cx, on_plus)),
            )
    }

    fn curve_step_button(
        &self,
        id: (&'static str, usize),
        label: &'static str,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        div().child(
            div()
                .id(id)
                .w(px(24.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(rgb(TEXT_DIM))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(FILL_HOVER)).text_color(rgb(TEXT)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| on_click(this, cx)))
                .child(label),
        )
    }
}

struct FunctionHelpTooltip;

impl FunctionHelpTooltip {
    fn row(label: &'static str, body: &'static str) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_0p5()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(TEXT))
                    .child(label),
            )
            .child(div().text_xs().text_color(rgb(TEXT_DIM)).child(body))
    }
}

impl Render for FunctionHelpTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(300.))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_lg()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow(floating_shadow())
            .child(Self::row(
                "Identity",
                "Uses the curve output exactly as drawn.",
            ))
            .child(Self::row(
                "Standard",
                "Adds hysteresis so small or brief temperature changes do not churn the fan target.",
            ))
            .child(Self::row(
                "EMA",
                "Smooths the source temperature before the curve. Higher alpha reacts faster; lower alpha is calmer.",
            ))
    }
}

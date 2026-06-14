use super::*;

impl Zugluft {
    /// The Settings page: units plus device/category/channel visibility.
    pub(super) fn render_settings(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        cx: &mut Context<Self>,
    ) -> Div {
        let section_title = |title: &'static str| {
            div()
                .text_base()
                .font_weight(FontWeight::MEDIUM)
                .child(title.to_string())
        };
        let row_label = |label: &'static str| {
            div()
                .w(px(110.))
                .flex_none()
                .text_sm()
                .text_color(rgb(TEXT_DIM))
                .child(label)
        };
        let unit_control = |control: Div| div().w(px(260.)).flex_none().child(control);

        let units = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section_title("Units"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(row_label("Temperature"))
                    .child(unit_control(self.segmented([
                        self.segment(
                            ("set-unit-c", 0),
                            "°C",
                            self.temp_unit == TempUnit::Celsius,
                            cx,
                            |this, cx| this.set_temp_unit(TempUnit::Celsius, cx),
                        ),
                        self.segment(
                            ("set-unit-f", 0),
                            "°F",
                            self.temp_unit == TempUnit::Fahrenheit,
                            cx,
                            |this, cx| this.set_temp_unit(TempUnit::Fahrenheit, cx),
                        ),
                    ]))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(row_label("Fan speed"))
                    .child(unit_control(self.segmented([
                        self.segment(
                            ("set-unit-rpm", 0),
                            "U/min",
                            self.fan_unit == FanUnit::Rpm,
                            cx,
                            |this, cx| this.set_fan_unit(FanUnit::Rpm, cx),
                        ),
                        self.segment(
                            ("set-unit-pct", 0),
                            "%",
                            self.fan_unit == FanUnit::Percent,
                            cx,
                            |this, cx| this.set_fan_unit(FanUnit::Percent, cx),
                        ),
                    ]))),
            );

        let hardware = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section_title("Hardware"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(row_label("Fans"))
                    .child(self.icon_button(
                        "calibrate-fans-settings",
                        "icons/fan.svg",
                        "Calibrate",
                        cx,
                        |this, cx| {
                            let _ = this.tx.send(Request::Calibrate);
                            cx.notify();
                        },
                    )),
            );

        // Visibility: every device/category/channel with its eye toggle.
        // Channels with no reading only show up while hidden, so they can
        // still be restored.
        let mut chip_sections: Vec<Div> = Vec::new();
        for (ci, info) in chips.iter().enumerate() {
            let Some(snapshot) = snapshots.get(ci) else {
                continue;
            };
            let chip_name = info.name.clone();
            let device_hidden = self.names.is_device_hidden(&chip_name);
            let mut fan_tags = Vec::new();
            for (fi, fan) in snapshot.fans.iter().enumerate() {
                let key = format!("fan{}", fi + 1);
                let hidden = self.names.is_channel_hidden(&chip_name, &key);
                if fan.rpm.is_none() && fan.duty.is_none() && !hidden {
                    continue;
                }
                fan_tags.push(self.visibility_tag(
                    ("vis-fan", ci * 64 + fi),
                    chip_name.clone(),
                    key,
                    format!("Fan: {}", self.names.fan_label(&chip_name, fi)),
                    hidden,
                    cx,
                ));
            }
            let mut temp_tags = Vec::new();
            for (ti, temp) in snapshot.temps.iter().enumerate() {
                let key = format!("temp{}", ti + 1);
                let hidden = self.names.is_channel_hidden(&chip_name, &key);
                if temp.is_none() && !hidden {
                    continue;
                }
                temp_tags.push(self.visibility_tag(
                    ("vis-temp", ci * 64 + ti),
                    chip_name.clone(),
                    key,
                    format!("Sensor: {}", self.temp_label(&chip_name, ti)),
                    hidden,
                    cx,
                ));
            }
            let mut power_tags = Vec::new();
            for (pi, power) in snapshot.powers.iter().enumerate() {
                let key = format!("power{}", pi + 1);
                let hidden = self.names.is_channel_hidden(&chip_name, &key);
                if power.is_none() && !hidden {
                    continue;
                }
                power_tags.push(self.visibility_tag(
                    ("vis-power", ci * 64 + pi),
                    chip_name.clone(),
                    key,
                    format!("Power: {}", self.power_label(&chip_name, pi)),
                    hidden,
                    cx,
                ));
            }

            let mut section = div().flex().flex_col().gap_1p5().child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT_DIM))
                            .child(self.names.device_label(&chip_name)),
                    )
                    .child(self.visibility_device_tag(
                        ("vis-device", ci),
                        chip_name.clone(),
                        device_hidden,
                        cx,
                    )),
            );
            let fans_hidden = self
                .names
                .is_category_hidden(&chip_name, HiddenCategory::Fans);
            if !fan_tags.is_empty() || fans_hidden {
                section =
                    section.child(
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(div().w(px(110.)).flex_none().child(
                                self.visibility_category_tag(
                                    ("vis-cat-fans", ci),
                                    chip_name.clone(),
                                    HiddenCategory::Fans,
                                    "Fans",
                                    fans_hidden,
                                    cx,
                                ),
                            ))
                            .child(div().flex().flex_wrap().gap_1p5().children(fan_tags)),
                    );
            }
            let temps_hidden = self
                .names
                .is_category_hidden(&chip_name, HiddenCategory::Temperatures);
            if !temp_tags.is_empty() || temps_hidden {
                section =
                    section.child(
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(div().w(px(110.)).flex_none().child(
                                self.visibility_category_tag(
                                    ("vis-cat-temps", ci),
                                    chip_name.clone(),
                                    HiddenCategory::Temperatures,
                                    "Sensors",
                                    temps_hidden,
                                    cx,
                                ),
                            ))
                            .child(div().flex().flex_wrap().gap_1p5().children(temp_tags)),
                    );
            }
            let power_hidden = self
                .names
                .is_category_hidden(&chip_name, HiddenCategory::Power);
            if !power_tags.is_empty() || power_hidden {
                section =
                    section.child(
                        div()
                            .flex()
                            .items_start()
                            .gap_2()
                            .child(div().w(px(110.)).flex_none().child(
                                self.visibility_category_tag(
                                    ("vis-cat-power", ci),
                                    chip_name.clone(),
                                    HiddenCategory::Power,
                                    "Power",
                                    power_hidden,
                                    cx,
                                ),
                            ))
                            .child(div().flex().flex_wrap().gap_1p5().children(power_tags)),
                    );
            }
            chip_sections.push(section);
        }

        let visibility = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(section_title("Visibility"))
            .children(chip_sections);

        div().flex_1().min_h(px(0.)).overflow_hidden().child(
            div()
                .id("settings-scroll")
                .size_full()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_4()
                .p_3()
                .child(units)
                .child(hardware)
                .child(visibility),
        )
    }

    /// The first live hardware temperature, as a curve source — the
    /// default for new curves and the fallback when a custom sensor that
    /// curves read from is deleted.
    pub(super) fn first_temp_source(&self) -> Option<CurveSource> {
        let UiState::Service(ServiceState::Ready {
            chips, snapshots, ..
        }) = &self.state
        else {
            return None;
        };
        for (ci, snapshot) in snapshots.iter().enumerate() {
            let Some(info) = chips.get(ci) else { continue };
            for (ti, temp) in snapshot.temps.iter().enumerate() {
                if temp.is_some() {
                    return Some(CurveSource::Temp {
                        chip: info.name.clone(),
                        temp: ti + 1,
                    });
                }
            }
        }
        None
    }

    /// Deletes a custom sensor; curves that read it are repointed to the
    /// first hardware temperature so they keep working.
    pub(super) fn delete_custom_sensor(&mut self, id: &str, cx: &mut Context<Self>) {
        let fallback = self.first_temp_source();
        for def in self.names.curves().to_vec() {
            let reads_it = matches!(&def.source, CurveSource::Custom { custom } if custom == id);
            if reads_it && let Some(source) = fallback.clone() {
                let mut def = def;
                def.source = source;
                config::save_curve(&def);
            }
        }
        config::delete_custom(id);
        self.reload_config(cx);
    }

    /// Small confirmation modal for deleting a curve or a custom sensor.
    pub(super) fn render_confirm_delete(
        &self,
        target: &ConfirmDelete,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let (name, message) = match target {
            ConfirmDelete::Curve(id) => (
                self.names
                    .curves()
                    .iter()
                    .find(|def| &def.id == id)?
                    .name
                    .clone(),
                "Fans driven by this curve return to UEFI/Firmware control.",
            ),
            ConfirmDelete::Custom(id) => (
                self.names
                    .customs()
                    .iter()
                    .find(|def| &def.id == id)?
                    .name
                    .clone(),
                "Curves using this sensor switch to a hardware temperature.",
            ),
        };
        let target = target.clone();
        let panel = self
            .modal_panel("confirm-delete-dialog", px(380.), cx)
            .overflow_y_scroll()
            .gap_3()
            .p_4()
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(format!("Delete “{name}”?")),
            )
            .child(div().text_sm().text_color(rgb(TEXT_DIM)).child(message))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        self.button("confirm-delete-cancel", "Cancel", cx, |this, cx| {
                            this.confirm_delete = None;
                            cx.notify();
                        }),
                    )
                    .child(
                        div().child(
                            div()
                                .id("confirm-delete-go")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .bg(rgb(ERROR))
                                .text_sm()
                                .text_color(rgb(BG))
                                .cursor_pointer()
                                .hover(|s| s.opacity(0.85))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.confirm_delete = None;
                                    match &target {
                                        ConfirmDelete::Curve(id) => {
                                            this.delete_curve(&id.clone(), cx)
                                        }
                                        ConfirmDelete::Custom(id) => {
                                            this.delete_custom_sensor(&id.clone(), cx)
                                        }
                                    }
                                }))
                                .child("Delete"),
                        ),
                    ),
            );

        Some(self.modal_backdrop(panel, cx, |this, cx| {
            this.confirm_delete = None;
            cx.notify();
        }))
    }
}

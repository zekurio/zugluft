use super::*;

pub(super) const TOAST_TICKS: u32 = 30;

pub(super) struct DashboardDeviceGroup {
    pub(super) title: String,
    pub(super) cards: Vec<Div>,
}

#[derive(Default)]
pub(super) struct DashboardGroups {
    pub(super) fans: Vec<DashboardDeviceGroup>,
    pub(super) curves: Vec<Div>,
    pub(super) sensors: Vec<DashboardDeviceGroup>,
}

impl Zugluft {
    pub(super) fn dashboard_fan_item(&self, chip: &str, index: usize) -> config::DashboardItem {
        config::DashboardItem::fan(chip, format!("fan{}", index + 1))
    }

    pub(super) fn dashboard_sensor_item(&self, sensor: &SensorReading) -> config::DashboardItem {
        config::DashboardItem::sensor(sensor.chip_name.clone(), channel_key(sensor.key))
    }

    pub(super) fn set_dashboard_pinned(
        &mut self,
        item: config::DashboardItem,
        pinned: bool,
        cx: &mut Context<Self>,
    ) {
        config::set_dashboard_pinned(&item, pinned);
        self.reload_config(cx);
        self.show_toast(
            if pinned {
                "Pinned to dashboard"
            } else {
                "Unpinned from dashboard"
            },
            cx,
        );
    }

    pub(super) fn render_dashboard_sections(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let groups = self.render_dashboard_groups(chips, snapshots, customs, cx);
        let mut sections = Vec::new();

        if !groups.fans.is_empty() {
            sections.push(self.render_dashboard_device_section("Fans", groups.fans));
        }
        if !groups.curves.is_empty() {
            sections.push(self.render_dashboard_card_section("Curves", groups.curves));
        }
        if !groups.sensors.is_empty() {
            sections.push(self.render_dashboard_device_section("Sensors", groups.sensors));
        }

        sections
    }

    fn render_dashboard_groups(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> DashboardGroups {
        let sensors = self.sensor_readings(chips, snapshots, customs);
        let mut groups = DashboardGroups::default();

        for item in self.names.dashboard_items() {
            match item.kind() {
                config::DashboardItemKind::Fan => {
                    if let Some((device, card)) =
                        self.render_dashboard_fan(item, chips, snapshots, cx)
                    {
                        push_device_card(&mut groups.fans, device, card);
                    }
                }
                config::DashboardItemKind::Curve => {}
                config::DashboardItemKind::Sensor => {
                    if let Some((device, card)) = self.render_dashboard_sensor(item, &sensors, cx) {
                        push_device_card(&mut groups.sensors, device, card);
                    }
                }
            }
        }

        groups.curves.extend(
            self.names.curves().iter().enumerate().map(|(index, def)| {
                self.render_curve_card(index, def, chips, snapshots, customs, cx)
            }),
        );

        groups
    }

    fn render_dashboard_card_section(&self, title: &'static str, cards: Vec<Div>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(dashboard_section_header(title))
            .child(dashboard_card_row(cards))
    }

    fn render_dashboard_device_section(
        &self,
        title: &'static str,
        groups: Vec<DashboardDeviceGroup>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(dashboard_section_header(title))
            .children(groups.into_iter().map(|group| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(dashboard_device_header(group.title))
                    .child(dashboard_card_row(group.cards))
            }))
    }

    pub(super) fn show_toast(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.toast = Some(Toast {
            message: message.into(),
            shown_tick: self.refresh_ticks,
        });
        cx.notify();
    }

    pub(super) fn render_toast(&self) -> Option<Div> {
        let toast = self.toast.as_ref()?;
        Some(
            div()
                .absolute()
                .right(px(18.))
                .top(px(54.))
                .max_w(px(280.))
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .rounded_lg()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(FILL_MANUAL))
                .shadow(floating_shadow())
                .text_sm()
                .text_color(rgb(TEXT))
                .child(
                    svg()
                        .path("icons/pin.svg")
                        .w(px(14.))
                        .h(px(14.))
                        .flex_none()
                        .text_color(rgb(FILL_MANUAL)),
                )
                .child(div().min_w(px(0.)).truncate().child(toast.message.clone())),
        )
    }

    fn render_dashboard_fan(
        &self,
        item: &config::DashboardItem,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        cx: &mut Context<Self>,
    ) -> Option<(String, Div)> {
        let chip_name = item.chip()?;
        let channel = item.channel()?;
        let fan_index = channel_index(channel, "fan")?;
        let chip_index = chips.iter().position(|chip| chip.name == chip_name)?;
        if self.names.is_hidden(chip_name, channel) {
            return None;
        }
        let fan = snapshots.get(chip_index)?.fans.get(fan_index)?;
        if fan.rpm.is_none() && fan.duty.is_none() {
            return None;
        }
        let name = self.names.fan_label(chip_name, fan_index);
        let device = self.names.device_label(chip_name);
        Some((
            device,
            self.render_fan_card((chip_index, fan_index), chip_name, name, fan, cx),
        ))
    }

    fn render_dashboard_sensor(
        &self,
        item: &config::DashboardItem,
        sensors: &[SensorReading],
        cx: &mut Context<Self>,
    ) -> Option<(String, Div)> {
        let chip = item.chip()?;
        let channel = item.channel()?;
        sensors
            .iter()
            .find(|sensor| {
                matches!(
                    sensor.key.kind,
                    SensorKind::Temperature | SensorKind::Power | SensorKind::Custom
                ) && sensor.chip_name == chip
                    && channel_key(sensor.key) == channel
            })
            .map(|sensor| {
                let device = match sensor.key.kind {
                    SensorKind::Custom => "Custom".to_string(),
                    _ => self.names.device_label(&sensor.chip_name),
                };
                (device, self.render_dashboard_sensor_card(sensor, cx))
            })
    }
}

fn push_device_card(groups: &mut Vec<DashboardDeviceGroup>, title: String, card: Div) {
    if let Some(group) = groups.iter_mut().find(|group| group.title == title) {
        group.cards.push(card);
    } else {
        groups.push(DashboardDeviceGroup {
            title,
            cards: vec![card],
        });
    }
}

fn dashboard_section_header(title: &'static str) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT))
        .child(title)
}

fn dashboard_device_header(title: String) -> Div {
    div()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(TEXT_DIM))
        .child(title)
}

fn dashboard_card_row(cards: Vec<Div>) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_start()
        .gap_2()
        .children(cards)
}

fn channel_index(channel: &str, prefix: &str) -> Option<usize> {
    channel
        .strip_prefix(prefix)?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
}

use super::*;

impl Zugluft {
    pub(super) fn dashboard_fan_item(&self, chip: &str, index: usize) -> config::DashboardItem {
        config::DashboardItem::fan(chip, format!("fan{}", index + 1))
    }

    pub(super) fn dashboard_sensor_item(&self, sensor: &SensorReading) -> config::DashboardItem {
        config::DashboardItem::sensor(sensor.chip_name.clone(), channel_key(sensor.key))
    }

    pub(super) fn dashboard_curve_item(&self, def: &CurveDef) -> config::DashboardItem {
        config::DashboardItem::curve(def.id.clone())
    }

    pub(super) fn set_dashboard_pinned(
        &mut self,
        item: config::DashboardItem,
        pinned: bool,
        cx: &mut Context<Self>,
    ) {
        config::set_dashboard_pinned(&item, pinned);
        self.reload_config(cx);
    }

    pub(super) fn dashboard_pin_button(
        &self,
        id: impl Into<ElementId>,
        item: config::DashboardItem,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let pinned = self.names.is_dashboard_pinned(&item);
        let label = if pinned {
            "Unpin from dashboard"
        } else {
            "Pin to dashboard"
        };
        let color = if pinned { FILL_MANUAL } else { TEXT_DIM };
        let hover_color = if pinned { ERROR } else { TEXT };
        div()
            .id(id)
            .flex_none()
            .p(px(2.))
            .rounded_sm()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(FILL_HOVER)))
            .tooltip(move |_, cx| cx.new(move |_| PinTooltip { label }).into())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.set_dashboard_pinned(item.clone(), !pinned, cx);
            }))
            .child(
                svg()
                    .path("icons/pin.svg")
                    .w(px(12.))
                    .h(px(12.))
                    .text_color(rgb(color))
                    .hover(move |s| s.text_color(rgb(hover_color))),
            )
    }

    pub(super) fn render_dashboard_cards(
        &self,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let sensors = self.sensor_readings(chips, snapshots, customs);
        self.names
            .dashboard_items()
            .iter()
            .filter_map(|item| {
                self.render_dashboard_item(item, chips, snapshots, customs, &sensors, cx)
            })
            .collect()
    }

    fn render_dashboard_item(
        &self,
        item: &config::DashboardItem,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        sensors: &[SensorReading],
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        match item.kind() {
            config::DashboardItemKind::Fan => self.render_dashboard_fan(item, chips, snapshots, cx),
            config::DashboardItemKind::Sensor => self.render_dashboard_sensor(item, sensors, cx),
            config::DashboardItemKind::Curve => {
                self.render_dashboard_curve(item, chips, snapshots, customs, cx)
            }
        }
    }

    fn render_dashboard_fan(
        &self,
        item: &config::DashboardItem,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        cx: &mut Context<Self>,
    ) -> Option<Div> {
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
        Some(self.render_fan_card((chip_index, fan_index), chip_name, name, fan, cx))
    }

    fn render_dashboard_sensor(
        &self,
        item: &config::DashboardItem,
        sensors: &[SensorReading],
        cx: &mut Context<Self>,
    ) -> Option<Div> {
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
            .map(|sensor| self.render_dashboard_sensor_card(sensor, cx))
    }

    fn render_dashboard_curve(
        &self,
        item: &config::DashboardItem,
        chips: &[ChipInfo],
        snapshots: &[ChipSnapshot],
        customs: &[CustomSensorValue],
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let id = item.id()?;
        let index = self.names.curves().iter().position(|def| def.id == id)?;
        let def = self.names.curves().get(index)?;
        Some(self.render_curve_card(index, def, chips, snapshots, customs, cx))
    }
}

fn channel_index(channel: &str, prefix: &str) -> Option<usize> {
    channel
        .strip_prefix(prefix)?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
}

struct PinTooltip {
    label: &'static str,
}

impl Render for PinTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow(subtle_shadow())
            .text_xs()
            .text_color(rgb(TEXT))
            .child(self.label)
    }
}

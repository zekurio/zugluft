use super::*;

pub(super) fn publish_ready(
    hub: &Hub,
    hw: &Hardware,
    customs: &[CustomSensorDef],
    curve_defs: &[CurveDef],
) {
    let custom_values = custom::compute(customs, &hw.chips, &hw.snapshots);
    let curve_values = curves::statuses(curve_defs, &hw.chips, &hw.snapshots, &custom_values);
    hub.publish(ServiceState::Ready {
        chips: hw.chips.clone(),
        snapshots: hw.snapshots.clone(),
        notes: hw.notes.clone(),
        customs: custom_values,
        curves: curve_values,
    });
}

/// Converts and stores fresh snapshots, stamping each fan with its
/// calibration results and user settings.
pub(super) fn store_snapshots(hw: &mut Hardware, snaps: &[zugluft_hw::ChipSnapshot]) {
    hw.snapshots = snaps.iter().map(convert_snapshot).collect();
    stamp_fans(hw);
}

/// Merges calibration results and user settings into the stored snapshots.
pub(super) fn stamp_fans(hw: &mut Hardware) {
    for (ci, snapshot) in hw.snapshots.iter_mut().enumerate() {
        for (fi, fan) in snapshot.fans.iter_mut().enumerate() {
            let calibration = hw
                .curves
                .get(ci)
                .and_then(|curves| curves.get(fi))
                .and_then(Option::as_ref);
            let settings = hw
                .settings
                .get(ci)
                .and_then(|fans| fans.get(fi))
                .copied()
                .unwrap_or_default();
            fan.target_percent = hw.requested.get(&(ci, fi)).map(|&duty| {
                effective_target_percent(settings, calibration, duty_to_percent(duty))
            });
            fan.max_rpm = calibration.map(|curve| curve.max_rpm);
            fan.min_percent = calibration.and_then(FanCurve::minimum_speed_percent);
            fan.stop_percent = calibration.and_then(|curve| curve.stop_duty.map(duty_to_percent));
            fan.start_percent = calibration.and_then(|curve| curve.start_duty.map(duty_to_percent));
            fan.settings = settings;
            fan.curve = hw
                .assignments
                .get(ci)
                .and_then(|fans| fans.get(fi))
                .cloned()
                .flatten();
        }
    }
}

pub(super) fn duty_to_percent(duty: u8) -> f32 {
    duty as f32 * 100.0 / 255.0
}

pub(super) fn percent_to_duty(percent: f32) -> u8 {
    (percent.clamp(0.0, 100.0) * 255.0 / 100.0).round() as u8
}

pub(super) fn settings_from_store(
    store: &settings::Store,
    chips: &[ipc::ChipInfo],
) -> Vec<Vec<FanSettings>> {
    chips
        .iter()
        .map(|info| {
            let key = calibration::chip_key(info);
            (0..info.fan_count).map(|fi| store.get(&key, fi)).collect()
        })
        .collect()
}

pub(super) fn assignments_from_store(
    store: &curves::Assignments,
    chips: &[ipc::ChipInfo],
) -> Vec<Vec<Option<String>>> {
    chips
        .iter()
        .map(|info| {
            let key = calibration::chip_key(info);
            (0..info.fan_count).map(|fi| store.get(&key, fi)).collect()
        })
        .collect()
}

pub(super) fn curves_from_store(
    store: &Store,
    chips: &[ipc::ChipInfo],
) -> Vec<Vec<Option<FanCurve>>> {
    chips
        .iter()
        .map(|info| {
            let key = calibration::chip_key(info);
            (0..info.fan_count)
                .map(|fi| store.curve(&key, fi).cloned())
                .collect()
        })
        .collect()
}

pub(super) fn convert_info(info: &zugluft_hw::ChipInfo) -> ipc::ChipInfo {
    ipc::ChipInfo {
        name: info.name.clone(),
        address: info.address,
        version: info.version,
        slot: info.slot,
        fan_count: info.fan_count,
        control_count: info.control_count,
        temp_count: info.temp_count,
        temp_labels: info.temp_labels.clone(),
        power_labels: info.power_labels.clone(),
    }
}

pub(super) fn convert_snapshot(snapshot: &zugluft_hw::ChipSnapshot) -> ipc::ChipSnapshot {
    ipc::ChipSnapshot {
        fans: snapshot
            .fans
            .iter()
            .map(|fan| ipc::FanStatus {
                rpm: fan.rpm,
                target_percent: None,
                duty: fan.duty.map(|duty| match duty {
                    zugluft_hw::FanDuty::Auto => ipc::FanDuty::Auto,
                    zugluft_hw::FanDuty::Manual { percent } => ipc::FanDuty::Manual { percent },
                }),
                max_rpm: None, // calibration + settings stamped in stamp_fans
                min_percent: None,
                stop_percent: None,
                start_percent: None,
                settings: ipc::FanSettings::default(),
                curve: None,
            })
            .collect(),
        temps: snapshot.temps.clone(),
        powers: snapshot.powers.clone(),
    }
}

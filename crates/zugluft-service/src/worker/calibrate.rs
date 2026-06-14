use super::*;
use std::collections::VecDeque;

/// Calibration command ladder, descending to a full stop: the upper steps map
/// the RPM response (and 100 % is the max-RPM reference), the fine-grained
/// lower steps find where each fan stalls.
pub(super) const CALIBRATION_DUTIES: [u8; 9] = [255, 204, 153, 102, 51, 38, 26, 13, 0];
/// Ascending probe for the restart duty of fans that stalled; capped at
/// 40 % — a fan that needs more than that to start is treated as unknown.
pub(super) const CALIBRATION_START_DUTIES: [u8; 8] = [13, 26, 38, 51, 64, 77, 89, 102];
/// Spin-up/down time before sampling a normal calibration step.
pub(super) const CALIBRATION_SETTLE: Duration = Duration::from_secs(15);
/// Longer wait for low-duty and restart probes, where fan inertia and tach
/// smoothing most often produce misleading transient readings.
pub(super) const CALIBRATION_LOW_DUTY_SETTLE: Duration = Duration::from_secs(25);
/// Longest wait for the full stop test.
pub(super) const CALIBRATION_STOP_SETTLE: Duration = Duration::from_secs(30);
/// Upper bound of samples per step if readings never stabilize.
pub(super) const CALIBRATION_MAX_SAMPLES: usize = 72;
/// Consecutive readings within this fraction (or 20 rpm) required per fan.
pub(super) const CALIBRATION_STABLE_READINGS: u8 = 8;
/// Recent stable readings averaged into the stored RPM for each step.
pub(super) const CALIBRATION_AVERAGE_READINGS: usize = 8;
/// Readings within this fraction (or 20 rpm) count as stable.
pub(super) const CALIBRATION_STABLE_DELTA: f32 = 0.01;
pub(super) const CALIBRATION_STABLE_RPM_DELTA: f32 = 20.0;
/// Steps at or below this duty watch the temperature guard.
pub(super) const CALIBRATION_GUARD_DUTY: u8 = 51;
/// Any sensor reaching this during a guarded step aborts the low-duty
/// phases and restores the previous duties.
pub(super) const CALIBRATION_ABORT_TEMP: f32 = 85.0;
/// Note shown to clients after a calibration run found its duty writes
/// being overwritten.
pub(super) const CONFLICT_NOTE: &str = "calibration aborted: another fan-control tool is adjusting fan duties — close it and recalibrate";

pub(super) enum CalEnd {
    Done,
    Shutdown,
    Redetect,
}

/// Steps every controllable fan through [`CALIBRATION_DUTIES`] down to a
/// full stop, records the stabilized RPM per step, then ramps stalled fans
/// back up to find their restart command. Curves are persisted and the
/// previous targets restored. Commands arriving mid-run are honored: SetDuty
/// updates the restore plan, Shutdown/Redetect abort (the dropped session
/// then restores fan control).
#[allow(clippy::too_many_arguments)]
pub(super) fn run_calibration(
    hub: &Hub,
    hw: &mut Hardware,
    rx: &Receiver<Command>,
    store: &mut Store,
    manual_store: &mut manual::Store,
    customs: &mut Vec<CustomSensorDef>,
    curve_defs: &mut Vec<CurveDef>,
    overrides: &HashMap<(usize, usize), Option<u8>>,
) -> CalEnd {
    // Calibration owns the duties for the duration of the run.
    hw.ramps.clear();

    // What each fan returns to afterwards: the encoded target the user last
    // requested (pre offset/minimum/calibration), falling back to the latest
    // command readback, or whatever SetDuty requests arrive before/during the
    // run.
    let mut restore: HashMap<(usize, usize), Option<u8>> = HashMap::new();
    for (ci, info) in hw.chips.iter().enumerate() {
        for fi in 0..info.control_count {
            let target = match hw.requested.get(&(ci, fi)) {
                Some(&raw) => Some(raw),
                None => {
                    let snapshot_duty = hw
                        .snapshots
                        .get(ci)
                        .and_then(|snap| snap.fans.get(fi))
                        .and_then(|fan| fan.duty);
                    match snapshot_duty {
                        Some(ipc::FanDuty::Manual { percent }) => {
                            Some((percent * 255.0 / 100.0).round() as u8)
                        }
                        _ => None,
                    }
                }
            };
            let target = overrides.get(&(ci, fi)).copied().unwrap_or(target);
            restore.insert((ci, fi), target);
        }
    }
    if restore.is_empty() {
        log_line("calibration skipped: no controllable fans");
        return CalEnd::Done;
    }

    log_line("calibration started");
    let mut samples: HashMap<(usize, usize), Vec<(u8, f32)>> = HashMap::new();
    let mut too_hot = false;
    let mut conflict = false;

    for (step, &duty) in CALIBRATION_DUTIES.iter().enumerate() {
        hub.publish(ServiceState::Calibrating {
            message: format!(
                "Step {}/{}: all fans at {:.0} % duty",
                step + 1,
                CALIBRATION_DUTIES.len(),
                duty as f32 * 100.0 / 255.0
            ),
        });

        let readings = match calibration_step(hw, rx, &mut restore, customs, curve_defs, duty) {
            Ok(readings) => readings,
            Err(end) => return end,
        };
        if readings.conflict_samples > 0 {
            conflict = true;
            break;
        }
        for (&key, &rpm) in &readings.rpms {
            samples.entry(key).or_default().push((duty, rpm));
        }
        if readings.never_stabilized {
            log_line(&format!("calibration: step {} never stabilized", step + 1));
        }
        if duty <= CALIBRATION_GUARD_DUTY && readings.max_temp >= CALIBRATION_ABORT_TEMP {
            log_line(&format!(
                "calibration: temperature guard tripped at {:.0} °C, aborting low-duty phase",
                readings.max_temp
            ));
            too_hot = true;
            break;
        }
    }

    // A fan stopped at the highest descending duty that read a stable
    // 0 rpm — only meaningful for fans that actually spun further up.
    let mut stop_duties: HashMap<(usize, usize), u8> = HashMap::new();
    for (&key, points) in &samples {
        if !points.iter().any(|&(_, rpm)| rpm > 0.0) {
            continue; // empty header / no tach
        }
        if let Some(&(duty, _)) = points.iter().find(|&&(_, rpm)| rpm < 1.0) {
            stop_duties.insert(key, duty);
        }
    }

    // Ramp stalled fans back up to find the duty that restarts them
    // (higher than the stall duty because of motor hysteresis).
    let mut start_duties: HashMap<(usize, usize), u8> = HashMap::new();
    if !too_hot && !conflict && !stop_duties.is_empty() {
        for &duty in &CALIBRATION_START_DUTIES {
            hub.publish(ServiceState::Calibrating {
                message: format!(
                    "Restart probe: stopped fans at {:.0} % duty",
                    duty as f32 * 100.0 / 255.0
                ),
            });
            let readings = match calibration_step(hw, rx, &mut restore, customs, curve_defs, duty) {
                Ok(readings) => readings,
                Err(end) => return end,
            };
            if readings.conflict_samples > 0 {
                conflict = true;
                break;
            }
            for &key in stop_duties.keys() {
                if !start_duties.contains_key(&key)
                    && readings.rpms.get(&key).copied().unwrap_or(0.0) > 0.0
                {
                    start_duties.insert(key, duty);
                }
            }
            if start_duties.len() == stop_duties.len() {
                break;
            }
            if readings.max_temp >= CALIBRATION_ABORT_TEMP {
                log_line("calibration: temperature guard tripped during restart probe");
                break;
            }
        }
        for key in stop_duties.keys() {
            if !start_duties.contains_key(key) {
                let max_probe_duty = CALIBRATION_START_DUTIES.last().copied().unwrap_or(0);
                log_line(&format!(
                    "calibration: fan {key:?} stopped but did not restart by {:.0} % duty",
                    max_probe_duty as f32 * 100.0 / 255.0
                ));
            }
        }
    }

    // Restore goes through request_duty so offset/minimum/calibration apply
    // the same way they would to a fresh client request.
    let mut manual_dirty = false;
    for (&(ci, fi), &target) in &restore {
        if let Some(info) = hw.chips.get(ci) {
            manual_store.set(&calibration::chip_key(info), fi, target);
            manual_dirty = true;
        }
        request_duty(hw, ci, fi, target);
    }
    if manual_dirty {
        manual_store.save();
    }

    // Another tool kept overwriting our duties; the readings are garbage,
    // so keep whatever was calibrated before and tell the user.
    if conflict {
        if !hw.notes.iter().any(|note| note == CONFLICT_NOTE) {
            hw.notes.push(CONFLICT_NOTE.to_string());
        }
        if let Ok(snaps) = hw.session.update() {
            store_snapshots(hw, &snaps);
        }
        log_line("calibration aborted: another tool is adjusting fan duties");
        return CalEnd::Done;
    }
    hw.notes.retain(|note| note != CONFLICT_NOTE);

    for (ci, info) in hw.chips.iter().enumerate() {
        let key = calibration::chip_key(info);
        for fi in 0..info.control_count {
            let points = samples.remove(&(ci, fi)).unwrap_or_default();
            // Full duty is the display/control reference. If that sample is
            // missing but later points exist, fall back to the highest seen
            // RPM rather than throwing the whole calibration away.
            let max_rpm = points
                .iter()
                .find_map(|&(duty, rpm)| (duty == u8::MAX && rpm > 0.0).then_some(rpm))
                .unwrap_or_else(|| points.iter().map(|&(_, rpm)| rpm).fold(0.0, f32::max));
            if max_rpm > 0.0 {
                store.insert(
                    &key,
                    fi,
                    FanCurve {
                        max_rpm,
                        points,
                        stop_duty: stop_duties.get(&(ci, fi)).copied(),
                        start_duty: start_duties.get(&(ci, fi)).copied(),
                    },
                );
            } else {
                // No tach response even at full duty (empty header) —
                // drop any stale curve from a previous run.
                store.remove(&key, fi);
            }
        }
    }
    store.save();
    hw.curves = curves_from_store(store, &hw.chips);
    if let Ok(snaps) = hw.session.update() {
        store_snapshots(hw, &snaps);
    }
    log_line("calibration finished");
    CalEnd::Done
}

pub(super) struct StepReadings {
    /// Stabilized RPM per fan; stalled fans read 0, tach-less fans are
    /// absent.
    rpms: HashMap<(usize, usize), f32>,
    /// Hottest sensor seen while sampling, for the temperature guard.
    max_temp: f32,
    /// Samples where some fan's read-back duty was not the one we set —
    /// another tool is driving the fans.
    conflict_samples: usize,
    never_stabilized: bool,
}

/// Sets every fan in `restore` to `duty`, lets the RPM settle, then samples
/// until each reporting tach has a stable window to average. Tach-less fans
/// never report and don't block stability.
pub(super) fn calibration_step(
    hw: &mut Hardware,
    rx: &Receiver<Command>,
    restore: &mut HashMap<(usize, usize), Option<u8>>,
    customs: &mut Vec<CustomSensorDef>,
    curve_defs: &mut Vec<CurveDef>,
    duty: u8,
) -> Result<StepReadings, CalEnd> {
    for &(ci, fi) in restore.keys() {
        if let Err(error) = hw.session.set_fan(ci, fi, Some(duty)) {
            log_line(&format!("calibration set_fan({ci},{fi}) failed: {error}"));
        }
    }

    let expected = duty_to_percent(duty);
    let mut max_temp = 0.0f32;
    let settle_deadline = Instant::now() + calibration_settle_for(duty);
    loop {
        let wait = settle_deadline.saturating_duration_since(Instant::now());
        if wait.is_zero() {
            break;
        }
        if let Some(end) = wait_pumping(rx, restore, customs, curve_defs, wait.min(POLL_INTERVAL)) {
            return Err(end);
        }
        if let Ok(snaps) = hw.session.update() {
            record_max_temp(&snaps, &mut max_temp);
            if duty <= CALIBRATION_GUARD_DUTY && max_temp >= CALIBRATION_ABORT_TEMP {
                return Ok(StepReadings {
                    rpms: HashMap::new(),
                    max_temp,
                    conflict_samples: 0,
                    never_stabilized: false,
                });
            }
        }
    }

    let mut last: HashMap<(usize, usize), f32> = HashMap::new();
    let mut stable_counts: HashMap<(usize, usize), u8> = HashMap::new();
    let mut average_windows: HashMap<(usize, usize), VecDeque<f32>> = HashMap::new();
    let mut conflict_samples = 0;
    let mut never_stabilized = false;
    for sample in 0..CALIBRATION_MAX_SAMPLES {
        if let Some(end) = wait_pumping(rx, restore, customs, curve_defs, POLL_INTERVAL) {
            return Err(end);
        }
        let Ok(snaps) = hw.session.update() else {
            continue; // transient (e.g. bus mutex contention)
        };
        record_max_temp(&snaps, &mut max_temp);
        let mut stable = false;
        let mut conflict = false;
        let mut current = HashMap::new();
        for &(ci, fi) in restore.keys() {
            let status = snaps.get(ci).and_then(|snap| snap.fans.get(fi));
            // We wrote `duty`; reading anything else back means another
            // tool overwrote it.
            conflict |= match status.and_then(|fan| fan.duty) {
                Some(zugluft_hw::FanDuty::Manual { percent }) => (percent - expected).abs() > 2.0,
                Some(zugluft_hw::FanDuty::Auto) => true,
                None => false,
            };
            let Some(rpm) = status.and_then(|fan| fan.rpm) else {
                continue;
            };
            let key = (ci, fi);
            let count = match last.get(&key) {
                Some(&prev)
                    if (rpm - prev).abs()
                        <= (prev.max(rpm) * CALIBRATION_STABLE_DELTA)
                            .max(CALIBRATION_STABLE_RPM_DELTA) =>
                {
                    stable_counts
                        .get(&(ci, fi))
                        .copied()
                        .unwrap_or(1)
                        .saturating_add(1)
                }
                _ => 1,
            };
            stable_counts.insert(key, count);
            let window = average_windows.entry(key).or_default();
            if count == 1 {
                window.clear();
            }
            window.push_back(rpm);
            while window.len() > CALIBRATION_AVERAGE_READINGS {
                window.pop_front();
            }
            current.insert(key, rpm);
        }
        conflict_samples += conflict as usize;
        if !current.is_empty() {
            stable = current.keys().all(|key| {
                stable_counts.get(key).copied().unwrap_or(0) >= CALIBRATION_STABLE_READINGS
                    && average_windows
                        .get(key)
                        .is_some_and(|readings| readings.len() >= CALIBRATION_AVERAGE_READINGS)
            });
        }
        if stable {
            last = averaged_readings(&average_windows, &current);
            break;
        }
        never_stabilized = sample + 1 == CALIBRATION_MAX_SAMPLES;
        if never_stabilized {
            last = averaged_readings(&average_windows, &current);
        } else {
            last = current;
        }
    }

    Ok(StepReadings {
        rpms: last,
        max_temp,
        conflict_samples,
        never_stabilized,
    })
}

fn calibration_settle_for(duty: u8) -> Duration {
    if duty == 0 {
        CALIBRATION_STOP_SETTLE
    } else if duty <= CALIBRATION_GUARD_DUTY {
        CALIBRATION_LOW_DUTY_SETTLE
    } else {
        CALIBRATION_SETTLE
    }
}

fn record_max_temp(snaps: &[zugluft_hw::ChipSnapshot], max_temp: &mut f32) {
    for temp in snaps.iter().flat_map(|snap| snap.temps.iter().flatten()) {
        *max_temp = (*max_temp).max(*temp);
    }
}

fn averaged_readings(
    windows: &HashMap<(usize, usize), VecDeque<f32>>,
    current: &HashMap<(usize, usize), f32>,
) -> HashMap<(usize, usize), f32> {
    current
        .iter()
        .filter_map(|(&key, &fallback)| {
            let readings = windows.get(&key)?;
            let count = readings.len();
            if count == 0 {
                return Some((key, fallback));
            }
            let sum: f32 = readings.iter().sum();
            Some((key, sum / count as f32))
        })
        .collect()
}

/// Sleeps for `duration` while handling queued commands: SetDuty lands in
/// the restore plan instead of fighting the ladder, custom sensor updates
/// are applied directly; Shutdown and Redetect abort the run.
pub(super) fn wait_pumping(
    rx: &Receiver<Command>,
    restore: &mut HashMap<(usize, usize), Option<u8>>,
    customs: &mut Vec<CustomSensorDef>,
    curve_defs: &mut Vec<CurveDef>,
    duration: Duration,
) -> Option<CalEnd> {
    let deadline = Instant::now() + duration;
    loop {
        let timeout = deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            return None;
        }
        match rx.recv_timeout(timeout) {
            Ok(Command::Shutdown) => return Some(CalEnd::Shutdown),
            Ok(Command::Request(Request::Redetect)) => return Some(CalEnd::Redetect),
            Ok(Command::Request(Request::SetDuty { chip, fan, duty })) => {
                if restore.contains_key(&(chip, fan)) {
                    restore.insert((chip, fan), duty);
                }
            }
            Ok(Command::Request(Request::SetCustomSensors(defs))) => {
                *customs = defs;
                custom::save(customs);
            }
            // Definitions apply once polling resumes; the ladder owns the
            // duties until then.
            Ok(Command::Request(Request::SetCurves(defs))) => {
                *curve_defs = defs;
                curves::save_defs(curve_defs);
            }
            // Ignored while the ladder runs; the client can resend after.
            Ok(Command::Request(Request::SetFanSettings { .. })) => {}
            Ok(Command::Request(Request::SetFanCurve { .. })) => {}
            Ok(Command::Request(Request::Calibrate)) => {} // already running
            Err(RecvTimeoutError::Timeout) => return None,
            Err(RecvTimeoutError::Disconnected) => return Some(CalEnd::Shutdown),
        }
    }
}

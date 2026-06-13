use super::*;

/// Returns true on shutdown.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_command(
    command: Command,
    pending: &mut HashMap<(usize, usize), Option<u8>>,
    redetect: &mut bool,
    calibrate: &mut bool,
    customs_update: &mut Option<Vec<CustomSensorDef>>,
    settings_updates: &mut Vec<(usize, usize, FanSettings)>,
    curves_update: &mut Option<Vec<CurveDef>>,
    assign_updates: &mut Vec<(usize, usize, Option<String>)>,
) -> bool {
    match command {
        Command::Shutdown => true,
        Command::Request(Request::SetDuty { chip, fan, duty }) => {
            pending.insert((chip, fan), duty);
            false
        }
        Command::Request(Request::Calibrate) => {
            *calibrate = true;
            false
        }
        Command::Request(Request::SetCustomSensors(defs)) => {
            *customs_update = Some(defs);
            false
        }
        Command::Request(Request::SetFanSettings {
            chip,
            fan,
            settings,
        }) => {
            settings_updates.push((chip, fan, settings));
            false
        }
        Command::Request(Request::SetCurves(defs)) => {
            *curves_update = Some(defs);
            false
        }
        Command::Request(Request::SetFanCurve { chip, fan, curve }) => {
            assign_updates.push((chip, fan, curve));
            false
        }
        Command::Request(Request::Redetect) => {
            *redetect = true;
            false
        }
    }
}

/// Drives every curve-assigned fan to its curve's current output. Writes go
/// through [`request_duty`] so per-fan settings and calibration shape curve
/// targets exactly like manual ones; unchanged final commands are skipped. A
/// fan whose assigned curve has vanished from the config is handed back to
/// automatic control once, so it can't sit pinned at a stale command; a curve
/// whose source reads `None` holds the fan's last command (sensor dropouts are
/// usually transient).
pub(super) fn apply_curves(
    hw: &mut Hardware,
    defs: &[CurveDef],
    customs: &[CustomSensorDef],
) -> bool {
    let assigned: Vec<(usize, usize, String)> = hw
        .assignments
        .iter()
        .enumerate()
        .flat_map(|(ci, fans)| {
            fans.iter()
                .enumerate()
                .filter_map(move |(fi, id)| id.clone().map(|id| (ci, fi, id)))
        })
        .collect();
    if assigned.is_empty() {
        return false;
    }

    let custom_values = custom::compute(customs, &hw.chips, &hw.snapshots);
    let mut wrote = false;
    let now = Instant::now();
    for (ci, fi, id) in assigned {
        let Some(def) = defs.iter().find(|def| def.id == id) else {
            if hw.curve_written.remove(&(ci, fi)).is_some() {
                log_line(&format!(
                    "curve '{id}' is gone, fan ({ci},{fi}) back to auto"
                ));
                wrote |= request_duty(hw, ci, fi, None);
            }
            clear_curve_runtime(hw, ci, fi);
            continue;
        };
        let Some(input) = def.source.resolve(&hw.chips, &hw.snapshots, &custom_values) else {
            continue;
        };
        let Some(percent) = apply_curve_functions(&mut hw.curve_runtime, (ci, fi), def, input, now)
        else {
            continue;
        };
        let request = percent_to_duty(percent);
        let command = command_for_request(hw, ci, fi, request);
        if hw.curve_written.insert((ci, fi), command) != Some(command) {
            wrote |= request_duty(hw, ci, fi, Some(request));
        } else {
            hw.requested.insert((ci, fi), request);
        }
    }
    wrote
}

fn apply_curve_functions(
    states: &mut HashMap<CurveFunctionKey, CurveRuntime>,
    fan_key: (usize, usize),
    def: &CurveDef,
    input: f32,
    now: Instant,
) -> Option<f32> {
    let mut input = input;
    let mut target = def.kind.evaluate(input)?;
    for (index, function) in def.processing_functions().into_iter().enumerate() {
        let key = (fan_key.0, fan_key.1, index);
        match function.sanitized() {
            CurveFunction::Identity => {}
            CurveFunction::Standard { hysteresis } => {
                target = apply_curve_hysteresis(states, key, hysteresis, input, target, now);
            }
            CurveFunction::Ema { alpha } => {
                input = apply_curve_ema(states, key, input, alpha);
                target = def.kind.evaluate(input)?;
            }
        }
    }
    Some(target.clamp(0.0, 100.0))
}

fn apply_curve_hysteresis(
    states: &mut HashMap<CurveFunctionKey, CurveRuntime>,
    key: CurveFunctionKey,
    hysteresis: CurveHysteresis,
    input: f32,
    target: f32,
    now: Instant,
) -> f32 {
    let target = target.clamp(0.0, 100.0);
    let hysteresis = hysteresis.sanitized();
    let state = match states.get_mut(&key) {
        Some(CurveRuntime::Hysteresis(state)) => state,
        _ => {
            states.insert(
                key,
                CurveRuntime::Hysteresis(CurveHysteresisRuntime {
                    accepted_input: input,
                    accepted_target: target,
                    pending: None,
                }),
            );
            return target;
        }
    };

    if hysteresis.is_disabled() || target >= 100.0 {
        accept_curve_target(state, input, target);
        return target;
    };

    let increasing = target > state.accepted_target + 0.5;
    let decreasing = target < state.accepted_target - 0.5;
    if !increasing && !decreasing {
        if hysteresis.only_downward && input > state.accepted_input {
            state.accepted_input = input;
        }
        state.pending = None;
        return state.accepted_target;
    }

    if increasing && hysteresis.only_downward {
        accept_curve_target(state, input, target);
        return target;
    }

    let beyond_threshold = if decreasing {
        input <= state.accepted_input - hysteresis.degrees
    } else {
        (input - state.accepted_input).abs() >= hysteresis.degrees
    };
    if !beyond_threshold {
        state.pending = None;
        return state.accepted_target;
    }

    if hysteresis.delay_ms == 0 {
        accept_curve_target(state, input, target);
        return target;
    }

    let pending = state.pending.get_or_insert(CurvePending {
        input,
        target,
        since: now,
    });
    pending.input = input;
    pending.target = target;
    if now.duration_since(pending.since) >= Duration::from_millis(hysteresis.delay_ms) {
        accept_curve_target(state, input, target);
        target
    } else {
        state.accepted_target
    }
}

fn apply_curve_ema(
    states: &mut HashMap<CurveFunctionKey, CurveRuntime>,
    key: CurveFunctionKey,
    input: f32,
    alpha: f32,
) -> f32 {
    let alpha = alpha.clamp(0.01, 1.0);
    match states.get_mut(&key) {
        Some(CurveRuntime::Ema(state)) => {
            state.value += (input - state.value) * alpha;
            state.value
        }
        _ => {
            states.insert(key, CurveRuntime::Ema(CurveEmaRuntime { value: input }));
            input
        }
    }
}

pub(super) fn clear_curve_runtime(hw: &mut Hardware, chip: usize, fan: usize) {
    hw.curve_runtime
        .retain(|&(ci, fi, _), _| ci != chip || fi != fan);
}

fn accept_curve_target(state: &mut CurveHysteresisRuntime, input: f32, target: f32) {
    state.accepted_input = input;
    state.accepted_target = target;
    state.pending = None;
}

/// Clamps user-supplied settings into safe ranges before persisting.
pub(super) fn sanitize(settings: FanSettings) -> FanSettings {
    let clamp_pct = |v: Option<f32>| v.map(|v| v.clamp(0.0, 100.0));
    let clamp_rate = |v: Option<f32>| v.filter(|v| *v > 0.0).map(|v| v.min(100.0));
    FanSettings {
        step_up: clamp_rate(settings.step_up),
        step_down: clamp_rate(settings.step_down),
        start_percent: clamp_pct(settings.start_percent),
        stop_percent: clamp_pct(settings.stop_percent),
        offset: settings.offset.clamp(-100.0, 100.0),
        minimum_percent: settings.minimum_percent.clamp(0.0, 100.0),
    }
}

pub(super) fn fan_settings(hw: &Hardware, chip: usize, fan: usize) -> FanSettings {
    hw.settings
        .get(chip)
        .and_then(|fans| fans.get(fan))
        .copied()
        .unwrap_or_default()
}

fn command_for_request(hw: &Hardware, chip: usize, fan: usize, request: u8) -> u8 {
    let target = effective_target_percent(
        fan_settings(hw, chip, fan),
        fan_calibration(hw, chip, fan),
        duty_to_percent(request),
    );
    command_for_target(hw, chip, fan, target)
}

fn command_for_target(hw: &Hardware, chip: usize, fan: usize, target_percent: f32) -> u8 {
    fan_calibration(hw, chip, fan)
        .and_then(|curve| curve.command_for_speed_percent(target_percent))
        .unwrap_or_else(|| percent_to_duty(target_percent))
}

pub(super) fn fan_calibration(hw: &Hardware, chip: usize, fan: usize) -> Option<&FanCurve> {
    hw.curves
        .get(chip)
        .and_then(|fans| fans.get(fan))
        .and_then(Option::as_ref)
}

pub(super) fn effective_target_percent(
    settings: FanSettings,
    calibration: Option<&FanCurve>,
    requested: f32,
) -> f32 {
    let calibrated_floor = calibration
        .and_then(FanCurve::minimum_speed_percent)
        .unwrap_or(0.0);
    let floor = settings
        .minimum_percent
        .max(calibrated_floor)
        .clamp(0.0, 100.0);
    (requested + settings.offset).clamp(floor, 100.0)
}

fn speed_percent_for_command(hw: &Hardware, chip: usize, fan: usize, command: u8) -> Option<f32> {
    fan_calibration(hw, chip, fan).and_then(|curve| curve.speed_percent_for_command(command))
}

/// Applies a client target request through the fan's settings and
/// calibration: offset/minimum shape the target speed, then the calibrated
/// command→RPM graph is inverted to find the hardware command. Step limits
/// turn the write into a ramp that [`tick_ramps`] advances. Returns true if a
/// register write happened.
pub(super) fn request_duty(hw: &mut Hardware, chip: usize, fan: usize, duty: Option<u8>) -> bool {
    let Some(request) = duty else {
        // Force the chip's own SmartFan mode rather than restoring the
        // pre-manual register state: that state is whatever the session
        // started with, which may itself be a manual duty left behind by
        // another tool — restoring it makes the auto switch a no-op.
        hw.ramps.remove(&(chip, fan));
        hw.requested.remove(&(chip, fan));
        if let Err(error) = hw.session.force_auto(chip, fan) {
            log_line(&format!("force_auto({chip},{fan}) failed: {error}"));
        }
        return true;
    };

    hw.requested.insert((chip, fan), request);
    let settings = fan_settings(hw, chip, fan);
    let target = effective_target_percent(
        settings,
        fan_calibration(hw, chip, fan),
        duty_to_percent(request),
    );
    if settings.step_up.is_none() && settings.step_down.is_none() {
        hw.ramps.remove(&(chip, fan));
        let command = command_for_target(hw, chip, fan, target);
        if let Err(error) = hw.session.set_fan(chip, fan, Some(command)) {
            log_line(&format!("set_fan({chip},{fan}) failed: {error}"));
        }
        return true;
    }

    // Ramp from wherever the fan sits now; if that's unknown (auto mode
    // reports no live duty), the first tick jumps straight to the target.
    let current = match hw.ramps.get(&(chip, fan)) {
        Some(ramp) => ramp.current,
        None => hw
            .snapshots
            .get(chip)
            .and_then(|snap| snap.fans.get(fan))
            .and_then(|status| match status.duty {
                Some(ipc::FanDuty::Manual { percent }) => {
                    let command = percent_to_duty(percent);
                    speed_percent_for_command(hw, chip, fan, command).or(Some(percent))
                }
                _ => None,
            })
            .unwrap_or(target),
    };
    hw.ramps.insert(
        (chip, fan),
        Ramp {
            current,
            target,
            last_written: None,
        },
    );
    false
}

/// Advances every active ramp by `dt` seconds, writing commands whose rounded
/// register value changed. Returns true if anything was written.
pub(super) fn tick_ramps(hw: &mut Hardware, dt: f32) -> bool {
    if hw.ramps.is_empty() {
        return false;
    }
    // The first tick after a ramp is created sees the elapsed time of the
    // preceding idle sleep; cap it so a fresh ramp can't jump ahead.
    let dt = dt.clamp(0.0, 2.0 * RAMP_TICK.as_secs_f32());
    let mut wrote = false;
    let keys: Vec<(usize, usize)> = hw.ramps.keys().copied().collect();
    for (chip, fan) in keys {
        let settings = fan_settings(hw, chip, fan);
        let Some((current, done)) = hw.ramps.get_mut(&(chip, fan)).map(|ramp| {
            let delta = ramp.target - ramp.current;
            let rate = if delta > 0.0 {
                settings.step_up
            } else {
                settings.step_down
            };
            ramp.current = match rate {
                Some(rate) => {
                    let step = rate * dt;
                    if delta.abs() <= step {
                        ramp.target
                    } else {
                        ramp.current + step * delta.signum()
                    }
                }
                // No limit in this direction: jump.
                None => ramp.target,
            };
            let done = (ramp.current - ramp.target).abs() < f32::EPSILON;
            (ramp.current, done)
        }) else {
            continue;
        };

        let value = command_for_target(hw, chip, fan, current);
        let Some(ramp) = hw.ramps.get_mut(&(chip, fan)) else {
            continue;
        };
        let write = ramp.last_written != Some(value);
        ramp.last_written = Some(value);
        if write {
            if let Err(error) = hw.session.set_fan(chip, fan, Some(value)) {
                log_line(&format!("set_fan({chip},{fan}) failed: {error}"));
            }
            wrote = true;
        }
        if done {
            hw.ramps.remove(&(chip, fan));
        }
    }
    wrote
}

#[cfg(test)]
mod tests {
    use super::*;
    use zugluft_ipc::{CurveKind, CurveSource};

    fn hysteresis() -> CurveHysteresis {
        CurveHysteresis {
            degrees: 2.0,
            delay_ms: 2_000,
            only_downward: true,
        }
    }

    #[test]
    fn curve_hysteresis_applies_heat_up_immediately() {
        let mut states = HashMap::new();
        let now = Instant::now();

        assert_eq!(
            apply_curve_hysteresis(&mut states, (0, 0, 0), hysteresis(), 50.0, 30.0, now),
            30.0
        );
        assert_eq!(
            apply_curve_hysteresis(
                &mut states,
                (0, 0, 0),
                hysteresis(),
                55.0,
                60.0,
                now + Duration::from_millis(500),
            ),
            60.0
        );
    }

    #[test]
    fn curve_hysteresis_delays_cool_down_until_sustained() {
        let mut states = HashMap::new();
        let now = Instant::now();
        apply_curve_hysteresis(&mut states, (0, 0, 0), hysteresis(), 80.0, 80.0, now);

        assert_eq!(
            apply_curve_hysteresis(
                &mut states,
                (0, 0, 0),
                hysteresis(),
                76.0,
                40.0,
                now + Duration::from_millis(500),
            ),
            80.0
        );
        assert_eq!(
            apply_curve_hysteresis(
                &mut states,
                (0, 0, 0),
                hysteresis(),
                76.0,
                40.0,
                now + Duration::from_millis(2_500),
            ),
            40.0
        );
    }

    #[test]
    fn curve_hysteresis_holds_small_temperature_drops() {
        let mut states = HashMap::new();
        let now = Instant::now();
        apply_curve_hysteresis(&mut states, (0, 0, 0), hysteresis(), 60.0, 50.0, now);

        assert_eq!(
            apply_curve_hysteresis(
                &mut states,
                (0, 0, 0),
                hysteresis(),
                58.5,
                40.0,
                now + Duration::from_secs(10),
            ),
            50.0
        );
    }

    #[test]
    fn ema_function_smooths_curve_input_before_evaluation() {
        let mut states = HashMap::new();
        let now = Instant::now();
        let def = CurveDef {
            id: "curve".to_string(),
            name: "Curve".to_string(),
            source: CurveSource::Custom {
                custom: "custom".to_string(),
            },
            functions: vec![CurveFunction::Ema { alpha: 0.5 }],
            hysteresis: Default::default(),
            window: Default::default(),
            kind: CurveKind::Graph {
                points: vec![(0.0, 0.0), (100.0, 100.0)],
            },
        };

        assert_eq!(
            apply_curve_functions(&mut states, (0, 0), &def, 20.0, now),
            Some(20.0)
        );
        assert_eq!(
            apply_curve_functions(
                &mut states,
                (0, 0),
                &def,
                60.0,
                now + Duration::from_millis(500),
            ),
            Some(40.0)
        );
    }
}

//! The hardware worker: owns the LHM-backed hardware session, applies fan targets
//! (coalesced, last-write-wins per fan), polls readings, and publishes state
//! through the [`Hub`]. Retries detection periodically so installing/fixing
//! the bridge, LHM's low-level dependencies or permissions doesn't require a
//! service restart.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

use zugluft_hw::Session;
use zugluft_ipc::{
    self as ipc, CurveDef, CurveFunction, CurveHysteresis, CustomSensorDef, FanSettings, Request,
    ServiceState,
};

use crate::calibration::{self, FanCurve, Store};
use crate::curves;
use crate::custom;
use crate::hub::Hub;
use crate::log_line;
use crate::manual;
use crate::settings;

const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Minimum delay between full re-reads triggered by fan writes (so slider
/// drags don't hammer the EC).
const WRITE_REFRESH_DEBOUNCE: Duration = Duration::from_millis(100);
/// Wake-up interval while a step-limited duty change is ramping.
const RAMP_TICK: Duration = Duration::from_millis(250);
/// Pulse length for a calibrated restart command before settling to the
/// requested steady command.
const START_BOOST_DURATION: Duration = Duration::from_secs(1);
const DETECT_RETRY_INTERVAL: Duration = Duration::from_secs(30);
/// Consecutive update failures before the session is considered dead.
const MAX_UPDATE_FAILURES: u32 = 5;

mod calibrate;
mod duty;
mod publish;

use calibrate::*;
use duty::*;
use publish::*;

pub enum Command {
    Request(Request),
    Shutdown,
}

struct Hardware {
    session: Session,
    chips: Vec<ipc::ChipInfo>,
    notes: Vec<String>,
    snapshots: Vec<ipc::ChipSnapshot>,
    /// Calibration results per chip per fan, merged into every snapshot.
    curves: Vec<Vec<Option<FanCurve>>>,
    /// User tuning per chip per fan, merged into every snapshot and
    /// applied to every target write.
    settings: Vec<Vec<FanSettings>>,
    /// In-flight step-limited target changes, advanced by [`tick_ramps`].
    ramps: HashMap<(usize, usize), Ramp>,
    /// The encoded target each manual fan was last asked for, before offset,
    /// minimum and calibration — so settings changes and calibration restores
    /// can re-apply the user's request instead of compounding adjustments.
    requested: HashMap<(usize, usize), u8>,
    /// Curve id driving each fan, per chip per fan; `None` is manual/auto.
    assignments: Vec<Vec<Option<String>>>,
    /// Hardware command each curve-driven fan last wrote, so unchanged final
    /// outputs don't turn into redundant writes every poll.
    curve_written: HashMap<(usize, usize), u8>,
    /// Runtime state per curve function on each curve-driven fan.
    curve_runtime: HashMap<CurveFunctionKey, CurveRuntime>,
    failures: u32,
}

/// A target-speed transition limited by `step_up`/`step_down` (%/s).
struct Ramp {
    current: f32,
    target: f32,
    last_written: Option<u8>,
    hold_until: Option<Instant>,
}

type CurveFunctionKey = (usize, usize, usize); // chip, fan, function index

enum CurveRuntime {
    Hysteresis(CurveHysteresisRuntime),
    Ema(CurveEmaRuntime),
}

struct CurveHysteresisRuntime {
    accepted_input: f32,
    accepted_target: f32,
    pending: Option<CurvePending>,
}

struct CurveEmaRuntime {
    value: f32,
}

struct CurvePending {
    input: f32,
    target: f32,
    since: Instant,
}

pub fn run(hub: &Arc<Hub>, rx: &Receiver<Command>) {
    let mut hardware: Option<Hardware> = None;
    let mut store = Store::load();
    let mut settings_store = settings::Store::load();
    let mut customs = custom::load();
    let mut curve_defs = curves::load_defs();
    let mut curve_store = curves::Assignments::load();
    let mut manual_store = manual::Store::load();
    let mut next_detect = Instant::now();
    let mut next_poll = Instant::now();
    let mut last_refresh = Instant::now() - WRITE_REFRESH_DEBOUNCE;
    let mut last_tick = Instant::now();

    'main: loop {
        // (Re-)detect hardware when due.
        if hardware.is_none() && Instant::now() >= next_detect {
            match Session::open() {
                Ok(mut session) => {
                    session.set_restore_on_drop(true);
                    let chips: Vec<ipc::ChipInfo> =
                        session.chips().iter().map(convert_info).collect();
                    let notes = session.notes().to_vec();
                    let assignments = assignments_from_store(&curve_store, &chips);
                    let saved_manual = manual_store.targets_for_chips(&chips, &assignments);
                    let mut hw = Hardware {
                        curves: curves_from_store(&store, &chips),
                        settings: settings_from_store(&settings_store, &chips),
                        assignments,
                        session,
                        chips,
                        notes,
                        snapshots: Vec::new(),
                        ramps: HashMap::new(),
                        requested: HashMap::new(),
                        curve_written: HashMap::new(),
                        curve_runtime: HashMap::new(),
                        failures: 0,
                    };
                    if let Ok(snaps) = hw.session.update() {
                        store_snapshots(&mut hw, &snaps);
                    }
                    for ((chip, fan), duty) in saved_manual {
                        request_target(&mut hw, chip, fan, Some(duty));
                    }
                    stamp_fans(&mut hw);
                    publish_ready(hub, &hw, &customs, &curve_defs);
                    log_line("hardware session opened");
                    hardware = Some(hw);
                    next_poll = Instant::now() + POLL_INTERVAL;
                }
                Err(error) => {
                    log_line(&format!("detection failed: {error}"));
                    hub.publish(ServiceState::Failed {
                        error: error.to_string(),
                    });
                    next_detect = Instant::now() + DETECT_RETRY_INTERVAL;
                }
            }
        }

        // Wait for commands until the next poll (or detect retry) is due,
        // then coalesce everything that queued up.
        let deadline = if hardware.is_some() {
            next_poll
        } else {
            next_detect
        };
        let mut timeout = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(1));
        if hardware.as_ref().is_some_and(|hw| !hw.ramps.is_empty()) {
            timeout = timeout.min(RAMP_TICK);
        }

        let mut pending: HashMap<(usize, usize), Option<u8>> = HashMap::new();
        let mut redetect = false;
        let mut calibrate = false;
        let mut customs_update: Option<Vec<CustomSensorDef>> = None;
        let mut settings_updates: Vec<(usize, usize, FanSettings)> = Vec::new();
        let mut curves_update: Option<Vec<CurveDef>> = None;
        let mut assign_updates: Vec<(usize, usize, Option<String>)> = Vec::new();
        match rx.recv_timeout(timeout) {
            Ok(command) => {
                if handle_command(
                    command,
                    &mut pending,
                    &mut redetect,
                    &mut calibrate,
                    &mut customs_update,
                    &mut settings_updates,
                    &mut curves_update,
                    &mut assign_updates,
                ) {
                    break 'main;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break 'main,
            Err(RecvTimeoutError::Timeout) => {}
        }
        loop {
            match rx.try_recv() {
                Ok(command) => {
                    if handle_command(
                        command,
                        &mut pending,
                        &mut redetect,
                        &mut calibrate,
                        &mut customs_update,
                        &mut settings_updates,
                        &mut curves_update,
                        &mut assign_updates,
                    ) {
                        break 'main;
                    }
                }
                Err(TryRecvError::Disconnected) => break 'main,
                Err(TryRecvError::Empty) => break,
            }
        }

        let now = Instant::now();
        let tick_dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        if let Some(defs) = customs_update.take() {
            customs = defs;
            custom::save(&customs);
            // Updated names/values show up with the next publish (≤ one
            // poll interval away).
        }

        // Forces a curve re-apply outside the poll cadence: changed
        // definitions retarget fans now, fresh assignments take effect now.
        let mut curves_dirty = false;
        if let Some(defs) = curves_update.take() {
            curve_defs = defs;
            for def in &mut curve_defs {
                def.normalize_functions();
                def.normalize_window();
                def.normalize_kind();
            }
            curves::save_defs(&curve_defs);
            if let Some(hw) = &mut hardware {
                hw.curve_written.clear();
                hw.curve_runtime.clear();
            }
            curves_dirty = true;
        }

        if redetect {
            hardware = None; // drops the session, restoring BIOS control
            hub.publish(ServiceState::Detecting);
            next_detect = Instant::now();
            continue;
        }

        let Some(hw) = &mut hardware else {
            continue;
        };

        let mut wrote = false;
        let mut manual_dirty = false;
        for (&(chip, fan), &duty) in &pending {
            if let Some(info) = hw.chips.get(chip)
                && fan < info.control_count
            {
                manual_store.set(&calibration::chip_key(info), fan, duty);
                manual_dirty = true;
            }
            // A manual/auto request takes the fan off its curve.
            if let Some(slot) = hw
                .assignments
                .get_mut(chip)
                .and_then(|fans| fans.get_mut(fan))
                && slot.take().is_some()
            {
                if let Some(info) = hw.chips.get(chip) {
                    curve_store.set(&calibration::chip_key(info), fan, None);
                    curve_store.save();
                }
                hw.curve_written.remove(&(chip, fan));
                clear_curve_runtime(hw, chip, fan);
            }
            // Transient failures (e.g. mutex contention with another tool)
            // resolve on the next write or poll.
            wrote |= request_target(hw, chip, fan, duty);
        }
        if manual_dirty {
            manual_store.save();
        }

        if !assign_updates.is_empty() {
            let mut manual_dirty = false;
            for (chip, fan, curve) in assign_updates.drain(..) {
                let Some(info) = hw.chips.get(chip) else {
                    continue;
                };
                if fan >= info.control_count {
                    continue;
                }
                let key = calibration::chip_key(info);
                curve_store.set(&key, fan, curve.as_deref());
                manual_store.set(&key, fan, None);
                manual_dirty = true;
                let Some(slot) = hw
                    .assignments
                    .get_mut(chip)
                    .and_then(|fans| fans.get_mut(fan))
                else {
                    continue;
                };
                let released = curve.is_none() && slot.is_some();
                *slot = curve;
                // Assigning clears the write cache so the curve's first
                // output lands even if it matches a stale value; releasing
                // hands the fan back to the chip's automatic control.
                hw.curve_written.remove(&(chip, fan));
                clear_curve_runtime(hw, chip, fan);
                if released {
                    wrote |= request_target(hw, chip, fan, None);
                }
            }
            curve_store.save();
            if manual_dirty {
                manual_store.save();
            }
            curves_dirty = true;
            stamp_fans(hw);
            publish_ready(hub, hw, &customs, &curve_defs);
        }

        if !settings_updates.is_empty() {
            for (chip, fan, new) in settings_updates.drain(..) {
                let Some(info) = hw.chips.get(chip) else {
                    continue;
                };
                settings_store.insert(&calibration::chip_key(info), fan, sanitize(new));
            }
            settings_store.save();
            hw.settings = settings_from_store(&settings_store, &hw.chips);
            // A changed offset or minimum retargets manual fans right away.
            let requested: Vec<((usize, usize), u8)> =
                hw.requested.iter().map(|(&k, &v)| (k, v)).collect();
            for ((chip, fan), raw) in requested {
                wrote |= request_target(hw, chip, fan, Some(raw));
            }
            stamp_fans(hw);
            publish_ready(hub, hw, &customs, &curve_defs);
        }

        if calibrate {
            match run_calibration(
                hub,
                hw,
                rx,
                &mut store,
                &mut manual_store,
                &mut customs,
                &mut curve_defs,
                &pending,
            ) {
                CalEnd::Done => {
                    publish_ready(hub, hw, &customs, &curve_defs);
                    last_refresh = Instant::now();
                    last_tick = Instant::now();
                    next_poll = Instant::now() + POLL_INTERVAL;
                }
                CalEnd::Shutdown => break 'main,
                // Dropping the session restores BIOS control, so the
                // aborted run doesn't need to restore duties itself.
                CalEnd::Redetect => {
                    hardware = None;
                    hub.publish(ServiceState::Detecting);
                    next_detect = Instant::now();
                    continue;
                }
            }
        }

        wrote |= tick_ramps(hw, tick_dt);

        let due = Instant::now() >= next_poll;
        // Curve-driven duties are recomputed on the poll cadence from the
        // previous poll's temperatures; the update below reads the result
        // back in the same pass.
        if due || curves_dirty {
            wrote |= apply_curves(hw, &curve_defs, &customs);
        }
        if due || (wrote && last_refresh.elapsed() >= WRITE_REFRESH_DEBOUNCE) {
            match hw.session.update() {
                Ok(snaps) => {
                    store_snapshots(hw, &snaps);
                    hw.failures = 0;
                }
                Err(error) => {
                    hw.failures += 1;
                    if hw.failures >= MAX_UPDATE_FAILURES {
                        log_line(&format!("hardware session lost: {error}"));
                        hub.publish(ServiceState::Failed {
                            error: error.to_string(),
                        });
                        hardware = None;
                        next_detect = Instant::now() + DETECT_RETRY_INTERVAL;
                        continue;
                    }
                }
            }
            publish_ready(hub, hw, &customs, &curve_defs);
            last_refresh = Instant::now();
            if due {
                next_poll = Instant::now() + POLL_INTERVAL;
            }
        }
    }

    drop(hardware); // restores BIOS fan control
    log_line("worker stopped, BIOS control restored");
}

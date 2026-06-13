use super::*;

/// Queries the running service over IPC; works without elevation.
pub(crate) fn cmd_status() -> ExitCode {
    use zugluft_ipc::{Event, ServiceState};

    let stream = match zugluft_ipc::pipe::connect_events() {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("service not reachable ({e})");
            eprintln!("install/start it with: zugluft-service.exe install  (elevated)");
            return ExitCode::FAILURE;
        }
    };
    let mut reader = std::io::BufReader::new(stream);
    match zugluft_ipc::recv::<Event>(&mut reader) {
        Ok(Some(Event::State(state))) => {
            match state {
                ServiceState::Detecting => println!("service running, detecting hardware…"),
                ServiceState::Failed { error } => {
                    println!("service running, hardware unavailable: {error}");
                }
                ServiceState::Calibrating { message } => {
                    println!("service running, calibrating fans: {message}");
                }
                ServiceState::Ready {
                    chips,
                    snapshots,
                    notes,
                    customs,
                    curves,
                } => {
                    println!("service running");
                    for (ci, (info, snap)) in chips.iter().zip(&snapshots).enumerate() {
                        println!(
                            "chip {ci}: {} (version 0x{:X}) at 0x{:04X}, slot {}",
                            info.name, info.version, info.address, info.slot
                        );
                        for (fi, fan) in snap.fans.iter().enumerate() {
                            let duty = match (&fan.curve, fan.duty) {
                                (Some(id), duty) => {
                                    let name = curves
                                        .iter()
                                        .find(|curve| &curve.id == id)
                                        .map_or(id.as_str(), |curve| curve.name.as_str());
                                    target_and_command(
                                        &format!("curve {name}"),
                                        fan.target_percent,
                                        duty,
                                    )
                                }
                                (None, None) => "(no control)".into(),
                                (None, Some(zugluft_ipc::FanDuty::Auto)) => "[auto]".into(),
                                (None, duty) => {
                                    target_and_command("manual", fan.target_percent, duty)
                                }
                            };
                            let calibrated = fan
                                .max_rpm
                                .map(|m| {
                                    let mut text = format!("  (max {m:.0} rpm");
                                    if let Some(min) = fan.min_percent.filter(|min| *min > 0.5) {
                                        text.push_str(&format!(", min {min:.0} %"));
                                    }
                                    if let Some(stop) = fan.stop_percent {
                                        text.push_str(&format!(", stops ≤{stop:.0} %"));
                                    }
                                    if let Some(start) = fan.start_percent {
                                        text.push_str(&format!(", starts ≥{start:.0} %"));
                                    }
                                    text.push(')');
                                    text
                                })
                                .unwrap_or_default();
                            println!("  fan {fi}: {:>8}  {duty}{calibrated}", rpm_text(fan.rpm));
                        }
                        let temps: Vec<String> = snap
                            .temps
                            .iter()
                            .enumerate()
                            .filter_map(|(ti, t)| {
                                t.map(|v| format!("{}={v:.1} °C", temp_key(&info.temp_labels, ti)))
                            })
                            .collect();
                        if !temps.is_empty() {
                            println!("  temps: {}", temps.join("  "));
                        }
                        let powers: Vec<String> = snap
                            .powers
                            .iter()
                            .enumerate()
                            .filter_map(|(pi, p)| {
                                p.map(|v| {
                                    let label =
                                        info.power_labels.get(pi).map_or("power", |l| l.as_str());
                                    format!("{label}={v:.1} W")
                                })
                            })
                            .collect();
                        if !powers.is_empty() {
                            println!("  power: {}", powers.join("  "));
                        }
                    }
                    for custom in customs {
                        let value = custom
                            .value
                            .map_or("—".to_string(), |v| format!("{v:.1} °C"));
                        println!("custom {}: {value}", custom.name);
                    }
                    for curve in curves {
                        let input = curve
                            .input
                            .map_or("—".to_string(), |v| format!("{v:.1} °C"));
                        let output = curve
                            .output
                            .map_or("—".to_string(), |v| format!("{v:.0} %"));
                        println!("curve {}: {input} → {output}", curve.name);
                    }
                    for note in notes {
                        println!("note: {note}");
                    }
                }
            }
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unexpected reply from service: {other:?}");
            ExitCode::FAILURE
        }
    }
}

fn target_and_command(
    mode: &str,
    target: Option<f32>,
    duty: Option<zugluft_ipc::FanDuty>,
) -> String {
    match (target, duty) {
        (Some(target), Some(zugluft_ipc::FanDuty::Manual { percent }))
            if (target - percent).abs() > 0.5 =>
        {
            format!("[{mode} {target:.0} % → cmd {percent:.0} %]")
        }
        (Some(target), _) => format!("[{mode} {target:.0} %]"),
        (None, Some(zugluft_ipc::FanDuty::Manual { percent })) => {
            format!("[{mode} cmd {percent:.0} %]")
        }
        (None, _) => format!("[{mode}]"),
    }
}

/// Asks the running service to calibrate and follows its progress until
/// the run finishes; works without elevation.
pub(crate) fn cmd_calibrate() -> ExitCode {
    use zugluft_ipc::{Event, Request, ServiceState};

    let events = match zugluft_ipc::pipe::connect_events() {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("service not reachable ({e})");
            eprintln!("install/start it with: zugluft-service.exe install  (elevated)");
            return ExitCode::FAILURE;
        }
    };
    let mut control = match zugluft_ipc::pipe::connect_control() {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("service control pipe not reachable ({e})");
            return ExitCode::FAILURE;
        }
    };
    if zugluft_ipc::send(&mut control, &Request::Calibrate).is_err() {
        eprintln!("failed to send calibrate request");
        return ExitCode::FAILURE;
    }
    println!(
        "calibrating: fans step from 100 % duty down to a stop test, stalled fans \
         are ramped back up to find their restart duty, then previous duties are restored"
    );

    let mut reader = std::io::BufReader::new(events);
    let mut started = false;
    let mut ready_before_start = 0;
    let mut last_message = String::new();
    while let Ok(Some(Event::State(state))) = zugluft_ipc::recv::<Event>(&mut reader) {
        match state {
            ServiceState::Calibrating { message } => {
                started = true;
                if message != last_message {
                    println!("  {message}");
                    last_message = message;
                }
            }
            // Regular polling states keep arriving until the worker picks
            // the request up; if it never does, don't wait forever.
            ServiceState::Ready { .. } if !started => {
                ready_before_start += 1;
                if ready_before_start > 8 {
                    eprintln!("calibration did not start (no controllable fans?)");
                    return ExitCode::FAILURE;
                }
            }
            // The first event is the pre-calibration state; only a Ready
            // arriving after calibration started means we're done.
            ServiceState::Ready {
                chips,
                snapshots,
                notes,
                ..
            } if started => {
                if let Some(note) = notes.iter().find(|note| note.starts_with("calibration")) {
                    eprintln!("{note}");
                    return ExitCode::FAILURE;
                }
                println!("calibration finished:");
                for (info, snap) in chips.iter().zip(&snapshots) {
                    for (fi, fan) in snap.fans.iter().enumerate() {
                        if let Some(max) = fan.max_rpm {
                            let stop = match (fan.stop_percent, fan.start_percent) {
                                (Some(stop), Some(start)) => {
                                    format!(", stops at {stop:.0} %, restarts at {start:.0} %")
                                }
                                (Some(stop), None) => {
                                    format!(", stops at {stop:.0} % (restart duty unknown)")
                                }
                                _ => ", no stop detected".to_string(),
                            };
                            println!("  {} fan {fi}: max {max:.0} rpm{stop}", info.name);
                        }
                    }
                }
                return ExitCode::SUCCESS;
            }
            ServiceState::Failed { error } => {
                eprintln!("hardware unavailable: {error}");
                return ExitCode::FAILURE;
            }
            _ => {}
        }
    }
    eprintln!("service connection lost");
    ExitCode::FAILURE
}

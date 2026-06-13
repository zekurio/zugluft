use super::*;

pub(crate) fn open() -> Result<Session, HwError> {
    let session = Session::open()?;
    for note in session.notes() {
        eprintln!("note: {note}");
    }
    Ok(session)
}

pub(crate) fn cmd_detect() -> Result<(), HwError> {
    let mut session = open()?;
    let snapshots = session.update()?;

    for (ci, (info, snap)) in session.chips().iter().zip(&snapshots).enumerate() {
        println!(
            "chip {ci}: {} (version 0x{:X}) at 0x{:04X}, slot {}",
            info.name, info.version, info.address, info.slot
        );
        for (fi, fan) in snap.fans.iter().enumerate() {
            println!(
                "  fan {fi}: {:>8}  {}",
                rpm_text(fan.rpm),
                duty_text(fan.duty)
            );
        }
        let temps: Vec<String> = snap
            .temps
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.map(|v| format!("{}={v:.1}°C", temp_key(&info.temp_labels, i))))
            .collect();
        if !temps.is_empty() {
            println!("  temps: {}", temps.join("  "));
        }
        let powers: Vec<String> = snap
            .powers
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                p.map(|v| {
                    let label = info.power_labels.get(i).map_or("power", |l| l.as_str());
                    format!("{label}={v:.1} W")
                })
            })
            .collect();
        if !powers.is_empty() {
            println!("  power: {}", powers.join("  "));
        }
    }
    Ok(())
}

/// A channel's short name: its chip-provided label, or `tN`.
pub(crate) fn temp_key(labels: &[String], index: usize) -> String {
    labels
        .get(index)
        .cloned()
        .unwrap_or_else(|| format!("t{index}"))
}

pub(crate) fn cmd_watch(interval_ms: u64) -> Result<(), HwError> {
    let mut session = open()?;
    println!("polling every {interval_ms} ms, Ctrl+C to stop\n");
    loop {
        match session.update() {
            Ok(snapshots) => {
                for (ci, snap) in snapshots.iter().enumerate() {
                    let fans: Vec<String> = snap
                        .fans
                        .iter()
                        .enumerate()
                        .map(|(fi, f)| {
                            format!("fan{fi} {:>8} {}", rpm_text(f.rpm), duty_text(f.duty))
                        })
                        .collect();
                    println!("chip {ci}: {}", fans.join(" | "));
                }
            }
            Err(HwError::MutexTimeout { .. }) => println!("(bus busy, skipped)"),
            Err(e) => return Err(e),
        }
        thread::sleep(Duration::from_millis(interval_ms));
    }
}

pub(crate) fn cmd_set(args: &[String]) -> Result<(), HwError> {
    let (Some(fan), Some(percent)) = (
        args.first().and_then(|s| s.parse::<usize>().ok()),
        args.get(1).and_then(|s| s.parse::<f32>().ok()),
    ) else {
        print!("{USAGE}");
        return Ok(());
    };
    let chip = parse_chip(args);
    let percent = percent.clamp(0.0, 100.0);
    let duty = (percent * 255.0 / 100.0).round() as u8;

    let mut session = open()?;

    // Save the BIOS state once, before our first ever write to this fan.
    let mut baseline = Baseline::load();
    if !baseline.contains(chip, fan) {
        let state = session.fan_reg_state(chip, fan)?;
        baseline.insert(chip, fan, state);
        baseline.store();
    }

    session.set_fan(chip, fan, Some(duty))?;
    println!("chip {chip} fan {fan} pinned to {percent:.0} % (duty {duty}/255)");

    // Give the fan a moment and read back the result.
    thread::sleep(Duration::from_millis(1500));
    let snap = session.update()?;
    if let Some(fan_status) = snap.get(chip).and_then(|s| s.fans.get(fan)) {
        println!(
            "readback: {} {}",
            rpm_text(fan_status.rpm),
            duty_text(fan_status.duty)
        );
    }
    println!("restore with: zugluftctl auto {fan}{}", chip_suffix(chip));
    Ok(())
}

pub(crate) fn cmd_auto(args: &[String]) -> Result<(), HwError> {
    let chip = parse_chip(args);
    let mut session = open()?;
    let mut baseline = Baseline::load();

    let fans: Vec<usize> = match args.first().map(String::as_str) {
        Some("all") => (0..session.chips().get(chip).map_or(0, |c| c.control_count)).collect(),
        Some(s) => match s.parse() {
            Ok(fan) => vec![fan],
            Err(_) => {
                print!("{USAGE}");
                return Ok(());
            }
        },
        None => {
            print!("{USAGE}");
            return Ok(());
        }
    };

    for fan in fans {
        if let Some(state) = baseline.remove(chip, fan) {
            session.apply_fan_reg_state(chip, fan, state)?;
            println!("chip {chip} fan {fan}: BIOS register state restored");
        } else {
            session.force_auto(chip, fan)?;
            println!("chip {chip} fan {fan}: switched to automatic mode (no saved baseline)");
        }
    }
    baseline.store();
    Ok(())
}

pub(crate) fn cmd_report() -> Result<(), HwError> {
    let mut session = open()?;
    for (ci, info) in session.chips().to_vec().iter().enumerate() {
        println!(
            "chip {ci}: {} (version 0x{:X}) at 0x{:04X}, slot {}",
            info.name, info.version, info.address, info.slot
        );
        let dump = match session.ec_dump(ci) {
            Ok(dump) => dump,
            // The LHM backend abstracts raw EC/register access.
            Err(HwError::NoRawRegisters { .. }) => {
                println!("  (raw registers unavailable through LHM)\n");
                continue;
            }
            Err(e) => return Err(e),
        };
        println!("\n      00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F");
        for (row, chunk) in dump.chunks(16).enumerate() {
            let cells: Vec<String> = chunk
                .iter()
                .map(|v| v.map_or("??".into(), |b| format!("{b:02X}")))
                .collect();
            println!(" {:02X}   {}", row << 4, cells.join(" "));
        }
        println!();
    }
    Ok(())
}

pub(crate) fn parse_chip(args: &[String]) -> usize {
    args.windows(2)
        .find(|w| w[0] == "--chip")
        .and_then(|w| w[1].parse().ok())
        .unwrap_or(0)
}

pub(crate) fn chip_suffix(chip: usize) -> String {
    if chip == 0 {
        String::new()
    } else {
        format!(" --chip {chip}")
    }
}

pub(crate) fn rpm_text(rpm: Option<f32>) -> String {
    rpm.map_or("    —   ".into(), |r| format!("{r:>5.0} rpm"))
}

pub(crate) fn duty_text(duty: Option<FanDuty>) -> String {
    match duty {
        None => "(no control)".into(),
        Some(FanDuty::Auto) => "[auto]".into(),
        Some(FanDuty::Manual { percent }) => format!("[manual {percent:.0} %]"),
    }
}

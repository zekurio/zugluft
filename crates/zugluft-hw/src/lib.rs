//! Hardware access layer for zugluft.
//!
//! The backend is LibreHardwareMonitorLib, loaded through a small NativeAOT
//! bridge. LHM owns the motherboard, CPU, storage and GPU hardware-specific
//! code; zugluft keeps the policy layer: service ownership, curve evaluation,
//! calibration, persistence and client IPC.

mod error;
mod ffi;

use std::collections::HashSet;

pub use error::{HwError, Result};
use ffi::{Bridge, LhmComputer, LhmHardware, LhmSensor};

const SENSOR_POWER: i32 = 2;
const SENSOR_TEMPERATURE: i32 = 4;
const SENSOR_FAN: i32 = 7;
const SENSOR_CONTROL: i32 = 9;

const HARDWARE_CPU: i32 = 2;
const HARDWARE_GPU_NVIDIA: i32 = 4;
const HARDWARE_GPU_AMD: i32 = 5;
const HARDWARE_GPU_INTEL: i32 = 6;
const HARDWARE_STORAGE: i32 = 7;
const HARDWARE_BATTERY: i32 = 11;

const SLOT_MOTHERBOARD: u8 = 0;
const SLOT_CPU: u8 = 2;
const SLOT_GPU: u8 = 3;
const SLOT_STORAGE: u8 = 4;
const SLOT_OTHER: u8 = 5;

/// Control state of one fan header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FanDuty {
    /// The hardware's automatic/default control is driving this fan.
    Auto,
    /// Software pinned the fan to a fixed duty.
    Manual { percent: f32 },
}

/// One fan header's current readings.
#[derive(Debug, Clone, Default)]
pub struct FanStatus {
    /// Measured speed. `None` if no tachometer exists or LHM has no reading.
    pub rpm: Option<f32>,
    /// Duty/mode for fans that have a control channel; `None` for
    /// monitor-only headers.
    pub duty: Option<FanDuty>,
}

/// One polling pass over a hardware device.
#[derive(Debug, Clone, Default)]
pub struct ChipSnapshot {
    pub fans: Vec<FanStatus>,
    pub temps: Vec<Option<f32>>,
    pub powers: Vec<Option<f32>>,
}

/// Static description of a detected hardware device. The name "chip" is kept
/// for the rest of the app's existing model: LHM hardware nodes with relevant
/// sensors are presented as chips, and sensor-only devices have
/// `control_count == 0`.
#[derive(Debug, Clone)]
pub struct ChipInfo {
    pub name: String,
    /// Stable 16-bit hash of LHM's hardware identifier.
    pub address: u16,
    /// LHM hardware type value, truncated for display/key compatibility.
    pub version: u8,
    /// Pseudo slot: 0 = motherboard/controller, 2 = CPU, 3 = GPU,
    /// 4 = storage, 5 = other sensor hardware.
    pub slot: u8,
    pub fan_count: usize,
    /// How many of the fans have a software control channel. Controls are
    /// exposed as the first N fans to match the existing service contract.
    pub control_count: usize,
    pub temp_count: usize,
    pub temp_labels: Vec<String>,
    pub power_labels: Vec<String>,
}

/// Saved LHM fan-control state for direct CLI set/auto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FanRegState {
    Default,
    Software { percent: f32 },
    Unknown,
}

struct Device {
    info: ChipInfo,
    temps: Vec<LhmSensor>,
    powers: Vec<LhmSensor>,
    fan_rpms: Vec<Option<LhmSensor>>,
    controls: Vec<LhmSensor>,
    original_controls: Vec<FanRegState>,
}

/// An open hardware session. LHM does device-specific probing and I/O, while
/// this session keeps the stable zugluft shape and restore-on-drop semantics.
pub struct Session {
    computer: LhmComputer,
    devices: Vec<Device>,
    infos: Vec<ChipInfo>,
    notes: Vec<String>,
    restore_on_drop: bool,
    touched: HashSet<(usize, usize)>,
    _bridge: Bridge,
}

impl Session {
    /// Opens the LHM bridge, probes hardware and builds the device list.
    ///
    /// This still needs elevation for sensors/controls that LHM can only read
    /// through privileged drivers.
    pub fn open() -> Result<Session> {
        let bridge = Bridge::load()?;
        let computer = bridge.create_computer()?;
        computer.update();

        let mut devices = Vec::new();
        let mut notes = Vec::new();
        for hardware in computer.hardware() {
            collect_hardware(&hardware, &mut devices, &mut notes);
        }

        if devices.is_empty() {
            return Err(HwError::NoSupportedHardware);
        }

        let infos = devices.iter().map(|device| device.info.clone()).collect();
        Ok(Session {
            computer,
            devices,
            infos,
            notes,
            restore_on_drop: false,
            touched: HashSet::new(),
            _bridge: bridge,
        })
    }

    pub fn chips(&self) -> &[ChipInfo] {
        &self.infos
    }

    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// When enabled, dropping the session returns every touched control to
    /// its startup state.
    pub fn set_restore_on_drop(&mut self, value: bool) {
        self.restore_on_drop = value;
    }

    /// Updates all LHM hardware and reads the latest snapshot.
    pub fn update(&mut self) -> Result<Vec<ChipSnapshot>> {
        self.computer.update();
        Ok(self.devices.iter().map(Device::snapshot).collect())
    }

    /// Pins a fan to a fixed duty (`Some(0..=255)`) or returns it to LHM's
    /// default control state (`None`).
    pub fn set_fan(&mut self, chip: usize, fan: usize, duty: Option<u8>) -> Result<()> {
        self.check(chip, fan)?;
        let control = self.control(chip, fan)?;
        match duty {
            Some(value) => {
                control.set_software(duty_to_percent(value))?;
                self.touched.insert((chip, fan));
            }
            None => {
                self.restore_control(chip, fan)?;
            }
        }
        Ok(())
    }

    /// Returns every touched fan to the state it had when the LHM session was
    /// opened.
    pub fn restore_all(&mut self) -> Result<()> {
        let touched: Vec<_> = self.touched.iter().copied().collect();
        for (chip, fan) in touched {
            self.restore_control(chip, fan)?;
        }
        Ok(())
    }

    /// Forces a fan into default/firmware control.
    pub fn force_auto(&mut self, chip: usize, fan: usize) -> Result<()> {
        self.check(chip, fan)?;
        self.control(chip, fan)?.set_default()?;
        self.touched.remove(&(chip, fan));
        Ok(())
    }

    /// Reads the current LHM control state for external persistence.
    pub fn fan_reg_state(&mut self, chip: usize, fan: usize) -> Result<FanRegState> {
        self.check(chip, fan)?;
        Ok(self.control(chip, fan)?.reg_state())
    }

    /// Restores a previously captured LHM control state.
    pub fn apply_fan_reg_state(
        &mut self,
        chip: usize,
        fan: usize,
        state: FanRegState,
    ) -> Result<()> {
        self.check(chip, fan)?;
        apply_state(self.control(chip, fan)?, state)?;
        self.touched.remove(&(chip, fan));
        Ok(())
    }

    /// LHM abstracts raw EC registers, so register dumps are no longer
    /// available from this backend.
    pub fn ec_dump(&mut self, chip: usize) -> Result<Vec<Option<u8>>> {
        Err(HwError::NoRawRegisters { chip })
    }

    fn check(&self, chip: usize, fan: usize) -> Result<()> {
        let Some(info) = self.infos.get(chip) else {
            return Err(HwError::InvalidChip {
                chip,
                chips: self.infos.len(),
            });
        };
        if fan >= info.control_count {
            return Err(HwError::InvalidFan {
                chip,
                fan,
                controls: info.control_count,
            });
        }
        Ok(())
    }

    fn control(&self, chip: usize, fan: usize) -> Result<&LhmSensor> {
        self.devices
            .get(chip)
            .and_then(|device| device.controls.get(fan))
            .ok_or_else(|| HwError::InvalidFan {
                chip,
                fan,
                controls: self.infos.get(chip).map_or(0, |info| info.control_count),
            })
    }

    fn restore_control(&mut self, chip: usize, fan: usize) -> Result<()> {
        let state = self.devices[chip].original_controls[fan];
        apply_state(self.control(chip, fan)?, state)?;
        self.touched.remove(&(chip, fan));
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.restore_on_drop {
            let _ = self.restore_all();
        }
    }
}

impl Device {
    fn snapshot(&self) -> ChipSnapshot {
        let mut fans = Vec::with_capacity(self.info.fan_count);
        for index in 0..self.info.fan_count {
            let rpm = self
                .fan_rpms
                .get(index)
                .and_then(Option::as_ref)
                .and_then(|sensor| sensor.value())
                .filter(|value| *value >= 0.0);
            let duty = self.controls.get(index).map(LhmSensor::fan_duty);
            fans.push(FanStatus { rpm, duty });
        }

        ChipSnapshot {
            fans,
            temps: self
                .temps
                .iter()
                .map(|sensor| {
                    sensor
                        .value()
                        .filter(|value| (-100.0..200.0).contains(value))
                })
                .collect(),
            powers: self
                .powers
                .iter()
                .map(|sensor| sensor.value().filter(|value| *value >= 0.0))
                .collect(),
        }
    }
}

fn collect_hardware(hardware: &LhmHardware, devices: &mut Vec<Device>, notes: &mut Vec<String>) {
    let sensors = hardware.sensors();
    let mut temps = Vec::new();
    let mut powers = Vec::new();
    let mut rpms = Vec::new();
    let mut controls = Vec::new();

    for sensor in sensors {
        match sensor.sensor_type() {
            SENSOR_TEMPERATURE if is_live_temperature_sensor(&sensor) => temps.push(sensor),
            SENSOR_TEMPERATURE => {}
            SENSOR_POWER => powers.push(sensor),
            SENSOR_FAN => rpms.push(sensor),
            SENSOR_CONTROL if sensor.has_control() => controls.push(sensor),
            _ => {}
        }
    }

    if !temps.is_empty() || !powers.is_empty() || !rpms.is_empty() || !controls.is_empty() {
        let name = hardware.name();
        let identifier = hardware.identifier();
        let hardware_type = hardware.hardware_type();
        let fan_count = rpms.len().max(controls.len());
        let control_count = controls.len();
        if !rpms.is_empty() && !controls.is_empty() && rpms.len() != controls.len() {
            notes.push(format!(
                "{name}: LHM reported {} RPM fan sensor(s) and {} control sensor(s); controls are matched by order",
                rpms.len(),
                controls.len()
            ));
        }

        let mut fan_rpms: Vec<Option<LhmSensor>> = (0..fan_count).map(|_| None).collect();
        for (index, sensor) in rpms.into_iter().enumerate() {
            fan_rpms[index] = Some(sensor);
        }
        let original_controls = controls.iter().map(LhmSensor::reg_state).collect();

        devices.push(Device {
            info: ChipInfo {
                name,
                address: stable_u16(&identifier),
                version: hardware_type as u8,
                slot: slot_for_hardware(hardware_type),
                fan_count,
                control_count,
                temp_count: temps.len(),
                temp_labels: temps.iter().map(LhmSensor::name).collect(),
                power_labels: powers.iter().map(LhmSensor::name).collect(),
            },
            temps,
            powers,
            fan_rpms,
            controls,
            original_controls,
        });
    }

    for child in hardware.children() {
        collect_hardware(&child, devices, notes);
    }
}

fn apply_state(control: &LhmSensor, state: FanRegState) -> Result<()> {
    match state {
        FanRegState::Default | FanRegState::Unknown => control.set_default(),
        FanRegState::Software { percent } => control.set_software(percent),
    }
}

fn duty_to_percent(duty: u8) -> f32 {
    duty as f32 * 100.0 / 255.0
}

fn stable_u16(value: &str) -> u16 {
    let mut hash = 0x811c_9dc5u32;
    for byte in value.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    ((hash >> 16) ^ hash) as u16
}

fn slot_for_hardware(hardware_type: i32) -> u8 {
    match hardware_type {
        HARDWARE_CPU => SLOT_CPU,
        HARDWARE_GPU_NVIDIA | HARDWARE_GPU_AMD | HARDWARE_GPU_INTEL => SLOT_GPU,
        HARDWARE_STORAGE => SLOT_STORAGE,
        HARDWARE_BATTERY => SLOT_OTHER,
        _ => SLOT_MOTHERBOARD,
    }
}

fn is_live_temperature_sensor(sensor: &LhmSensor) -> bool {
    is_live_temperature_name(&sensor.name())
}

fn is_live_temperature_name(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return true;
    }

    // LHM exposes some DIMM and storage metadata as SensorType.Temperature:
    // threshold constants like "Critical Temperature" and "Thermal Sensor
    // High Limit", plus SPD metadata like "Temperature Sensor Resolution".
    // They are not live thermal readings and must not trip calibration's
    // temperature guard or appear as graphable sensors.
    !(normalized.contains("limit")
        || normalized.contains("resolution")
        || (normalized.contains("temperature")
            && (normalized.contains("warning") || normalized.contains("critical"))))
}

#[cfg(test)]
mod tests {
    use super::is_live_temperature_name;

    #[test]
    fn keeps_live_temperature_names() {
        assert!(is_live_temperature_name("CPU"));
        assert!(is_live_temperature_name("Core (Tctl/Tdie)"));
        assert!(is_live_temperature_name("GPU Hot Spot"));
        assert!(is_live_temperature_name("Composite Temperature"));
        assert!(is_live_temperature_name("Temperature #8"));
    }

    #[test]
    fn rejects_threshold_and_metadata_temperature_names() {
        assert!(!is_live_temperature_name("Warning Temperature"));
        assert!(!is_live_temperature_name("Critical Temperature"));
        assert!(!is_live_temperature_name("Temperature Sensor Resolution"));
        assert!(!is_live_temperature_name("Thermal Sensor Low Limit"));
        assert!(!is_live_temperature_name("Thermal Sensor High Limit"));
        assert!(!is_live_temperature_name("Thermal Sensor Critical Limit"));
    }
}

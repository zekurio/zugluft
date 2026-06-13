use std::ffi::{CStr, c_char, c_void};
use std::iter::once;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::Arc;

use windows_sys::Win32::Foundation::{FreeLibrary, GetLastError, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use crate::{FanDuty, FanRegState, HwError, Result};

const BRIDGE_DLL: &str = "zugluft-lhm-bridge.dll";
const CONTROL_MODE_SOFTWARE: i32 = 1;
const CONTROL_MODE_DEFAULT: i32 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
struct CreateComputerResult {
    computer: *mut c_void,
    error: *mut c_char,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PtrArray {
    len: usize,
    data: *const *mut c_void,
    handle: *mut c_void,
}

type FreeString = unsafe extern "C" fn(*mut c_char);
type FreePtrArray = unsafe extern "C" fn(PtrArray);
type CreateComputer = unsafe extern "C" fn() -> CreateComputerResult;
type UpdateComputer = unsafe extern "C" fn(*mut c_void);
type FreeComputer = unsafe extern "C" fn(*mut c_void);
type GetComputerHardware = unsafe extern "C" fn(*mut c_void) -> PtrArray;
type GetHardwareChildren = unsafe extern "C" fn(*mut c_void) -> PtrArray;
type GetHardwareSensors = unsafe extern "C" fn(*mut c_void) -> PtrArray;
type GetHardwareString = unsafe extern "C" fn(*mut c_void) -> *mut c_char;
type GetHardwareI32 = unsafe extern "C" fn(*mut c_void) -> i32;
type FreeHardware = unsafe extern "C" fn(*mut c_void);
type GetSensorString = unsafe extern "C" fn(*mut c_void) -> *mut c_char;
type GetSensorI32 = unsafe extern "C" fn(*mut c_void) -> i32;
type GetSensorF32 = unsafe extern "C" fn(*mut c_void) -> f32;
type GetSensorBool = unsafe extern "C" fn(*mut c_void) -> i32;
type SetSensorF32 = unsafe extern "C" fn(*mut c_void, f32) -> i32;
type SetSensorDefault = unsafe extern "C" fn(*mut c_void) -> i32;
type FreeSensor = unsafe extern "C" fn(*mut c_void);

#[derive(Clone)]
pub(crate) struct Bridge {
    inner: Arc<BridgeInner>,
}

struct BridgeInner {
    module: HMODULE,
    free_string: FreeString,
    free_ptr_array: FreePtrArray,
    create_computer: CreateComputer,
    update_computer: UpdateComputer,
    free_computer: FreeComputer,
    get_computer_hardware: GetComputerHardware,
    get_hardware_children: GetHardwareChildren,
    get_hardware_sensors: GetHardwareSensors,
    get_hardware_identifier: GetHardwareString,
    get_hardware_name: GetHardwareString,
    get_hardware_type: GetHardwareI32,
    free_hardware: FreeHardware,
    get_sensor_identifier: GetSensorString,
    get_sensor_name: GetSensorString,
    get_sensor_type: GetSensorI32,
    get_sensor_value: GetSensorF32,
    get_sensor_has_control: GetSensorBool,
    get_sensor_control_mode: GetSensorI32,
    get_sensor_control_software_value: GetSensorF32,
    set_sensor_software: SetSensorF32,
    set_sensor_default: SetSensorDefault,
    free_sensor: FreeSensor,
}

unsafe impl Send for BridgeInner {}
unsafe impl Sync for BridgeInner {}

impl Bridge {
    pub(crate) fn load() -> Result<Self> {
        let candidates = candidate_paths();
        for path in &candidates {
            if !path.is_file() {
                continue;
            }
            return Self::load_from(path);
        }
        Err(HwError::BridgeNotFound {
            searched: candidates,
        })
    }

    pub(crate) fn create_computer(&self) -> Result<LhmComputer> {
        let result = unsafe { (self.inner.create_computer)() };
        if !result.error.is_null() {
            let error = self.take_string(result.error);
            return Err(classify_lhm_error(error));
        }
        if result.computer.is_null() {
            return Err(HwError::Lhm(
                "bridge returned a null LibreHardwareMonitor computer".to_string(),
            ));
        }
        Ok(LhmComputer {
            ptr: result.computer,
            bridge: self.clone(),
        })
    }

    fn load_from(path: &Path) -> Result<Self> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(once(0)).collect();
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            return Err(HwError::BridgeLoad {
                path: path.to_path_buf(),
                code: unsafe { GetLastError() },
            });
        }

        macro_rules! symbol {
            ($name:literal, $ty:ty) => {{
                let proc = unsafe { GetProcAddress(module, concat!($name, "\0").as_ptr()) };
                match proc {
                    Some(proc) => unsafe {
                        mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(proc)
                    },
                    None => {
                        unsafe { FreeLibrary(module) };
                        return Err(HwError::MissingExport {
                            path: path.to_path_buf(),
                            symbol: $name,
                        });
                    }
                }
            }};
        }

        Ok(Self {
            inner: Arc::new(BridgeInner {
                module,
                free_string: symbol!("free_string", FreeString),
                free_ptr_array: symbol!("free_ptr_array", FreePtrArray),
                create_computer: symbol!("create_computer", CreateComputer),
                update_computer: symbol!("update_computer", UpdateComputer),
                free_computer: symbol!("free_computer", FreeComputer),
                get_computer_hardware: symbol!("get_computer_hardware", GetComputerHardware),
                get_hardware_children: symbol!("get_hardware_children", GetHardwareChildren),
                get_hardware_sensors: symbol!("get_hardware_sensors", GetHardwareSensors),
                get_hardware_identifier: symbol!("get_hardware_identifier", GetHardwareString),
                get_hardware_name: symbol!("get_hardware_name", GetHardwareString),
                get_hardware_type: symbol!("get_hardware_type", GetHardwareI32),
                free_hardware: symbol!("free_hardware", FreeHardware),
                get_sensor_identifier: symbol!("get_sensor_identifier", GetSensorString),
                get_sensor_name: symbol!("get_sensor_name", GetSensorString),
                get_sensor_type: symbol!("get_sensor_type", GetSensorI32),
                get_sensor_value: symbol!("get_sensor_value", GetSensorF32),
                get_sensor_has_control: symbol!("get_sensor_has_control", GetSensorBool),
                get_sensor_control_mode: symbol!("get_sensor_control_mode", GetSensorI32),
                get_sensor_control_software_value: symbol!(
                    "get_sensor_control_software_value",
                    GetSensorF32
                ),
                set_sensor_software: symbol!("set_sensor_software", SetSensorF32),
                set_sensor_default: symbol!("set_sensor_default", SetSensorDefault),
                free_sensor: symbol!("free_sensor", FreeSensor),
            }),
        })
    }

    fn take_string(&self, ptr: *mut c_char) -> String {
        if ptr.is_null() {
            return String::new();
        }
        let value = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().to_string();
        unsafe { (self.inner.free_string)(ptr) };
        value
    }

    fn ptrs(&self, array: PtrArray) -> Vec<*mut c_void> {
        let values = if array.data.is_null() || array.len == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(array.data, array.len) }.to_vec()
        };
        unsafe { (self.inner.free_ptr_array)(array) };
        values
    }
}

impl Drop for BridgeInner {
    fn drop(&mut self) {
        unsafe { FreeLibrary(self.module) };
    }
}

pub(crate) struct LhmComputer {
    ptr: *mut c_void,
    bridge: Bridge,
}

impl LhmComputer {
    pub(crate) fn update(&self) {
        unsafe { (self.bridge.inner.update_computer)(self.ptr) };
    }

    pub(crate) fn hardware(&self) -> Vec<LhmHardware> {
        let array = unsafe { (self.bridge.inner.get_computer_hardware)(self.ptr) };
        self.bridge
            .ptrs(array)
            .into_iter()
            .filter(|ptr| !ptr.is_null())
            .map(|ptr| LhmHardware {
                ptr,
                bridge: self.bridge.clone(),
            })
            .collect()
    }
}

impl Drop for LhmComputer {
    fn drop(&mut self) {
        unsafe { (self.bridge.inner.free_computer)(self.ptr) };
        self.ptr = null_mut();
    }
}

pub(crate) struct LhmHardware {
    ptr: *mut c_void,
    bridge: Bridge,
}

impl LhmHardware {
    pub(crate) fn identifier(&self) -> String {
        self.string(self.bridge.inner.get_hardware_identifier)
    }

    pub(crate) fn name(&self) -> String {
        self.string(self.bridge.inner.get_hardware_name)
    }

    pub(crate) fn hardware_type(&self) -> i32 {
        unsafe { (self.bridge.inner.get_hardware_type)(self.ptr) }
    }

    pub(crate) fn children(&self) -> Vec<LhmHardware> {
        let array = unsafe { (self.bridge.inner.get_hardware_children)(self.ptr) };
        self.bridge
            .ptrs(array)
            .into_iter()
            .filter(|ptr| !ptr.is_null())
            .map(|ptr| LhmHardware {
                ptr,
                bridge: self.bridge.clone(),
            })
            .collect()
    }

    pub(crate) fn sensors(&self) -> Vec<LhmSensor> {
        let array = unsafe { (self.bridge.inner.get_hardware_sensors)(self.ptr) };
        self.bridge
            .ptrs(array)
            .into_iter()
            .filter(|ptr| !ptr.is_null())
            .map(|ptr| LhmSensor {
                ptr,
                bridge: self.bridge.clone(),
            })
            .collect()
    }

    fn string(&self, f: GetHardwareString) -> String {
        self.bridge.take_string(unsafe { f(self.ptr) })
    }
}

impl Drop for LhmHardware {
    fn drop(&mut self) {
        unsafe { (self.bridge.inner.free_hardware)(self.ptr) };
        self.ptr = null_mut();
    }
}

pub(crate) struct LhmSensor {
    ptr: *mut c_void,
    bridge: Bridge,
}

impl LhmSensor {
    pub(crate) fn identifier(&self) -> String {
        self.string(self.bridge.inner.get_sensor_identifier)
    }

    pub(crate) fn name(&self) -> String {
        self.string(self.bridge.inner.get_sensor_name)
    }

    pub(crate) fn sensor_type(&self) -> i32 {
        unsafe { (self.bridge.inner.get_sensor_type)(self.ptr) }
    }

    pub(crate) fn value(&self) -> Option<f32> {
        finite(unsafe { (self.bridge.inner.get_sensor_value)(self.ptr) })
    }

    pub(crate) fn has_control(&self) -> bool {
        unsafe { (self.bridge.inner.get_sensor_has_control)(self.ptr) != 0 }
    }

    pub(crate) fn fan_duty(&self) -> FanDuty {
        match self.control_mode() {
            CONTROL_MODE_DEFAULT => FanDuty::Auto,
            CONTROL_MODE_SOFTWARE => FanDuty::Manual {
                percent: self
                    .software_value()
                    .or_else(|| self.value())
                    .unwrap_or(0.0),
            },
            _ => self
                .value()
                .map(|percent| FanDuty::Manual { percent })
                .unwrap_or(FanDuty::Auto),
        }
    }

    pub(crate) fn reg_state(&self) -> FanRegState {
        match self.control_mode() {
            CONTROL_MODE_DEFAULT => FanRegState::Default,
            CONTROL_MODE_SOFTWARE => self
                .software_value()
                .map(|percent| FanRegState::Software { percent })
                .unwrap_or(FanRegState::Unknown),
            _ => FanRegState::Unknown,
        }
    }

    pub(crate) fn set_software(&self, percent: f32) -> Result<()> {
        let percent = percent.clamp(0.0, 100.0);
        let ok = unsafe { (self.bridge.inner.set_sensor_software)(self.ptr, percent) };
        if ok != 0 {
            Ok(())
        } else {
            Err(HwError::Lhm(format!(
                "failed to set LHM control `{}` to {percent:.0} %",
                self.identifier()
            )))
        }
    }

    pub(crate) fn set_default(&self) -> Result<()> {
        let ok = unsafe { (self.bridge.inner.set_sensor_default)(self.ptr) };
        if ok != 0 {
            Ok(())
        } else {
            Err(HwError::Lhm(format!(
                "failed to restore LHM control `{}` to default",
                self.identifier()
            )))
        }
    }

    fn string(&self, f: GetSensorString) -> String {
        self.bridge.take_string(unsafe { f(self.ptr) })
    }

    fn control_mode(&self) -> i32 {
        unsafe { (self.bridge.inner.get_sensor_control_mode)(self.ptr) }
    }

    fn software_value(&self) -> Option<f32> {
        finite(unsafe { (self.bridge.inner.get_sensor_control_software_value)(self.ptr) })
    }
}

impl Drop for LhmSensor {
    fn drop(&mut self) {
        unsafe { (self.bridge.inner.free_sensor)(self.ptr) };
        self.ptr = null_mut();
    }
}

fn finite(value: f32) -> Option<f32> {
    value.is_finite().then_some(value)
}

fn classify_lhm_error(error: String) -> HwError {
    if error.contains("Access is denied") || error.contains("UnauthorizedAccess") {
        HwError::AccessDenied
    } else {
        HwError::Lhm(error)
    }
}

fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(value) = std::env::var_os("ZUGLUFT_LHM_BRIDGE") {
        paths.push(PathBuf::from(value));
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        paths.push(dir.join(BRIDGE_DLL));
        paths.push(dir.join("modules").join(BRIDGE_DLL));
    }

    if let Some(program_data) = std::env::var_os("ProgramData") {
        paths.push(PathBuf::from(program_data).join("zugluft").join(BRIDGE_DLL));
    }

    if let Some(path) = option_env!("ZUGLUFT_BUILT_LHM_BRIDGE") {
        paths.push(PathBuf::from(path));
    }

    dedupe(paths)
}

fn dedupe(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if !unique.iter().any(|existing: &PathBuf| existing == &path) {
            unique.push(path);
        }
    }
    unique
}

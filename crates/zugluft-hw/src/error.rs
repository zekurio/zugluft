use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, HwError>;

#[derive(Debug, Error)]
pub enum HwError {
    #[error(
        "the LibreHardwareMonitor bridge DLL was not found{}; build with the .NET SDK or set ZUGLUFT_LHM_BRIDGE",
        searched_suffix(searched)
    )]
    BridgeNotFound { searched: Vec<PathBuf> },

    #[error("could not load LibreHardwareMonitor bridge `{}`: Win32 error {code}", path.display())]
    BridgeLoad { path: PathBuf, code: u32 },

    #[error("LibreHardwareMonitor bridge `{}` is missing export `{symbol}`", path.display())]
    MissingExport { path: PathBuf, symbol: &'static str },

    #[error("LibreHardwareMonitor failed: {0}")]
    Lhm(String),

    #[error("access to hardware was denied; zugluft-service must run elevated")]
    AccessDenied,

    #[error("no supported LHM hardware sensors or fan controls were found")]
    NoSupportedHardware,

    #[error("fan index {fan} out of range: chip {chip} has {controls} controllable fans")]
    InvalidFan {
        chip: usize,
        fan: usize,
        controls: usize,
    },

    #[error("chip index {chip} out of range ({chips} chips detected)")]
    InvalidChip { chip: usize, chips: usize },

    #[error("chip {chip} has no raw environment-controller register dump in the LHM backend")]
    NoRawRegisters { chip: usize },

    // Kept so older CLI handling remains source-compatible; the LHM backend
    // does not use this error.
    #[error("timed out waiting for the {what} mutex (another monitoring tool may be holding it)")]
    MutexTimeout { what: &'static str },
}

fn searched_suffix(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        String::new()
    } else {
        format!(
            " (searched: {})",
            paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("; ")
        )
    }
}

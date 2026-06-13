use super::*;

/// Persisted pre-manual LHM control state, one line per fan:
/// `c0f1 mode=software percent=42.0`
pub(crate) struct Baseline {
    entries: BTreeMap<(usize, usize), FanRegState>,
}

impl Baseline {
    pub(crate) fn path() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("zugluft").join("fan-baseline.txt")
    }

    pub(crate) fn load() -> Self {
        let mut entries = BTreeMap::new();
        if let Ok(text) = fs::read_to_string(Self::path()) {
            for line in text.lines() {
                if let Some(entry) = Self::parse_line(line) {
                    entries.insert(entry.0, entry.1);
                }
            }
        }
        Self { entries }
    }

    pub(crate) fn parse_line(line: &str) -> Option<((usize, usize), FanRegState)> {
        let mut parts = line.split_whitespace();
        let key = parts.next()?;
        let (chip, fan) = key.strip_prefix('c')?.split_once('f')?;
        let key = (chip.parse().ok()?, fan.parse().ok()?);

        let mut mode = None;
        let mut percent = None;
        for part in parts {
            let (name, value) = part.split_once('=')?;
            match name {
                "mode" => mode = Some(value),
                "percent" => percent = value.parse::<f32>().ok(),
                // Pre-LHM baselines stored raw IT87 registers. Those cannot
                // be applied by LHM, but default/firmware control is the
                // closest safe restoration.
                "ctrl" => mode = Some("default"),
                _ => {}
            }
        }
        let state = match mode? {
            "default" => FanRegState::Default,
            "software" => FanRegState::Software { percent: percent? },
            "unknown" => FanRegState::Unknown,
            _ => return None,
        };
        Some((key, state))
    }

    pub(crate) fn store(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let mut text = String::new();
        for (&(chip, fan), state) in &self.entries {
            text.push_str(&format!("c{chip}f{fan}"));
            match state {
                FanRegState::Default => text.push_str(" mode=default"),
                FanRegState::Software { percent } => {
                    text.push_str(&format!(" mode=software percent={percent:.3}"));
                }
                FanRegState::Unknown => text.push_str(" mode=unknown"),
            }
            text.push('\n');
        }
        let _ = fs::write(path, text);
    }

    pub(crate) fn contains(&self, chip: usize, fan: usize) -> bool {
        self.entries.contains_key(&(chip, fan))
    }

    pub(crate) fn insert(&mut self, chip: usize, fan: usize, state: FanRegState) {
        self.entries.insert((chip, fan), state);
    }

    pub(crate) fn remove(&mut self, chip: usize, fan: usize) -> Option<FanRegState> {
        self.entries.remove(&(chip, fan))
    }
}

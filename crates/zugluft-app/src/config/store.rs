use super::*;

/// Persist a display name override for a hardware channel; `None` removes
/// the override (back to the "Temp N"/"Fan N" default). Edits in place so
/// user comments survive.
pub fn save_chip_name(chip: &str, key: &str, name: Option<&str>) {
    edit_config(|doc| match name {
        Some(name) => doc["chips"][chip][key] = toml_edit::value(name),
        None => {
            if let Some(table) = doc
                .get_mut("chips")
                .and_then(|chips| chips.get_mut(chip))
                .and_then(|chip| chip.as_table_like_mut())
            {
                table.remove(key);
            }
        }
    });
}

/// Renames a `[[custom]]` entry, matching by id (or by the positional
/// fallback id for entries without one).
pub fn save_custom_name(id: &str, name: &str) {
    edit_config(|doc| {
        let Some(entries) = doc
            .get_mut("custom")
            .and_then(|custom| custom.as_array_of_tables_mut())
        else {
            return;
        };
        for (i, entry) in entries.iter_mut().enumerate() {
            let entry_id = entry
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| fallback_custom_id(i));
            if entry_id == id {
                entry["name"] = toml_edit::value(name);
                return;
            }
        }
    });
}

/// Inserts or replaces a `[[custom]]` entry, matched by id (or the
/// positional fallback id). The whole entry is rewritten — the sensor
/// editor owns these tables, like the curve editor owns `[[curve]]`.
pub fn save_custom(def: &CustomSensorDef) {
    edit_config(|doc| {
        let mut table = toml_edit::Table::new();
        table["id"] = toml_edit::value(&def.id);
        table["name"] = toml_edit::value(&def.name);
        table["kind"] = toml_edit::value(match def.kind {
            CustomKind::Average => "average",
            CustomKind::Min => "min",
            CustomKind::Max => "max",
        });
        let mut inputs = toml_edit::Array::new();
        for input in &def.inputs {
            let mut entry = toml_edit::InlineTable::new();
            entry.insert("chip", input.chip.as_str().into());
            entry.insert("temp", (input.temp as i64).into());
            // Only weights that matter are written, keeping configs tidy;
            // a missing weight loads back as 1.0.
            if (input.weight - 1.0).abs() > f32::EPSILON {
                entry.insert("weight", round1(input.weight).into());
            }
            inputs.push(entry);
        }
        table["inputs"] = toml_edit::value(inputs);

        let entries = doc["custom"]
            .or_insert(toml_edit::Item::ArrayOfTables(
                toml_edit::ArrayOfTables::new(),
            ))
            .as_array_of_tables_mut();
        let Some(entries) = entries else { return };
        for (i, entry) in entries.iter_mut().enumerate() {
            let entry_id = entry
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| fallback_custom_id(i));
            if entry_id == def.id {
                *entry = table;
                return;
            }
        }
        entries.push(table);
    });
}

/// Removes a `[[custom]]` entry by id (or by the positional fallback id
/// for entries without one).
pub fn delete_custom(id: &str) {
    edit_config(|doc| {
        let Some(entries) = doc
            .get_mut("custom")
            .and_then(|custom| custom.as_array_of_tables_mut())
        else {
            return;
        };
        let position = entries.iter().enumerate().position(|(i, entry)| {
            entry
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| fallback_custom_id(i))
                == id
        });
        if let Some(position) = position {
            entries.remove(position);
        }
    });
}

/// Inserts or replaces a `[[curve]]` entry, matched by id (or by the
/// positional fallback id for entries without one). The whole entry is
/// rewritten — the curve editor owns these tables, unlike hand-kept names.
pub fn save_curve(def: &CurveDef) {
    edit_config(|doc| {
        let mut table = toml_edit::Table::new();
        table["id"] = toml_edit::value(&def.id);
        table["name"] = toml_edit::value(&def.name);
        let mut source = toml_edit::InlineTable::new();
        match &def.source {
            CurveSource::Temp { chip, temp } => {
                source.insert("chip", chip.as_str().into());
                source.insert("temp", (*temp as i64).into());
            }
            CurveSource::Custom { custom } => {
                source.insert("custom", custom.as_str().into());
            }
        }
        table["source"] = toml_edit::value(source);
        let window = def.window.sanitized();
        let mut window_table = toml_edit::InlineTable::new();
        window_table.insert("temp_min", round1(window.temp_min).into());
        window_table.insert("temp_max", round1(window.temp_max).into());
        window_table.insert("duty_min", round1(window.duty_min).into());
        window_table.insert("duty_max", round1(window.duty_max).into());
        table["window"] = toml_edit::value(window_table);
        let mut functions = toml_edit::Array::new();
        for function in def.processing_functions() {
            let mut entry = toml_edit::InlineTable::new();
            match function {
                CurveFunction::Identity => {
                    entry.insert("kind", "identity".into());
                }
                CurveFunction::Standard { hysteresis } => {
                    entry.insert("kind", "standard".into());
                    let hysteresis = hysteresis.sanitized();
                    let mut hysteresis_table = toml_edit::InlineTable::new();
                    hysteresis_table.insert("degrees", round1(hysteresis.degrees).into());
                    hysteresis_table.insert("delay_ms", (hysteresis.delay_ms as i64).into());
                    hysteresis_table.insert("only_downward", hysteresis.only_downward.into());
                    entry.insert("hysteresis", hysteresis_table.into());
                }
                CurveFunction::Ema { alpha } => {
                    entry.insert("kind", "ema".into());
                    entry.insert("alpha", round2(alpha.clamp(0.01, 1.0)).into());
                }
            }
            functions.push(entry);
        }
        table["functions"] = toml_edit::value(functions);
        match def.kind.sanitized() {
            CurveKind::Graph { points } => {
                table["kind"] = toml_edit::value("graph");
                let mut array = toml_edit::Array::new();
                for (temp, percent) in points {
                    let mut pair = toml_edit::Array::new();
                    // One decimal is plenty for °C and target steps.
                    pair.push(round1(temp));
                    pair.push(round1(percent));
                    array.push(pair);
                }
                table["points"] = toml_edit::value(array);
            }
            CurveKind::Trigger {
                threshold,
                before,
                after,
            } => {
                table["kind"] = toml_edit::value("trigger");
                table["threshold"] = toml_edit::value(round1(threshold));
                table["before"] = toml_edit::value(round1(before));
                table["after"] = toml_edit::value(round1(after));
            }
            CurveKind::Linear { start, end } => {
                table["kind"] = toml_edit::value("linear");
                table["start"] = toml_edit::value(point_array(start));
                table["end"] = toml_edit::value(point_array(end));
            }
        }

        let entries = doc["curve"]
            .or_insert(toml_edit::Item::ArrayOfTables(
                toml_edit::ArrayOfTables::new(),
            ))
            .as_array_of_tables_mut();
        let Some(entries) = entries else { return };
        for (i, entry) in entries.iter_mut().enumerate() {
            let entry_id = entry
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| fallback_curve_id(i));
            if entry_id == def.id {
                *entry = table;
                return;
            }
        }
        entries.push(table);
    });
}

fn round1(value: f32) -> f64 {
    (value as f64 * 10.0).round() / 10.0
}

fn round2(value: f32) -> f64 {
    (value as f64 * 100.0).round() / 100.0
}

fn point_array((temp, percent): (f32, f32)) -> toml_edit::Array {
    let mut pair = toml_edit::Array::new();
    pair.push(round1(temp));
    pair.push(round1(percent));
    pair
}

/// Removes a `[[curve]]` entry by id.
pub fn delete_curve(id: &str) {
    edit_config(|doc| {
        let Some(entries) = doc
            .get_mut("curve")
            .and_then(|curve| curve.as_array_of_tables_mut())
        else {
            return;
        };
        let position = entries.iter().enumerate().position(|(i, entry)| {
            entry
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| fallback_curve_id(i))
                == id
        });
        if let Some(position) = position {
            entries.remove(position);
        }
    });
}

/// Adds or removes one dashboard pin while preserving the other pins'
/// order. Pinning appends to the end, so a later drag/reorder feature can
/// use the same `[[dashboard.item]]` order directly.
pub fn set_dashboard_pinned(item: &DashboardItem, pinned: bool) {
    edit_config(|doc| {
        set_dashboard_pinned_in_doc(doc, item, pinned);
    });
}

fn set_dashboard_pinned_in_doc(
    doc: &mut toml_edit::DocumentMut,
    item: &DashboardItem,
    pinned: bool,
) {
    normalize_dashboard_table(doc);

    if let Some(entries) = doc
        .get_mut("dashboard")
        .and_then(|dashboard| dashboard.get_mut("item"))
        .and_then(|item| item.as_array_of_tables_mut())
    {
        let mut index = 0;
        while index < entries.len() {
            if dashboard_table_matches(entries.get(index), item) {
                entries.remove(index);
            } else {
                index += 1;
            }
        }
    }

    if pinned {
        if doc.get("dashboard").is_none() {
            doc.as_table_mut()
                .insert("dashboard", toml_edit::Item::Table(toml_edit::Table::new()));
        }
        let entries = doc["dashboard"]["item"]
            .or_insert(toml_edit::Item::ArrayOfTables(
                toml_edit::ArrayOfTables::new(),
            ))
            .as_array_of_tables_mut();
        if let Some(entries) = entries {
            entries.push(dashboard_table(item));
        }
    }

    prune_empty_dashboard(doc);
}

fn normalize_dashboard_table(doc: &mut toml_edit::DocumentMut) {
    let Some(item) = doc.as_table_mut().remove("dashboard") else {
        return;
    };
    match item.into_table() {
        Ok(table) => {
            doc.as_table_mut()
                .insert("dashboard", toml_edit::Item::Table(table));
        }
        Err(item) => {
            doc.as_table_mut().insert("dashboard", item);
        }
    }
}

fn dashboard_table(item: &DashboardItem) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    table["kind"] = toml_edit::value(item.kind.as_str());
    match item.kind {
        DashboardItemKind::Fan | DashboardItemKind::Sensor => {
            if let Some(chip) = item.chip() {
                table["chip"] = toml_edit::value(chip);
            }
            if let Some(channel) = item.channel() {
                table["channel"] = toml_edit::value(channel);
            }
        }
        DashboardItemKind::Curve => {
            if let Some(id) = item.id() {
                table["id"] = toml_edit::value(id);
            }
        }
    }
    table
}

fn dashboard_table_matches(entry: Option<&toml_edit::Table>, item: &DashboardItem) -> bool {
    let Some(entry) = entry else {
        return false;
    };
    if entry.get("kind").and_then(|kind| kind.as_str()) != Some(item.kind.as_str()) {
        return false;
    }
    match item.kind {
        DashboardItemKind::Fan | DashboardItemKind::Sensor => {
            entry.get("chip").and_then(|chip| chip.as_str()) == item.chip()
                && entry.get("channel").and_then(|channel| channel.as_str()) == item.channel()
        }
        DashboardItemKind::Curve => entry.get("id").and_then(|id| id.as_str()) == item.id(),
    }
}

fn prune_empty_dashboard(doc: &mut toml_edit::DocumentMut) {
    let item_empty = doc
        .get("dashboard")
        .and_then(|dashboard| dashboard.get("item"))
        .and_then(|item| item.as_array_of_tables())
        .is_some_and(|items| items.is_empty());
    if item_empty
        && let Some(dashboard) = doc
            .get_mut("dashboard")
            .and_then(|dashboard| dashboard.as_table_like_mut())
    {
        dashboard.remove("item");
    }

    let dashboard_empty = doc
        .get("dashboard")
        .and_then(|dashboard| dashboard.as_table_like())
        .is_some_and(|dashboard| dashboard.is_empty());
    if dashboard_empty {
        doc.as_table_mut().remove("dashboard");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_into_empty_inline_dashboard_table() {
        let mut doc = "dashboard = {}\n"
            .parse::<toml_edit::DocumentMut>()
            .expect("valid toml");
        let item = DashboardItem::fan("nct6798", "fan1");

        set_dashboard_pinned_in_doc(&mut doc, &item, true);
        let text = doc.to_string();

        assert!(text.contains("[[dashboard.item]]"));
        let config: NamesConfig = toml::from_str(&text).expect("dashboard pin loads");
        assert!(config.is_dashboard_pinned(&item));
    }

    #[test]
    fn unpin_prunes_empty_dashboard_table() {
        let mut doc =
            "[[dashboard.item]]\nkind = \"fan\"\nchip = \"nct6798\"\nchannel = \"fan1\"\n"
                .parse::<toml_edit::DocumentMut>()
                .expect("valid toml");
        let item = DashboardItem::fan("nct6798", "fan1");

        set_dashboard_pinned_in_doc(&mut doc, &item, false);

        assert!(doc.get("dashboard").is_none());
    }
}

/// Hides or shows a full chip/device by editing the `[hidden]` table.
pub fn set_device_hidden(chip: &str, hidden: bool) {
    set_hidden(chip, HIDDEN_DEVICE_KEY, hidden);
}

/// Hides or shows a whole category on a chip by editing the `[hidden]`
/// table.
pub fn set_category_hidden(chip: &str, category: HiddenCategory, hidden: bool) {
    set_hidden(chip, category.key(), hidden);
}

/// Hides or shows a channel (`fanN`/`tempN`/`powerN`) by editing the
/// `[hidden]` table; empty per-chip lists are removed again.
pub fn set_hidden(chip: &str, key: &str, hidden: bool) {
    edit_config(|doc| {
        let mut keys: Vec<String> = doc
            .get("hidden")
            .and_then(|table| table.get(chip))
            .and_then(|entry| entry.as_array())
            .map(|array| {
                array
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        keys.retain(|existing| existing != key);
        if hidden {
            keys.push(key.to_string());
        }
        if keys.is_empty() {
            if let Some(table) = doc
                .get_mut("hidden")
                .and_then(|table| table.as_table_like_mut())
            {
                table.remove(chip);
            }
        } else {
            let mut array = toml_edit::Array::new();
            for key in &keys {
                array.push(key.as_str());
            }
            doc["hidden"][chip] = toml_edit::value(array);
        }
    });
}

/// Sets or clears a graph line's color override (`"#rrggbb"`).
pub fn set_graph_color(chip: &str, key: &str, color: Option<&str>) {
    set_graph_field("graph_color", chip, key, color.map(toml_edit::value));
}

/// Sets or clears a graph line's style override.
pub fn set_graph_style(chip: &str, key: &str, style: Option<&str>) {
    set_graph_field("graph_style", chip, key, style.map(toml_edit::value));
}

/// Sets or clears a graph line's visibility override.
pub fn set_graph_shown(chip: &str, key: &str, shown: Option<bool>) {
    set_graph_field("graph_shown", chip, key, shown.map(toml_edit::value));
}

/// Remembers the last curve selected for a fan. This is a resume preference,
/// not the active service assignment.
pub fn save_fan_curve(chip: &str, fan: usize, curve: &str) {
    edit_config(|doc| {
        doc["fan_curve"][chip][format!("fan{}", fan + 1)] = toml_edit::value(curve);
    });
}

/// Writes one `[section.<chip>] key = value`, or removes it when `value`
/// is `None`, pruning the chip table (and section) once empty so cleared
/// overrides leave no residue.
fn set_graph_field(section: &str, chip: &str, key: &str, value: Option<toml_edit::Item>) {
    edit_config(|doc| {
        match value {
            Some(item) => doc[section][chip][key] = item,
            None => {
                if let Some(chip_table) = doc
                    .get_mut(section)
                    .and_then(|table| table.get_mut(chip))
                    .and_then(|chip| chip.as_table_like_mut())
                {
                    chip_table.remove(key);
                }
            }
        }
        // Drop empty chip tables, then the empty section.
        let chip_empty = doc
            .get(section)
            .and_then(|table| table.get(chip))
            .and_then(|chip| chip.as_table_like())
            .is_some_and(|chip| chip.is_empty());
        if chip_empty {
            if let Some(table) = doc
                .get_mut(section)
                .and_then(|table| table.as_table_like_mut())
            {
                table.remove(chip);
            }
            let section_empty = doc
                .get(section)
                .and_then(|table| table.as_table_like())
                .is_some_and(|table| table.is_empty());
            if section_empty {
                doc.as_table_mut().remove(section);
            }
        }
    });
}

/// Sets or clears a curve's line color override (`"#rrggbb"`), keyed by
/// curve id under `[curve_color]`. Display-only, like `set_graph_color`.
pub fn save_curve_color(id: &str, color: Option<&str>) {
    edit_config(|doc| {
        match color {
            Some(color) => doc["curve_color"][id] = toml_edit::value(color),
            None => {
                if let Some(table) = doc
                    .get_mut("curve_color")
                    .and_then(|table| table.as_table_like_mut())
                {
                    table.remove(id);
                }
            }
        }
        let empty = doc
            .get("curve_color")
            .and_then(|table| table.as_table_like())
            .is_some_and(|table| table.is_empty());
        if empty {
            doc.as_table_mut().remove("curve_color");
        }
    });
}

/// Renames a `[[curve]]` entry, matching by id.
pub fn save_curve_name(id: &str, name: &str) {
    edit_config(|doc| {
        let Some(entries) = doc
            .get_mut("curve")
            .and_then(|curve| curve.as_array_of_tables_mut())
        else {
            return;
        };
        for (i, entry) in entries.iter_mut().enumerate() {
            let entry_id = entry
                .get("id")
                .and_then(|id| id.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| fallback_curve_id(i));
            if entry_id == id {
                entry["name"] = toml_edit::value(name);
                return;
            }
        }
    });
}

fn edit_config(edit: impl FnOnce(&mut toml_edit::DocumentMut)) {
    let Some(path) = config_path() else { return };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else {
        return; // don't overwrite a file the user is mid-edit on
    };
    edit(&mut doc);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, doc.to_string());
}

/// Persist the unit toggles, editing the file in place so user names and
/// comments survive.
pub fn save_units(temp: TempUnit, fan: FanUnit) {
    edit_config(|doc| {
        doc["units"]["temperature"] = toml_edit::value(temp.as_str());
        doc["units"]["fan"] = toml_edit::value(fan.as_str());
    });
}

/// Write a starter config listing every detected channel, unless the file
/// already exists.
pub fn write_template(chips: &[ChipInfo], snapshots: &[ChipSnapshot]) {
    let Some(path) = config_path() else { return };
    if path.exists() {
        return;
    }

    let mut text = String::from(
        "# zugluft display names — uncomment a line and change its value to\n\
         # rename a sensor or fan. The app picks up changes automatically.\n\
         \n\
         [units]\n\
         temperature = \"celsius\" # or \"fahrenheit\"\n\
         fan = \"rpm\" # or \"percent\" (of the calibrated max RPM)\n",
    );
    for (ci, info) in chips.iter().enumerate() {
        let Some(snapshot) = snapshots.get(ci) else {
            continue;
        };
        let _ = write!(text, "\n[chips.\"{}\"]\n", info.name);
        for (ti, temp) in snapshot.temps.iter().enumerate() {
            if temp.is_some() {
                let label = info
                    .temp_labels
                    .get(ti)
                    .cloned()
                    .unwrap_or_else(|| format!("Temp {}", ti + 1));
                let _ = writeln!(text, "# temp{} = \"{label}\"", ti + 1);
            }
        }
        for (fi, fan) in snapshot.fans.iter().enumerate() {
            if fan.rpm.is_some() || fan.duty.is_some() {
                let _ = writeln!(text, "# fan{} = \"Fan {}\"", fi + 1, fi + 1);
            }
        }
        for (pi, power) in snapshot.powers.iter().enumerate() {
            if power.is_some() {
                let label = info
                    .power_labels
                    .get(pi)
                    .cloned()
                    .unwrap_or_else(|| format!("Power {}", pi + 1));
                let _ = writeln!(text, "# power{} = \"{label}\"", pi + 1);
            }
        }
    }

    let example_chip = chips.first().map_or("ITE IT8688E", |info| &info.name);
    let _ = write!(
        text,
        "\n# Derived sensors for the graph and fan curves: the Sensors tab's\n\
         # “Add sensor” button edits these; hand-editing works too.\n\
         # [[custom]]\n\
         # id = \"mix\"\n\
         # name = \"CPU/System Mix\"\n\
         # kind = \"average\" # average | min | max (\"average\" honors weights)\n\
         # inputs = [\n\
         #     {{ chip = \"{example_chip}\", temp = 1, weight = 2.0 }},\n\
         #     {{ chip = \"{example_chip}\", temp = 2 }},\n\
         # ]\n\
         \n\
         # Fan curves: the Curves tab edits these; hand-editing works too.\n\
         # `source` is a hardware channel or {{ custom = \"mix\" }}. Graph\n\
         # `points` are [°C, target fan %] pairs and clamp at the ends;\n\
         # `linear` uses start/end pairs and clamps outside them; `trigger`\n\
         # uses threshold, before, and after. Functions\n\
         # fine-tune the base curve; standard hysteresis defaults to heat-up\n\
         # now, cool-down after a 2 °C drop for 2 seconds.\n\
         # [[curve]]\n\
         # id = \"cpu\"\n\
         # name = \"CPU Curve\"\n\
         # kind = \"graph\"\n\
         # source = {{ chip = \"{example_chip}\", temp = 1 }}\n\
         # window = {{ temp_min = 25.0, temp_max = 90.0, duty_min = 15.0, duty_max = 100.0 }}\n\
         # functions = [{{ kind = \"standard\", hysteresis = {{ degrees = 2.0, delay_ms = 2000, only_downward = true }} }}]\n\
         # points = [[30.0, 20.0], [50.0, 40.0], [70.0, 100.0]]\n\
         # kind = \"trigger\"\n\
         # threshold = 60.0\n\
         # before = 35.0\n\
         # after = 85.0\n\
         # kind = \"linear\"\n\
         # start = [35.0, 25.0]\n\
         # end = [75.0, 90.0]\n",
    );

    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, text);
}

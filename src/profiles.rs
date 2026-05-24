use crate::gcode_gen::GCodeConfig;

#[derive(Debug, Clone)]
pub struct DeviceProfile {
    pub name: String,
    pub config: GCodeConfig,
}

pub fn builtin_profiles() -> Vec<DeviceProfile> {
    let mut profiles = Vec::new();

    for (stem, raw) in builtin_profiles_raw() {
        match parse_profile(raw) {
            Ok(p) => profiles.push(p),
            Err(e) => log::error!("Failed to parse profile '{}': {}", stem, e),
        }
    }

    profiles
}

include!(concat!(env!("OUT_DIR"), "/profile_list.rs"));

pub fn default_profile_name() -> &'static str {
    "Custom"
}

fn parse_profile(text: &str) -> Result<DeviceProfile, String> {
    let mut config = GCodeConfig::low_level_defaults();
    let mut name = String::from("Unnamed");

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '=');
        let key = parts.next().unwrap().trim();
        let val = parts.next().ok_or_else(|| format!("missing '=' in line: {}", line))?.trim();

        match key {
            "name" => name = val.to_string(),
            "pen_up_z" => config.pen_up_z = val.parse().map_err(|e| format!("pen_up_z: {}", e))?,
            "pen_down_z" => config.pen_down_z = val.parse().map_err(|e| format!("pen_down_z: {}", e))?,
            "pen_thickness" => config.pen_thickness = val.parse().map_err(|e| format!("pen_thickness: {}", e))?,
            "feedrate_xy" => config.feedrate_xy = val.parse().map_err(|e| format!("feedrate_xy: {}", e))?,
            "min_feedrate_xy" => config.min_feedrate_xy = val.parse().map_err(|e| format!("min_feedrate_xy: {}", e))?,
            "feedrate_z" => config.feedrate_z = val.parse().map_err(|e| format!("feedrate_z: {}", e))?,
            "bed_width" => config.bed_width = val.parse().map_err(|e| format!("bed_width: {}", e))?,
            "bed_height" => config.bed_height = val.parse().map_err(|e| format!("bed_height: {}", e))?,
            "y_axis_invert" => config.y_axis_invert = val.parse().map_err(|e| format!("y_axis_invert: {}", e))?,
            "auto_home" => config.auto_home = val.parse().map_err(|e| format!("auto_home: {}", e))?,
            "travel_feedrate" => config.travel_feedrate = val.parse().map_err(|e| format!("travel_feedrate: {}", e))?,
            _ => log::warn!("Unknown profile key '{}'", key),
        }
    }

    Ok(DeviceProfile { name, config })
}

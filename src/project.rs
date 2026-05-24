use crate::app::AppState;
use crate::geometry::Point;
use crate::gcode_gen::GCodeConfig;

/// Serializable project data for .pcp files.
#[derive(serde::Serialize, serde::Deserialize)]
struct ProjectData {
    version: u32,
    sources: Vec<crate::app::SourceElement>,
    gcode_config: GCodeConfig,
    position_offset: Point,
}

/// Save the current session to a .pcp file.
pub fn save_project(state: &AppState, path: &str) -> anyhow::Result<()> {
    let data = ProjectData {
        version: 1,
        sources: state.source_elements.clone(),
        gcode_config: state.gcode_config.clone(),
        position_offset: state.position_offset,
    };
    let json = serde_json::to_string_pretty(&data)?;
    std::fs::write(path, json)?;
    log::info!("Project saved to {}", path);
    Ok(())
}

/// Load a session from a .pcp file, replacing the current state.
pub fn load_project(state: &mut AppState, path: &str) -> anyhow::Result<()> {
    let json = std::fs::read_to_string(path)?;
    let data: ProjectData = serde_json::from_str(&json)?;
    if data.version != 1 {
        anyhow::bail!("Unsupported project version: {}", data.version);
    }
    state.clear();
    state.source_elements = data.sources;
    state.gcode_config = data.gcode_config;
    state.position_offset = data.position_offset;
    state.rebuild_all_paths();
    state.status = format!("Loaded project: {}", path);
    log::info!("Project loaded from {}", path);
    Ok(())
}

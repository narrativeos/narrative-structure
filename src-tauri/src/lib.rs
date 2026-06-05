pub mod project_manager;
pub mod db_engine;

use project_manager::ProjectState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(ProjectState::new())
        .invoke_handler(tauri::generate_handler![
            // project_manager
            project_manager::create_project,
            project_manager::open_project,
            project_manager::close_project,
            project_manager::get_project_path,
            // db_engine
            db_engine::get_toc,
            db_engine::get_blocks,
            db_engine::update_block,
            db_engine::search_blocks,
            db_engine::get_child_count,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NarrativeStructure");
}

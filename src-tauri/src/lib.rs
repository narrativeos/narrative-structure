pub mod project_manager;
pub mod db_engine;
pub mod markdown_parser;

use project_manager::ProjectState;
use tauri::http::{Request, Response, StatusCode};

/// 自定义协议：narrativestructure://localhost/<file_path> 直接提供文件
fn asset_protocol(
    _ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    req: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    use std::fs;
    use std::io::Read;

    let path_str = req.uri().path().trim_start_matches('/');
    let path = std::path::PathBuf::from(
        percent_encoding::percent_decode_str(path_str).decode_utf8_lossy().as_ref()
    );

    match fs::File::open(&path) {
        Ok(mut file) => {
            let mut buf = Vec::new();
            if file.read_to_end(&mut buf).is_ok() {
                let mime = mime_guess::from_path(&path).first_or_octet_stream();
                return Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", mime.as_ref())
                    .header("Access-Control-Allow-Origin", "*")
                    .body(buf)
                    .unwrap();
            }
        }
        _ => {}
    }
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(b"File not found".to_vec())
        .unwrap()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .register_uri_scheme_protocol("narrativestructure", asset_protocol)
        .manage(ProjectState::new())
        .invoke_handler(tauri::generate_handler![
            // project_manager
            project_manager::import_new_project,
            project_manager::open_project,
            project_manager::close_project,
            project_manager::get_project_path,
            project_manager::import_document,
            project_manager::list_project_files,
            project_manager::find_asset_file,
            // db_engine
            db_engine::get_toc,
            db_engine::get_blocks,
            db_engine::get_block,
            db_engine::update_block,
            db_engine::search_blocks,
            db_engine::get_child_count,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NarrativeStructure");
}

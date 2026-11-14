mod bridge;
mod agent;
mod command_proposal;
mod context;
mod deep_video;
mod read_tools;
mod semantic;
#[cfg(test)]
mod retrieval_eval;
#[cfg(test)]
mod retrieval_eval_extended;
mod db;
mod process_status;
mod secret;
mod storage;
mod storyboard;
mod notes;
mod note_design;
mod note_compose;
mod note_images;
mod workflow;
pub use process_status::ProcessStatus;
use tauri::Manager;

#[tauri::command]
fn open_extension_folder(app: tauri::AppHandle) -> Result<(), String> {
    let bundled = app.path().resource_dir().map_err(|error| error.to_string())?.join("extension");
    let folder = if bundled.join("manifest.json").is_file() {
        bundled
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("extension")
    };
    if !folder.join("manifest.json").is_file() {
        return Err("找不到浏览器扩展文件夹，请重新安装 GearNote。".into());
    }
    std::process::Command::new("explorer").arg(folder).spawn()
        .map_err(|error| format!("无法打开扩展文件夹：{error}"))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(storage::AppState::load(app.handle())?);
            app.manage(agent::AgentState::load(app.handle())?);
            app.manage(notes::NotesState::load(app.handle())?);
            bridge::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![open_extension_folder, storage::list_records, storage::list_capture_jobs, storage::dismiss_capture_job, storage::query_record_page, storage::get_record, storage::query_record_segments, storage::get_bridge_status,
            storyboard::analyze_video_storyboard, storyboard::get_storyboard_image, storyboard::open_video_at,
            agent::get_agent_status, agent::save_agent_config, agent::save_explore_references, agent::save_context_tier, agent::save_semantic_retrieval, agent::test_semantic_retrieval, agent::test_agent_tools, agent::list_chat_messages, agent::get_chat_state,
            agent::create_chat_session, agent::switch_chat_session, agent::rename_chat_session,
            agent::delete_chat_session, agent::set_chat_references, agent::resolve_library_search, agent::resolve_command_proposal, agent::save_working_context, agent::confirm_project_memory, agent::revoke_project_memory, agent::compact_chat_context, agent::preview_agent_context, agent::ask_agent,
            agent::get_persona, agent::save_persona, agent::get_default_persona, agent::reload_persona,
            agent::get_chat_frame_image, agent::suggest_chat_followups,
            notes::list_notes, notes::save_note, notes::delete_note, notes::export_note,
            note_design::preview_note_design, note_design::save_note_design, note_design::get_note_design, note_design::delete_note_design, note_design::export_note_design,
            note_compose::compose_note_from_sources, note_compose::propose_note_restructure, note_compose::revise_note_restructure,
            note_images::list_note_frames, note_images::get_note_frame_image, note_images::capture_note_frame,
            notes::list_note_folders, notes::create_note_folder, notes::rename_note_folder, notes::delete_note_folder,
            workflow::propose_note_edit, workflow::apply_note_edit, workflow::undo_note_edit,
            workflow::reject_note_edit, workflow::list_note_workflow_events, workflow::get_pending_note_edit,
            workflow::suggest_working_context, workflow::apply_working_suggestion, workflow::undo_working_suggestion,
            workflow::reject_working_suggestion, workflow::get_pending_working_suggestion, workflow::list_working_workflow_events])
        .run(tauri::generate_context!())
        .expect("failed to run GearNote");
}


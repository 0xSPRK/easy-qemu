use easy_qemu_core::manager::QmpAction;
use easy_qemu_core::snapshots::SnapInfo;
use easy_qemu_core::store::DeleteReport;
use easy_qemu_core::{config, ConsoleInfo, Manager, RunningInfo, VmDraft, VmListItem, VmUpdate};
use tauri::{AppHandle, Emitter, Manager as _, State};
use tauri_plugin_dialog::DialogExt;

use crate::STATUSES_EVENT;

type CmdResult<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[tauri::command]
pub async fn list_vms(m: State<'_, Manager>) -> CmdResult<Vec<VmListItem>> {
    m.list().await.map_err(err)
}

#[tauri::command]
pub async fn get_statuses(app: AppHandle, m: State<'_, Manager>) -> CmdResult<Vec<VmUpdate>> {
    let updates = m.refresh().await.map_err(err)?;
    let _ = app.emit(STATUSES_EVENT, updates.clone());
    Ok(updates)
}

#[tauri::command]
pub async fn create_vm(
    app: AppHandle,
    m: State<'_, Manager>,
    draft: VmDraft,
) -> CmdResult<easy_qemu_core::vm::Vm> {
    let vm = m.create(draft).await.map_err(err)?;
    crate::emit_statuses(&app);
    Ok(vm)
}

#[tauri::command]
pub async fn update_vm(
    app: AppHandle,
    m: State<'_, Manager>,
    vm: easy_qemu_core::vm::Vm,
) -> CmdResult<easy_qemu_core::vm::Vm> {
    let vm = m.update(vm).await.map_err(err)?;
    crate::emit_statuses(&app);
    Ok(vm)
}

#[tauri::command]
pub async fn delete_vm(
    app: AppHandle,
    m: State<'_, Manager>,
    id: String,
    delete_disk: bool,
) -> CmdResult<DeleteReport> {
    let report = m.delete(&id, delete_disk).await.map_err(err)?;
    crate::emit_statuses(&app);
    Ok(report)
}

#[tauri::command]
pub async fn start_vm(app: AppHandle, m: State<'_, Manager>, id: String) -> CmdResult<RunningInfo> {
    let info = m.start(&id).await.map_err(err)?;
    crate::emit_statuses(&app);
    Ok(info)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Pause,
    Resume,
    Reset,
    Shutdown,
}

#[tauri::command]
pub async fn vm_action(
    app: AppHandle,
    m: State<'_, Manager>,
    id: String,
    action: Action,
) -> CmdResult<()> {
    let qa = match action {
        Action::Pause => QmpAction::Pause,
        Action::Resume => QmpAction::Resume,
        Action::Reset => QmpAction::Reset,
        Action::Shutdown => QmpAction::Shutdown,
    };
    let res = m.qmp_action(&id, qa).await;
    crate::emit_statuses(&app);
    res.map_err(err)
}

#[tauri::command]
pub async fn force_stop(app: AppHandle, m: State<'_, Manager>, id: String) -> CmdResult<()> {
    let res = m.force_stop(&id).await;
    crate::emit_statuses(&app);
    res.map_err(err)
}

#[tauri::command]
pub async fn open_console(
    app: AppHandle,
    m: State<'_, Manager>,
    id: String,
) -> CmdResult<ConsoleInfo> {
    let info = m.open_console(&id).await.map_err(err)?;

    // Bring up the console window (or focus the existing one).
    let label = format!("console_{id}");
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.set_focus();
        return Ok(info);
    }
    let name = m
        .list()
        .await
        .ok()
        .and_then(|items| items.into_iter().find(|i| i.vm.id == id))
        .map(|i| i.vm.name)
        .unwrap_or_else(|| id.clone());
    let title = format!("{name} — Console");
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(1024.0, 640.0)
        .min_inner_size(640.0, 460.0)
        .build()
        .map_err(|e| format!("failed to create console window: {e}"))?;

    Ok(info)
}

#[tauri::command]
pub async fn close_console(app: AppHandle, id: String) -> CmdResult<()> {
    if let Some(w) = app.get_webview_window(format!("console_{id}").as_str()) {
        let _ = w.close();
    }
    Ok(())
}

#[tauri::command]
pub fn read_log(m: State<'_, Manager>, id: String, lines: Option<u32>) -> String {
    m.log_tail(&id, lines.unwrap_or(400) as usize)
}

// ---------- snapshots ----------

#[tauri::command]
pub async fn snapshot_list(m: State<'_, Manager>, id: String) -> CmdResult<Vec<SnapInfo>> {
    m.snapshot_list(&id).await.map_err(err)
}

#[tauri::command]
pub async fn snapshot_create(m: State<'_, Manager>, id: String, name: String) -> CmdResult<()> {
    m.snapshot_create(&id, &name).await.map_err(err)
}

#[tauri::command]
pub async fn snapshot_apply(m: State<'_, Manager>, id: String, name: String) -> CmdResult<()> {
    m.snapshot_apply(&id, &name).await.map_err(err)
}

#[tauri::command]
pub async fn snapshot_delete(m: State<'_, Manager>, id: String, name: String) -> CmdResult<()> {
    m.snapshot_delete(&id, &name).await.map_err(err)
}

// ---------- settings ----------

#[tauri::command]
pub async fn get_config(m: State<'_, Manager>) -> CmdResult<config::Config> {
    Ok(m.get_config().await)
}

#[tauri::command]
pub async fn set_config(m: State<'_, Manager>, cfg: config::Config) -> CmdResult<()> {
    m.set_config(cfg).await.map_err(err)
}

#[tauri::command]
pub async fn get_config_warnings(m: State<'_, Manager>) -> CmdResult<Vec<String>> {
    Ok(m.get_config_warnings().await)
}

/// Native file/folder picker. `kind`: "iso" | "qcow2" | "folder" | "any".
#[tauri::command]
pub fn pick_path(app: AppHandle, kind: String) -> Option<String> {
    let mut builder = app.dialog().file();
    match kind.as_str() {
        "iso" => {
            builder = builder.add_filter("Disk images", &["iso", "img"]);
        }
        "qcow2" => {
            builder = builder.add_filter("QEMU disks", &["qcow2", "qcow", "img"]);
        }
        _ => {}
    }
    if kind == "folder" {
        builder = builder.set_title("Choose a folder");
        builder.blocking_pick_folder().map(|p| p.to_string())
    } else {
        builder.blocking_pick_file().map(|p| p.to_string())
    }
}

use easy_qemu_core::manager::QmpAction;
use easy_qemu_core::snapshots::SnapInfo;
use easy_qemu_core::store::DeleteReport;
use easy_qemu_core::{
    config, ConsoleInfo, Manager, RunningInfo, SshConfig, VmDraft, VmListItem, VmUpdate,
};
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
    Suspend,
    Wakeup,
    Nmi,
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
        Action::Suspend => QmpAction::Suspend,
        Action::Wakeup => QmpAction::Wakeup,
        Action::Nmi => QmpAction::Nmi,
    };
    let res = m.qmp_action(&id, qa).await;
    crate::emit_statuses(&app);
    res.map_err(err)
}

/// Send a key combo (e.g. "ctrl-alt-delete") to the guest.
#[tauri::command]
pub async fn vm_sendkey(m: State<'_, Manager>, id: String, keys: String) -> CmdResult<()> {
    m.send_key(&id, &keys).await.map_err(err)
}

/// Save a guest screenshot; returns the written file path.
#[tauri::command]
pub async fn vm_screendump(m: State<'_, Manager>, id: String) -> CmdResult<String> {
    m.screendump(&id).await.map_err(err)
}

/// Adjust guest memory at runtime via the virtio-balloon (MB).
#[tauri::command]
pub async fn vm_set_balloon(m: State<'_, Manager>, id: String, mb: u64) -> CmdResult<()> {
    m.set_balloon(&id, mb).await.map_err(err)
}

/// Insert/eject the CD image while the VM runs. `None` ejects.
#[tauri::command]
pub async fn vm_change_iso(
    m: State<'_, Manager>,
    id: String,
    iso: Option<String>,
) -> CmdResult<()> {
    m.change_iso(&id, iso).await.map_err(err)
}

/// Set the VNC password (requires vnc_password_auth on the VM record).
#[tauri::command]
pub async fn vm_set_vnc_password(
    m: State<'_, Manager>,
    id: String,
    password: String,
) -> CmdResult<()> {
    m.set_vnc_password(&id, &password).await.map_err(err)
}

// ---------- saved state (migrate to file) ----------

#[tauri::command]
pub async fn has_saved_state(m: State<'_, Manager>, id: String) -> CmdResult<bool> {
    Ok(m.has_saved_state(&id).await)
}

#[tauri::command]
pub async fn discard_saved_state(m: State<'_, Manager>, id: String) -> CmdResult<()> {
    m.discard_saved_state(&id).await.map_err(err)
}

/// Save the live VM state and stop it. Starting again resumes from the state.
#[tauri::command]
pub async fn save_vm_state(app: AppHandle, m: State<'_, Manager>, id: String) -> CmdResult<()> {
    let res = m.save_state(&id).await;
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

/// Open an SSH session to the VM in an external terminal window.
#[tauri::command]
pub fn ssh_connect(m: State<'_, Manager>, id: String) -> CmdResult<SshConfig> {
    m.ssh_connect(&id).map_err(err)
}

// ---------- snapshots ----------

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapOp {
    List,
    Create,
    Apply,
    Delete,
    /// Live variants operate on a RUNNING VM via QMP (all disks at once).
    LiveList,
    LiveCreate,
    LiveApply,
    LiveDelete,
}

/// Single entry point for snapshot operations.
/// `list`/`live_list` return the snapshots; mutations return an empty vec.
#[tauri::command]
pub async fn snapshot_op(
    m: State<'_, Manager>,
    id: String,
    op: SnapOp,
    name: Option<String>,
    disk_id: Option<String>,
) -> CmdResult<Vec<SnapInfo>> {
    let empty = <Vec<SnapInfo>>::new();
    let name = name.as_deref().unwrap_or_default();
    match op {
        SnapOp::List => m.snapshot_list(&id, disk_id).await.map_err(err),
        SnapOp::Create => m
            .snapshot_create(&id, name, disk_id)
            .await
            .map(|_| empty)
            .map_err(err),
        SnapOp::Apply => m
            .snapshot_apply(&id, name, disk_id)
            .await
            .map(|_| empty)
            .map_err(err),
        SnapOp::Delete => m
            .snapshot_delete(&id, name, disk_id)
            .await
            .map(|_| empty)
            .map_err(err),
        SnapOp::LiveList => m.snapshot_live_list(&id).await.map_err(err),
        SnapOp::LiveCreate => m
            .snapshot_live_create(&id, name)
            .await
            .map(|_| empty)
            .map_err(err),
        SnapOp::LiveApply => m
            .snapshot_live_apply(&id, name)
            .await
            .map(|_| empty)
            .map_err(err),
        SnapOp::LiveDelete => m
            .snapshot_live_delete(&id, name)
            .await
            .map(|_| empty)
            .map_err(err),
    }
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

/// Detect QEMU binaries and supported accelerators on this host.
#[tauri::command]
pub async fn probe_env(m: State<'_, Manager>) -> CmdResult<easy_qemu_core::env::EnvProbe> {
    let cfg = m.get_config().await;
    Ok(easy_qemu_core::env::probe(&cfg))
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

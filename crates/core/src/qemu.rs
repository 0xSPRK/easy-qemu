use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};

use crate::accel;
use crate::config::Config;
use crate::qmp::Qmp;
use crate::store::{Store, RunningInfo};
use crate::vm::{Accel, CpuModel, DisplayMode, Firmware, Vm};

pub fn resolve_binary(name: &str, override_path: &Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        if p.exists() {
            return Ok(p.clone());
        }
        return Err(anyhow!(
            "Бинарный файл '{name}' не найден по пути: {}",
            p.display()
        ));
    }

    if let Some(p) = search_in_path(name) {
        return Ok(p);
    }

    #[cfg(windows)]
    {
        let prog = PathBuf::from("C:\\Program Files\\qemu").join(name);
        if prog.is_file() {
            return Ok(prog);
        }
    }

    Err(anyhow!(
        "Не удалось найти '{name}'. Добавьте его в PATH или укажите путь в настройках."
    ))
}

fn search_in_path(name: &str) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(&exe_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Свободный TCP-порт для QMP. Возможную гонку закрывает retry-цикл подключения.
pub fn free_tcp_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

pub fn qmp_addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
}

pub fn pidfile_path_for(vm: &Vm) -> PathBuf {
    vm.pidfile_path()
}

const VNC_PORT_SEARCH_RANGE: u16 = 20;

pub fn build_qemu_args(
    vm: &Vm,
    qmp_port: u16,
    vnc_bind: &str,
    accel_resolved: Accel,
    daemonize: bool,
) -> Result<Vec<String>> {
    let mut a: Vec<String> = vec![
        "-name".into(),
        vm.name.clone(),
        "-m".into(),
        vm.memory_mb.to_string(),
        "-smp".into(),
        vm.cpus.to_string(),
        "-drive".into(),
        format!("file={},if=virtio,format=qcow2", vm.disk_path.display()),
    ];

    let net = match vm.net_mode {
        crate::vm::NetMode::Nat => {
            let mut nic = "user".to_string();
            if let Some(m) = vm.net_model.qemu_model() {
                nic.push_str(&format!(",model={m}"));
            }
            for hf in &vm.hostfwd {
                nic.push(',');
                nic.push_str(&hf.qemu_fragment());
            }
            nic
        }
        crate::vm::NetMode::Bridged => {
            let mut nic = "tap".to_string();
            if let Some(m) = vm.net_model.qemu_model() {
                nic.push_str(&format!(",model={m}"));
            }
            nic
        }
        crate::vm::NetMode::None => "none".to_string(),
    };
    a.extend(["-nic".into(), net]);

    if let Some(flag) = vm.machine.qemu_flag() {
        a.extend(["-machine".into(), flag.to_string()]);
    }

    match vm.cpu {
        CpuModel::Auto => {}
        CpuModel::Max => a.extend(["-cpu".into(), "max".into()]),
        // `-cpu host` валиден только под KVM; для WHPX/TCG используем max.
        CpuModel::Host => {
            let model = if accel_resolved == Accel::Kvm { "host" } else { "max" };
            a.extend(["-cpu".into(), model.into()]);
        }
    }

    match accel_resolved {
        Accel::Kvm => a.push("-enable-kvm".into()),
        Accel::Whpx => a.extend(["-accel".into(), "whpx".into()]),
        Accel::Tcg => a.extend(["-accel".into(), "tcg".into()]),
        Accel::Auto | Accel::None => {}
    }

    match vm.display {
        DisplayMode::None => a.extend(["-display".into(), "none".into()]),
        DisplayMode::Vnc => {
            // Динамический подбор свободного порта: стартуем от подсказки,
            // QEMU сам найдёт первый свободный в диапазоне; реальный порт
            // уточняем через QMP query-vnc.
            let start = vm.vnc_display.unwrap_or(0);
            a.extend([
                "-vnc".into(),
                format!("{vb}:{d},to={rng}", vb = vnc_bind, d = start, rng = VNC_PORT_SEARCH_RANGE),
            ]);
        }
        DisplayMode::Gtk => a.extend(["-display".into(), "gtk".into()]),
        DisplayMode::Sdl => a.extend(["-display".into(), "sdl".into()]),
    }

    if vm.firmware == Firmware::Uefi {
        setup_uefi(vm, &mut a)?;
    }

    if let Some(iso) = &vm.iso {
        a.extend([
            "-cdrom".into(),
            iso.display().to_string(),
            "-boot".into(),
            "d".into(),
        ]);
    }

    a.extend([
        "-chardev".into(),
        format!(
            "socket,host=127.0.0.1,port={},server=on,wait=off,id=qmp0",
            qmp_port
        ),
        "-mon".into(),
        "chardev=qmp0,mode=control".into(),
        "-pidfile".into(),
        vm.pidfile_path().to_string_lossy().to_string(),
    ]);

    if daemonize {
        #[cfg(target_os = "linux")]
        a.push("-daemonize".into());
    }

    Ok(a)
}

/// Находит OVMF (UEFI) образы: CODE (только чтение) и VARS (шаблон для копии на ВМ).
fn find_ovmf() -> Option<(PathBuf, PathBuf)> {
    let candidates: &[(&str, &str)] = &[
        ("OVMF_CODE.fd", "OVMF_VARS.fd"),
        ("ovmf_CODE.fd", "ovmf_VARS.fd"),
        ("edk2-x86_64-code.fd", "edk2-i386-vars.fd"),
        ("edk2-x86_64-secure-code.fd", "edk2-i386-vars.fd"),
    ];
    let dirs: &[&str] = &[
        "C:\\Program Files\\qemu\\share",
        "C:\\Program Files\\qemu\\share\\edk2-x86_64",
        "/usr/share/OVMF",
        "/usr/share/ovmf",
        "/usr/share/edk2/ovmf",
        "/usr/share/edk2",
    ];
    for d in dirs {
        for (code, vars) in candidates {
            let code_p = PathBuf::from(d).join(code);
            let vars_p = PathBuf::from(d).join(vars);
            if code_p.is_file() && vars_p.is_file() {
                return Some((code_p, vars_p));
            }
        }
    }
    None
}

fn setup_uefi(vm: &Vm, a: &mut Vec<String>) -> Result<()> {
    let (code, vars_tmpl) = find_ovmf()
        .ok_or_else(|| anyhow!("UEFI запрошен, но OVMF не найден (OVMF_CODE.fd / OVMF_VARS.fd)."))?;

    let vars_path = vm
        .disk_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{}-vars.fd", vm.id));

    if !vars_path.exists() {
        if let Err(e) = std::fs::copy(&vars_tmpl, &vars_path) {
            return Err(anyhow!("Не удалось скопировать OVMF VARS: {e}"));
        }
    }

    a.extend([
        "-drive".into(),
        format!("if=pflash,format=raw,readonly=on,file={}", code.display()),
        "-drive".into(),
        format!("if=pflash,format=raw,file={}", vars_path.display()),
    ]);
    Ok(())
}

fn open_log(path: &Path) -> std::fs::File {
    if let Ok(f) = std::fs::File::create(path) {
        return f;
    }
    #[cfg(windows)]
    {
        std::fs::OpenOptions::new().write(true).open("NUL").unwrap_or_else(|_| {
            std::fs::File::create(path.parent().unwrap_or(Path::new(".")).join("fallback.log"))
                .unwrap()
        })
    }
    #[cfg(not(windows))]
    {
        std::fs::OpenOptions::new().write(true).open("/dev/null").unwrap()
    }
}

/// Последние `max_lines` строк лога (для показа ошибки старта).
pub fn log_tail(path: &Path, max_lines: usize) -> String {
    let Ok(s) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let lines: Vec<&str> = s.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

#[cfg(windows)]
const DETACHED_FLAGS: u32 = 0x0000_0008 | 0x0000_0200; // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP

/// Запуск QEMU + ожидание готовности (pidfile и QMP).
/// При ранней смерти процесса ошибка содержит хвост лога.
pub async fn start_vm(
    store: &Store,
    vm: &Vm,
    cfg: &Config,
    support: &accel::AccelSupport,
) -> Result<RunningInfo> {
    let bin = resolve_binary("qemu-system-x86_64", &cfg.qemu_binary)?;
    let resolved = accel::effective(vm.accel, support);
    let qmp_port = free_tcp_port()?;
    let args = build_qemu_args(vm, qmp_port, &cfg.vnc_bind(), resolved, true)?;
    let log_path = store.base.join(format!("{}.log", vm.id));

    let mut cmd = std::process::Command::new(&bin);
    cmd.args(&args)
        .stdout(open_log(&log_path))
        .stderr(std::fs::OpenOptions::new().append(true).open(&log_path).unwrap_or_else(|_| open_log(&log_path)));

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(DETACHED_FLAGS);
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("не удалось запустить {}", bin.display()))?;

    let pid = wait_for_pidfile(&mut child, &vm.pidfile_path(), &log_path).await?;

    // Ждём готовности QMP и узнаём реальный VNC-порт.
    let mut vnc_port = None;
    for _ in 0..10 {
        match Qmp::connect(qmp_addr(qmp_port)).await {
            Ok(mut q) => {
                if vm.display == DisplayMode::Vnc {
                    vnc_port = q.query_vnc_port().await.ok().flatten();
                }
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }

    Ok(RunningInfo {
        id: vm.id.clone(),
        pid,
        qmp_port,
        vnc_display: vm.vnc_display,
        vnc_port,
        display: vm.display,
        started_at: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        ),
    })
}

async fn wait_for_pidfile(
    child: &mut std::process::Child,
    pidfile: &Path,
    log_path: &Path,
) -> Result<i32> {
    for _ in 0..40 {
        if let Ok(Some(status)) = child.try_wait() {
            let tail = log_tail(log_path, 30);
            let tail_str = if tail.trim().is_empty() {
                String::new()
            } else {
                format!("\n\nХвост лога:\n{tail}")
            };
            anyhow::bail!("QEMU завершился сразу после запуска ({status}).{tail_str}");
        }
        if pidfile.exists() {
            return read_pidfile(pidfile);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let _ = child.kill();
    anyhow::bail!(
        "QEMU не создал pidfile за 10 секунд.\nХвост лога:\n{}",
        log_tail(log_path, 30)
    )
}

fn read_pidfile(path: &Path) -> Result<i32> {
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("не удалось прочитать pidfile {}", path.display()))?;
    s.lines()
        .next()
        .and_then(|l| l.trim().parse::<i32>().ok())
        .ok_or_else(|| anyhow!("pidfile не содержит корректный PID"))
}

/// Создание qcow2-диска через qemu-img (асинхронно, без окна консоли на Windows).
pub async fn create_disk(img_bin: &Path, path: &Path, size_gb: u32) -> Result<()> {
    let mut cmd = tokio::process::Command::new(img_bin);
    cmd.args(["create", "-f", "qcow2"])
        .arg(path)
        .arg(format!("{size_gb}G"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    hide_window(&mut cmd);

    let out = cmd.output().await?;
    if !out.status.success() {
        return Err(anyhow!(
            "qemu-img завершился с ошибкой: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(windows)]
pub fn hide_window(cmd: &mut tokio::process::Command) {
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

#[cfg(not(windows))]
pub fn hide_window(_cmd: &mut tokio::process::Command) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::{
        Accel, CpuModel, DisplayMode, Firmware, HostFwd, MachineType, FwdProto, NetModel, NetMode, Vm,
    };

    fn sample_vm(display: DisplayMode) -> Vm {
        Vm {
            id: "test".into(),
            name: "demo".into(),
            memory_mb: 2048,
            cpus: 2,
            disk_size_gb: 20,
            disk_path: PathBuf::from("/tmp/demo.qcow2"),
            disk_owned: true,
            iso: Some(PathBuf::from("/tmp/boot.iso")),
            accel: Accel::Auto,
            display,
            vnc_display: Some(3),
            machine: MachineType::Auto,
            firmware: Firmware::Bios,
            cpu: CpuModel::Auto,
            net_mode: NetMode::Nat,
            net_model: NetModel::Auto,
            hostfwd: vec![],
        }
    }

    #[test]
    fn args_contain_core_options() {
        let vm = sample_vm(DisplayMode::Vnc);
        let args =
            build_qemu_args(&vm, 5555, "127.0.0.1", Accel::Tcg, true).unwrap();
        let joined = args.join(" ");
        assert!(joined.contains("-name demo"));
        assert!(joined.contains("-m 2048"));
        assert!(joined.contains("-smp 2"));
        assert!(joined.contains("file=/tmp/demo.qcow2,if=virtio,format=qcow2"));
        assert!(joined.contains("-nic user"));
        assert!(joined.contains("-cdrom /tmp/boot.iso"));
        assert!(joined.contains("-boot d"));
        assert!(joined.contains("socket,host=127.0.0.1,port=5555,server=on,wait=off,id=qmp0"));
        assert!(joined.contains("-mon chardev=qmp0,mode=control"));
        // Динамический VNC: подсказка 3 + диапазон поиска.
        assert!(joined.contains("-vnc 127.0.0.1:3,to=20"));
    }

    #[test]
    fn headless_has_no_vnc() {
        let vm = sample_vm(DisplayMode::None);
        let args = build_qemu_args(&vm, 5555, "127.0.0.1", Accel::Tcg, true).unwrap();
        let joined = args.join(" ");
        assert!(joined.contains("-display none"));
        assert!(!joined.contains("-vnc"));
    }

    #[test]
    fn machine_and_cpu_flags() {
        let mut vm = sample_vm(DisplayMode::None);
        vm.machine = MachineType::Q35;
        vm.cpu = CpuModel::Max;
        let joined = build_qemu_args(&vm, 5555, "127.0.0.1", Accel::Tcg, true)
            .unwrap()
            .join(" ");
        assert!(joined.contains("-machine q35"));
        assert!(joined.contains("-cpu max"));
    }

    #[test]
    fn host_cpu_falls_back_on_non_kvm() {
        let mut vm = sample_vm(DisplayMode::None);
        vm.cpu = CpuModel::Host;
        let joined = build_qemu_args(&vm, 5555, "127.0.0.1", Accel::Whpx, true)
            .unwrap()
            .join(" ");
        assert!(joined.contains("-cpu max"));
        assert!(!joined.contains("-cpu host"));
        assert!(joined.contains("-accel whpx"));
    }

    #[test]
    fn kvm_enables_host_cpu_and_flag() {
        let mut vm = sample_vm(DisplayMode::None);
        vm.cpu = CpuModel::Host;
        let joined = build_qemu_args(&vm, 5555, "127.0.0.1", Accel::Kvm, true)
            .unwrap()
            .join(" ");
        assert!(joined.contains("-enable-kvm"));
        assert!(joined.contains("-cpu host"));
    }

    #[test]
    fn default_machine_omits_flag() {
        let vm = sample_vm(DisplayMode::None);
        let joined = build_qemu_args(&vm, 5555, "127.0.0.1", Accel::Tcg, true)
            .unwrap()
            .join(" ");
        assert!(!joined.contains("-machine "));
        assert!(!joined.contains("-cpu"));
    }

    #[test]
    fn nat_hostfwd_rules_in_args() {
        let mut vm = sample_vm(DisplayMode::None);
        vm.hostfwd = vec![
            HostFwd { proto: FwdProto::Tcp, host_port: 2222, guest_port: 22 },
            HostFwd { proto: FwdProto::Udp, host_port: 5353, guest_port: 53 },
        ];
        let joined = build_qemu_args(&vm, 5555, "127.0.0.1", Accel::Tcg, true)
            .unwrap()
            .join(" ");
        assert!(joined.contains("-nic user,hostfwd=tcp::2222-:22,hostfwd=udp::5353-:53"));
    }

    #[test]
    fn nat_with_explicit_model_keeps_hostfwd() {
        let mut vm = sample_vm(DisplayMode::None);
        vm.net_model = NetModel::Virtio;
        vm.hostfwd = vec![HostFwd { proto: FwdProto::Tcp, host_port: 8080, guest_port: 80 }];
        let joined = build_qemu_args(&vm, 5555, "127.0.0.1", Accel::Tcg, true)
            .unwrap()
            .join(" ");
        assert!(joined.contains("-nic user,model=virtio-net-pci,hostfwd=tcp::8080-:80"));
    }

    #[test]
    fn bridged_net_uses_tap() {
        let mut vm = sample_vm(DisplayMode::None);
        vm.net_mode = NetMode::Bridged;
        vm.net_model = NetModel::E1000;
        let joined = build_qemu_args(&vm, 5555, "127.0.0.1", Accel::Tcg, true)
            .unwrap()
            .join(" ");
        assert!(joined.contains("-nic tap,model=e1000"));
    }

    #[test]
    fn no_net_uses_none() {
        let mut vm = sample_vm(DisplayMode::None);
        vm.net_mode = NetMode::None;
        let joined = build_qemu_args(&vm, 5555, "127.0.0.1", Accel::Tcg, true)
            .unwrap()
            .join(" ");
        assert!(joined.contains("-nic none"));
        assert!(!joined.contains("hostfwd"));
    }

    #[test]
    fn uefi_requires_ovmf_present_or_errors() {
        let mut vm = sample_vm(DisplayMode::None);
        vm.firmware = Firmware::Uefi;
        match build_qemu_args(&vm, 5555, "127.0.0.1", Accel::Tcg, true) {
            Err(e) => assert!(e.to_string().contains("OVMF")),
            Ok(args) => {
                let joined = args.join(" ");
                assert!(joined.contains("if=pflash,format=raw,readonly=on"));
                assert!(joined.contains("test-vars.fd"));
            }
        }
    }

    #[test]
    fn free_port_is_available() {
        let p = free_tcp_port().unwrap();
        assert!(TcpListener::bind(("127.0.0.1", p)).is_ok());
    }

    #[test]
    fn log_tail_limits_lines() {
        let dir = std::env::temp_dir().join(format!("eq-log-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.log");
        std::fs::write(&p, (1..=50).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n")).unwrap();
        let tail = log_tail(&p, 10);
        assert_eq!(tail.lines().count(), 10);
        assert!(tail.ends_with("line50"));
        assert!(!tail.contains("line39"));
    }
}

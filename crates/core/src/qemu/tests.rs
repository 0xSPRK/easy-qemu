use super::*;
use crate::vm::{
    AioMode, AudioBackend, BootFirst, CacheMode, DiskBus, DiskEntry, DiskOpts, Firmware, FwdProto,
    HostFwd, NetMode, NetModel, RtcBase, SshConfig, VgaModel,
};

fn disk(id: &str, bus: DiskBus) -> DiskEntry {
    DiskEntry {
        id: id.into(),
        path: PathBuf::from(format!("/tmp/{id}.qcow2")),
        size_gb: 20,
        owned: true,
        opts: DiskOpts {
            bus,
            ..Default::default()
        },
    }
}

fn sample_vm(display: DisplayMode) -> Vm {
    let mut vm = Vm {
        id: "test".into(),
        name: "demo".into(),
        memory_mb: 2048,
        cpus: 2,
        disks: vec![disk("d0", DiskBus::VirtioBlk)],
        iso: Some(PathBuf::from("/tmp/boot.iso")),
        accel: Accel::Auto,
        display,
        vnc_display: Some(3),
        machine: MachineType::Auto,
        firmware: Firmware::Bios,
        cpu: CpuModel::Auto,
        smp_sockets: None,
        smp_cores: None,
        smp_threads: None,
        maxcpus: None,
        rng: false,
        usb_tablet: false,
        vga: VgaModel::Auto,
        vgamem_mb: None,
        gl: false,
        mem_prealloc: false,
        mem_path: None,
        rtc: RtcBase::Utc,
        audio: AudioBackend::Off,
        vnc_password_auth: false,
        boot_first: BootFirst::Disk,
        boot_menu: false,
        net_mode: NetMode::Nat,
        net_model: NetModel::Auto,
        mac_address: None,
        tap_ifname: None,
        tap_br: None,
        hostfwd: vec![],
        ssh: SshConfig::default(),
        legacy_disk_path: None,
        legacy_disk_size_gb: None,
        legacy_disk_owned: None,
    };
    vm.normalize();
    vm
}

fn launch() -> QemuLaunch {
    QemuLaunch {
        qmp_port: 5555,
        pidfile: PathBuf::from("/base/test.pid"),
        vars_fd: PathBuf::from("/base/test-vars.fd"),
        vnc_bind: "127.0.0.1".into(),
        hostfwd_bind: String::new(),
        accel_resolved: Accel::Tcg,
        daemonize: true,
        incoming: None,
    }
}

fn build(vm: &Vm) -> String {
    build_qemu_args(vm, &launch()).unwrap().join(" ")
}

#[test]
fn args_contain_core_options() {
    let vm = sample_vm(DisplayMode::Vnc);
    let joined = build(&vm);
    assert!(joined.contains("-name demo"));
    assert!(joined.contains("-m 2048"));
    assert!(joined.contains("-smp 2"));
    assert!(joined.contains("file=/tmp/d0.qcow2,format=qcow2,id=d0,if=none,node-name=d0"));
    assert!(joined.contains("-device virtio-blk-pci,drive=d0"));
    assert!(joined.contains("-nic user,mac=52:54:"));
    assert!(joined.contains("-cdrom /tmp/boot.iso"));
    assert!(joined.contains("socket,host=127.0.0.1,port=5555,server=on,wait=off,id=qmp0"));
    assert!(joined.contains("-mon chardev=qmp0,mode=control"));
    // Dynamic VNC: hint display 3 + search range.
    assert!(joined.contains("-vnc 127.0.0.1:3,to=20"));
    // ISO present + default boot order -> one-shot cdrom boot.
    assert!(joined.contains("-boot once=d"));
}

#[test]
fn headless_has_no_vnc_and_no_video() {
    let mut vm = sample_vm(DisplayMode::None);
    let joined = build(&vm);
    assert!(joined.contains("-display none"));
    assert!(!joined.contains("-vnc"));
    assert!(!joined.contains("-vga"));

    // Explicit video model is emitted even headless (screendump support).
    vm.vga = VgaModel::Std;
    assert!(build(&vm).contains("-vga std"));

    // Explicit off stays silent.
    vm.vga = VgaModel::None;
    assert!(!build(&vm).contains("-vga "));
}

#[test]
fn machine_cpu_topology_flags() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.machine = MachineType::Q35;
    vm.cpu = CpuModel::Max;
    vm.smp_sockets = Some(1);
    vm.smp_cores = Some(2);
    vm.maxcpus = Some(4);
    let joined = build(&vm);
    assert!(joined.contains("-machine q35"));
    assert!(joined.contains("-cpu max"));
    assert!(joined.contains("-smp 2,sockets=1,cores=2,maxcpus=4"));
}

#[test]
fn host_cpu_falls_back_on_non_kvm() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.cpu = CpuModel::Host;
    let mut l = launch();
    l.accel_resolved = Accel::Whpx;
    let joined = build_qemu_args(&vm, &l).unwrap().join(" ");
    assert!(joined.contains("-cpu max"));
    assert!(!joined.contains("-cpu host"));
    assert!(joined.contains("-accel whpx"));
}

#[test]
fn kvm_enables_host_cpu_and_accel() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.cpu = CpuModel::Host;
    let mut l = launch();
    l.accel_resolved = Accel::Kvm;
    let joined = build_qemu_args(&vm, &l).unwrap().join(" ");
    assert!(joined.contains("-accel kvm"));
    assert!(joined.contains("-cpu host"));
}

#[test]
fn tcg_multithread_when_multiple_cpus() {
    // sample vm has 2 cpus -> MTTCG on.
    assert!(build(&sample_vm(DisplayMode::None)).contains("-accel tcg,thread=multi"));

    let mut single = sample_vm(DisplayMode::None);
    single.cpus = 1;
    let joined = build(&single);
    assert!(joined.contains("-accel tcg"));
    assert!(!joined.contains("thread=multi"));
}

#[test]
fn default_machine_omits_flag_and_cpu() {
    let joined = build(&sample_vm(DisplayMode::None));
    assert!(!joined.contains("-machine "));
    assert!(!joined.contains("-cpu"));
}

#[test]
fn disk_perf_options_flow_through() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.disks[0].opts.cache = CacheMode::None;
    vm.disks[0].opts.aio = AioMode::Native;
    vm.disks[0].opts.discard_unmap = true;
    vm.disks[0].opts.iothread = true;
    let joined = build(&vm);
    assert!(joined.contains(
        "file=/tmp/d0.qcow2,format=qcow2,id=d0,if=none,node-name=d0,cache=none,aio=native,discard=unmap,iothread=io0"
    ));
    assert!(joined.contains("-object iothread,id=io0"));
}

#[test]
fn nvme_disk_uses_device_attach() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.disks[0].opts.bus = DiskBus::Nvme;
    let joined = build(&vm);
    assert!(joined.contains("-drive file=/tmp/d0.qcow2,format=qcow2,id=d0,if=none,node-name=d0"));
    assert!(joined.contains("-device nvme,drive=d0,serial=eq"));
}

#[test]
fn ide_sata_port_wiring() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.machine = MachineType::I440Fx;
    vm.disks[0].opts.bus = DiskBus::Ide;
    vm.disks.push(disk("d1", DiskBus::Ide));
    let joined = build(&vm);
    assert!(joined.contains("-device ide-hd,drive=d0,bus=ide.0,unit=0"));
    assert!(joined.contains("-device ide-hd,drive=d1,bus=ide.0,unit=1"));

    let mut vm = sample_vm(DisplayMode::None);
    vm.machine = MachineType::Q35;
    vm.disks[0].opts.bus = DiskBus::Sata;
    assert!(build(&vm).contains("-device ide-hd,drive=d0,bus=ide.0"));

    // IDE overflow fails instead of producing a broken command line.
    let mut vm = sample_vm(DisplayMode::None);
    for i in 0..5 {
        vm.disks.push(disk(&format!("dx{i}"), DiskBus::Ide));
    }
    assert!(build_qemu_args(&vm, &launch()).is_err());
}

#[test]
fn microvm_uses_mmio_devices() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.machine = MachineType::MicroVm;
    vm.rng = true;
    vm.usb_tablet = true;
    let joined = build(&vm);
    assert!(joined.contains("-machine microvm"));
    assert!(joined.contains("-device virtio-blk-device,drive=d0"));
    assert!(joined.contains("-device virtio-rng-device,rng=rng0"));
    assert!(!joined.contains("usb-tablet"));
}

#[test]
fn nat_hostfwd_rules_in_args_with_bind() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.hostfwd = vec![
        HostFwd {
            proto: FwdProto::Tcp,
            host_port: 2222,
            guest_port: 22,
        },
        HostFwd {
            proto: FwdProto::Udp,
            host_port: 5353,
            guest_port: 53,
        },
    ];
    let joined = build(&vm);
    assert!(joined.contains("-nic user,"));
    assert!(joined.contains(",mac=52:54:"));
    assert!(joined.contains(",hostfwd=tcp::2222-:22,hostfwd=udp::5353-:53"));

    let mut l = launch();
    l.hostfwd_bind = "127.0.0.1".into();
    let joined = build_qemu_args(&vm, &l).unwrap().join(" ");
    assert!(joined.contains("hostfwd=tcp:127.0.0.1:2222-:22"));
}

#[test]
fn bridged_net_uses_tap_params() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.net_mode = NetMode::Bridged;
    vm.net_model = NetModel::E1000;
    vm.tap_ifname = Some("MyTap ".into());
    vm.tap_br = Some("br0".into());
    let joined = build(&vm);
    assert!(joined.contains("-nic tap,model=e1000,"));
    assert!(joined.contains(",ifname=MyTap,br=br0,script=no,downscript=no"));
    assert!(joined.contains(",mac=52:54:"));
}

#[test]
fn explicit_mac_wins_over_derived() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.mac_address = Some("52:54:00:aa:bb:cc".into());
    assert!(build(&vm).contains(",mac=52:54:00:aa:bb:cc"));
}

#[test]
fn no_net_uses_none() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.net_mode = NetMode::None;
    let joined = build(&vm);
    assert!(joined.contains("-nic none"));
    assert!(!joined.contains("hostfwd"));
    assert!(!joined.contains("mac="));
}

#[test]
fn vnc_password_flag() {
    let vm = sample_vm(DisplayMode::Vnc);
    assert!(!build(&vm).contains("password=on"));
    let mut vm = sample_vm(DisplayMode::Vnc);
    vm.vnc_password_auth = true;
    assert!(build(&vm).contains("-vnc 127.0.0.1:3,to=20,password=on"));
}

#[test]
fn video_models_and_vgamem() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.vga = VgaModel::Virtio;
    assert!(build(&vm).contains("-vga virtio"));

    vm.vga = VgaModel::Qxl;
    vm.vgamem_mb = Some(64);
    assert!(build(&vm).contains("-vga qxl,vgamem_mb=64"));

    vm.vga = VgaModel::None;
    assert!(!build(&vm).contains("-vga "));
}

#[test]
fn virgl_gl_couples_display_and_video() {
    let mut vm = sample_vm(DisplayMode::Gtk);
    vm.gl = true;
    let joined = build(&vm);
    assert!(joined.contains("-display gtk,gl=on"));
    assert!(joined.contains("-device virtio-vga-gl"));
    assert!(!joined.contains("-vga "));

    // SDL variant.
    let mut vm = sample_vm(DisplayMode::Sdl);
    vm.gl = true;
    let joined = build(&vm);
    assert!(joined.contains("-display sdl,gl=on"));

    // VNC + gl is rejected at validation, builder just follows the flags.
    let mut vm = sample_vm(DisplayMode::Vnc);
    vm.gl = true;
    assert!(build(&vm).contains("-device virtio-vga-gl"));
}

#[test]
fn ramfb_device_for_uefi_framebuffer() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.vga = VgaModel::Ramfb;
    assert!(build(&vm).contains("-device ramfb"));
}

#[test]
fn usb_tablet_and_rng() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.usb_tablet = true;
    vm.rng = true;
    let joined = build(&vm);
    assert!(joined.contains("-usb -device usb-tablet"));
    assert!(joined.contains("-object rng-builtin,id=rng0"));
    assert!(joined.contains("-device virtio-rng-pci,rng=rng0"));

    let mut off = sample_vm(DisplayMode::None);
    assert!(!build(&off).contains("usb-tablet"));
    off.rng = false;
    assert!(!build(&off).contains("rng-builtin"));
}

#[test]
fn audio_devices() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.audio = AudioBackend::Auto;
    let joined = build(&vm);
    assert!(joined.contains("-audiodev "), "auto picks a host driver");
    assert!(joined.contains("-device hda-duplex,audiodev=snd0"));

    vm.machine = MachineType::Q35;
    assert!(build(&vm).contains("-device ich9-intel-hda"));

    // Off (default) emits nothing — legacy behavior preserved.
    let quiet = sample_vm(DisplayMode::None);
    assert!(!build(&quiet).contains("-audiodev"));
}

#[test]
fn rtc_base() {
    let mut vm = sample_vm(DisplayMode::None);
    assert!(build(&vm).contains("-rtc base=utc"));
    vm.rtc = RtcBase::LocalTime;
    assert!(build(&vm).contains("-rtc base=localtime"));
}

#[test]
fn boot_order_and_menu() {
    let mut vm = sample_vm(DisplayMode::None); // has ISO
    assert!(build(&vm).contains("-boot once=d"));

    vm.iso = None;
    assert!(!build(&vm).contains("-boot")); // fully default

    vm.boot_menu = true;
    assert!(build(&vm).contains("-boot menu=on"));

    let mut cd = sample_vm(DisplayMode::None);
    cd.boot_first = BootFirst::Cd;
    assert!(build(&cd).contains("-boot order=d"));

    let mut pxe = sample_vm(DisplayMode::None);
    pxe.iso = None;
    pxe.boot_first = BootFirst::Network;
    pxe.boot_menu = true;
    assert!(build(&pxe).contains("-boot order=n,menu=on"));
}

#[test]
fn memory_backend_and_prealloc() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.mem_prealloc = true;
    assert!(build(&vm).contains("-mem-prealloc"));

    let mut vm = sample_vm(DisplayMode::None);
    vm.machine = MachineType::Q35;
    vm.mem_path = Some("/dev/hugepages".into());
    vm.mem_prealloc = true;
    let joined = build(&vm);
    assert!(joined.contains(
        "-object memory-backend-file,id=mem,size=2048M,mem-path=/dev/hugepages,share=on,prealloc=on"
    ));
    assert!(joined.contains("-machine q35,memory-backend=mem"));
    assert!(!joined.contains("-mem-prealloc"));
}

#[test]
fn incoming_state_restores_via_file_uri() {
    let mut l = launch();
    l.incoming = Some(PathBuf::from("/base/test.sav"));
    let args = build_qemu_args(&sample_vm(DisplayMode::None), &l).unwrap();
    let joined = args.join(" ");
    assert!(joined.contains("-incoming file:/base/test.sav"));
}

/// A comma inside an option value must not start a new option (audit F-08).
#[test]
fn commas_in_option_values_are_escaped() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.disks[0].path = PathBuf::from("/tmp/we,ird.qcow2");
    vm.disks[0].id = "d,0".into();
    vm.hostfwd = vec![HostFwd {
        proto: FwdProto::Tcp,
        host_port: 2222,
        guest_port: 22,
    }];
    vm.mem_path = Some("/dev/huge,pages".into());

    let mut l = launch();
    l.hostfwd_bind = "127.0.0.1,1".into();
    let joined = build_qemu_args(&vm, &l).unwrap().join(" ");

    assert!(joined.contains("file=/tmp/we,,ird.qcow2,format=qcow2"));
    assert!(joined.contains("id=d,,0"));
    assert!(joined.contains("drive=d,,0"));
    assert!(joined.contains("hostfwd=tcp:127.0.0.1,,1:2222-:22"));
    assert!(joined.contains("mem-path=/dev/huge,,pages"));
}

/// A TAP interface name cannot carry a comma, so it is rejected instead of
/// being escaped (audit F-08).
#[test]
fn tap_name_with_a_comma_is_rejected() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.net_mode = NetMode::Bridged;
    vm.tap_ifname = Some("tap,0".into());
    assert!(build_qemu_args(&vm, &launch()).is_err());
}

/// A bridge name is a free-form value, so it is escaped.
#[test]
fn bridge_name_with_a_comma_is_escaped() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.net_mode = NetMode::Bridged;
    vm.tap_ifname = Some("tap0".into());
    vm.tap_br = Some("br,0".into());
    let joined = build(&vm);
    assert!(joined.contains("br=br,,0"));
}

#[test]
fn too_many_disks_rejected() {
    let mut vm = sample_vm(DisplayMode::None);
    for i in 1..9 {
        vm.disks.push(disk(&format!("dx{i}"), DiskBus::VirtioBlk));
    }
    assert!(build_qemu_args(&vm, &launch()).is_err());
}

#[test]
fn uefi_requires_ovmf_present_or_errors() {
    let mut vm = sample_vm(DisplayMode::None);
    vm.firmware = Firmware::Uefi;
    match build_qemu_args(&vm, &launch()) {
        Err(e) => assert!(e.to_string().contains("OVMF")),
        Ok(args) => {
            let joined = args.join(" ");
            assert!(joined.contains("if=pflash,format=raw,readonly=on"));
            assert!(joined.contains("/base/test-vars.fd"));
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
    std::fs::write(
        &p,
        (1..=50)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let tail = log_tail(&p, 10);
    assert_eq!(tail.lines().count(), 10);
    assert!(tail.ends_with("line50"));
    assert!(!tail.contains("line39"));
}

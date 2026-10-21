//! Single validation point for user-supplied values.
//!
//! Everything that reaches a path, a file name, a QEMU option value or an
//! external command line goes through this module first. The rules are
//! deliberately strict: a value that is not accepted here must not be
//! concatenated into a path or an option string anywhere else.

use std::path::Path;

use anyhow::{bail, Result};

/// Maximum length of a VM name (the name is also used as a folder name).
pub const VM_NAME_MAX_LEN: usize = 64;
/// Maximum length of a VM id.
pub const VM_ID_MAX_LEN: usize = 64;
/// Maximum length of a disk id (used as a QEMU drive id).
pub const DISK_ID_MAX_LEN: usize = 32;
/// Smallest disk size the manager accepts.
pub const DISK_SIZE_MIN_GB: u32 = 1;
/// Largest disk size the manager accepts.
pub const DISK_SIZE_MAX_GB: u32 = 4096;
/// Maximum length of a snapshot name.
pub const SNAPSHOT_NAME_MAX_LEN: usize = 60;
/// Maximum length of a TAP interface name (Linux `IFNAMSIZ` is 16).
pub const TAP_NAME_MAX_LEN: usize = 15;

/// A VM id is joined with the storage directory for `<id>.json`, `<id>.pid`,
/// `<id>.log` and `<id>.sav`, so it must be a plain file name.
pub fn vm_id(id: &str) -> Result<&str> {
    if id.is_empty() {
        bail!("VM id cannot be empty");
    }
    if id.len() > VM_ID_MAX_LEN {
        bail!("VM id must be at most {VM_ID_MAX_LEN} characters");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("VM id may contain only letters, digits, '-' and '_'");
    }
    Ok(id)
}

/// A VM name is shown in the UI and used as a folder name for new disks.
/// Path separators, wildcards and control characters are rejected; letters of
/// any script are allowed.
pub fn vm_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Name cannot be empty");
    }
    if name.chars().count() > VM_NAME_MAX_LEN {
        bail!("Name must be at most {VM_NAME_MAX_LEN} characters");
    }
    if name.starts_with('-') {
        bail!("Name must not start with '-'");
    }
    if name.chars().any(|c| {
        c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
    }) {
        bail!("Name contains characters that are not allowed in a file name");
    }
    if name.ends_with('.') || name.ends_with(' ') {
        bail!("Name must not end with '.' or a space");
    }
    if name == "." || name == ".." {
        bail!("Name is reserved");
    }
    Ok(name)
}

/// A disk id is used as a QEMU drive id inside comma-separated options.
pub fn disk_id(id: &str) -> Result<&str> {
    if id.is_empty() {
        bail!("Disk id cannot be empty");
    }
    if id.len() > DISK_ID_MAX_LEN {
        bail!("Disk id must be at most {DISK_ID_MAX_LEN} characters");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("Disk id may contain only letters, digits, '-' and '_'");
    }
    Ok(id)
}

/// The size of a newly created disk image.
pub fn disk_size_gb(size: u32) -> Result<u32> {
    if !(DISK_SIZE_MIN_GB..=DISK_SIZE_MAX_GB).contains(&size) {
        bail!("Disk size must be between {DISK_SIZE_MIN_GB} and {DISK_SIZE_MAX_GB} GB");
    }
    Ok(size)
}

/// The format of a disk image. Only the QEMU formats are supported, because
/// the manager attaches every disk with `format=qcow2`.
pub fn disk_extension(path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "qcow2" && ext != "qcow" {
        bail!("Only disks in the qcow2/qcow format are supported");
    }
    Ok(())
}

/// A TCP port. Zero is never a usable port.
pub fn port(port: u16) -> Result<u16> {
    if port == 0 {
        bail!("Port must be between 1 and 65535");
    }
    Ok(port)
}

/// A TAP interface name.
pub fn tap_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        bail!("TAP interface name cannot be empty");
    }
    if name.len() > TAP_NAME_MAX_LEN {
        bail!("TAP interface name must be at most {TAP_NAME_MAX_LEN} characters");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        bail!("TAP interface name may contain only letters, digits and '_'");
    }
    Ok(name)
}

/// A snapshot name is passed to `qemu-img` and to the QMP job id.
pub fn snapshot_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Snapshot name cannot be empty");
    }
    if name.chars().count() > SNAPSHOT_NAME_MAX_LEN {
        bail!("Snapshot name must be at most {SNAPSHOT_NAME_MAX_LEN} characters");
    }
    if name.starts_with('-') {
        bail!("Snapshot name must not start with '-'");
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || "-_ .".contains(c))
    {
        bail!("Snapshot name may contain only letters, digits, spaces, '-', '_' and '.'");
    }
    Ok(name)
}

/// The host part of an SSH target. It reaches an external command line, so
/// only characters that are meaningful in a host name are accepted.
pub fn ssh_host(host: &str) -> Result<&str> {
    let host = host.trim();
    if host.is_empty() {
        bail!("SSH host cannot be empty");
    }
    if host.starts_with('-') {
        bail!("SSH host must not start with '-'");
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@' | ':' | '%'))
    {
        bail!("SSH host contains unsupported characters");
    }
    Ok(host)
}

/// The user part of an SSH target.
pub fn ssh_user(user: &str) -> Result<&str> {
    let user = user.trim();
    if user.is_empty() {
        bail!("SSH user cannot be empty");
    }
    if user.starts_with('-') {
        bail!("SSH user must not start with '-'");
    }
    if !user
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@'))
    {
        bail!("SSH user contains unsupported characters");
    }
    Ok(user)
}

/// Escape a value that is embedded in a comma-separated QEMU option.
/// QEMU reads a doubled comma as a literal comma, so escaping keeps the
/// value inside its own option instead of starting a new one.
pub fn escape_commas(value: &str) -> String {
    value.replace(',', ",,")
}

/// The same, for a path: a disk path is embedded in `-drive file=...`.
pub fn qemu_path(path: &Path) -> String {
    escape_commas(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_id_accepts_generated_ids() {
        let id = format!("vm{}", uuid::Uuid::new_v4().simple());
        assert_eq!(vm_id(&id).unwrap(), id);
    }

    #[test]
    fn vm_id_rejects_traversal() {
        for bad in [
            "",
            "..",
            "../secret",
            "..\\secret",
            "a/b",
            "a\\b",
            "C:evil",
            "vm 1",
            "vm.1",
            "vm\x00",
        ] {
            assert!(vm_id(bad).is_err(), "must reject {bad:?}");
        }
    }

    #[test]
    fn vm_id_rejects_overlong_values() {
        assert!(vm_id(&"a".repeat(VM_ID_MAX_LEN + 1)).is_err());
        assert!(vm_id(&"a".repeat(VM_ID_MAX_LEN)).is_ok());
    }

    #[test]
    fn vm_name_accepts_ordinary_names() {
        assert_eq!(vm_name("  Test VM  ").unwrap(), "Test VM");
        assert_eq!(vm_name("ВМ-1").unwrap(), "ВМ-1");
        assert_eq!(vm_name("web-server_01").unwrap(), "web-server_01");
    }

    #[test]
    fn vm_name_rejects_path_unsafe_names() {
        for bad in [
            "",
            "   ",
            "..",
            ".",
            "../victim",
            "..\\victim",
            "C:\\Windows\\Temp",
            "a/b",
            "a\\b",
            "with:colon",
            "star*",
            "question?",
            "quote\"",
            "less<greater>",
            "pipe|",
            "nul\x00",
            "bell\x07",
            "-dash",
            "trailing.",
        ] {
            assert!(vm_name(bad).is_err(), "must reject {bad:?}");
        }
    }

    #[test]
    fn vm_name_trims_surrounding_whitespace() {
        assert_eq!(vm_name("  padded  ").unwrap(), "padded");
    }

    #[test]
    fn vm_name_rejects_overlong_values() {
        let long = "a".repeat(VM_NAME_MAX_LEN + 1);
        assert!(vm_name(&long).is_err());
        let ok = "a".repeat(VM_NAME_MAX_LEN);
        assert!(vm_name(&ok).is_ok());
    }

    #[test]
    fn disk_id_rejects_option_injection() {
        assert!(disk_id("d0").is_ok());
        for bad in [
            "",
            "d0,evil=1",
            "d0 id",
            "d0/x",
            "a".repeat(DISK_ID_MAX_LEN + 1).as_str(),
        ] {
            assert!(disk_id(bad).is_err(), "must reject {bad:?}");
        }
    }

    #[test]
    fn disk_size_is_bounded() {
        assert_eq!(disk_size_gb(10).unwrap(), 10);
        assert_eq!(disk_size_gb(DISK_SIZE_MAX_GB).unwrap(), DISK_SIZE_MAX_GB);
        assert!(disk_size_gb(0).is_err());
        assert!(disk_size_gb(DISK_SIZE_MAX_GB + 1).is_err());
    }

    #[test]
    fn only_qcow_formats_are_accepted() {
        assert!(disk_extension(Path::new("/tmp/a.qcow2")).is_ok());
        assert!(disk_extension(Path::new("/tmp/a.QCOW2")).is_ok());
        assert!(disk_extension(Path::new("/tmp/a.qcow")).is_ok());
        for bad in ["/tmp/a.img", "/tmp/a", "/tmp/a.iso", "/tmp/a.qcow2.txt"] {
            assert!(disk_extension(Path::new(bad)).is_err(), "must reject {bad}");
        }
    }

    #[test]
    fn port_zero_is_rejected() {
        assert_eq!(port(22).unwrap(), 22);
        assert!(port(0).is_err());
    }

    #[test]
    fn tap_name_is_bounded() {
        assert_eq!(tap_name(" tap0 ").unwrap(), "tap0");
        assert!(tap_name("").is_err());
        assert!(tap_name("a".repeat(TAP_NAME_MAX_LEN + 1).as_str()).is_err());
        assert!(tap_name("tap0,evil").is_err());
    }

    #[test]
    fn snapshot_name_rejects_option_like_values() {
        assert_eq!(snapshot_name(" before ").unwrap(), "before");
        assert!(snapshot_name("").is_err());
        assert!(snapshot_name("-apply").is_err());
        assert!(snapshot_name("a/b").is_err());
        assert!(snapshot_name("a\nb").is_err());
    }

    #[test]
    fn ssh_fields_reject_shell_metacharacters() {
        assert_eq!(ssh_host(" 10.0.0.5 ").unwrap(), "10.0.0.5");
        assert_eq!(ssh_user("root").unwrap(), "root");
        for bad in ["", "h&st", "h|st", "h st", "-h", "h;rm -rf /"] {
            assert!(ssh_host(bad).is_err(), "must reject host {bad:?}");
        }
        for bad in ["", "u&er", "u er", "-u"] {
            assert!(ssh_user(bad).is_err(), "must reject user {bad:?}");
        }
    }

    #[test]
    fn commas_are_escaped_for_qemu_options() {
        assert_eq!(escape_commas("a,b"), "a,,b");
        assert_eq!(
            qemu_path(Path::new("C:/disks/my,disk.qcow2")),
            "C:/disks/my,,disk.qcow2"
        );
        assert_eq!(escape_commas("plain"), "plain");
    }
}

# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.3.x | yes |
| 0.2.x | yes (bug and security fixes only) |
| < 0.2 | no |

## Reporting a vulnerability

Please do not open a public issue. Use GitHub's private reporting
("Security" -> "Report a vulnerability") on the repository, or contact the
maintainer directly. Include:

* a description of the issue and its impact,
* the affected version and platform,
* reproduction steps or a proof of concept.

We aim to acknowledge a report within a few days and to keep you updated
while it is being fixed. Please allow a reasonable time for a fix to be
released before disclosing publicly.

## Threat model in one paragraph

Easy QEMU runs a local desktop application that starts QEMU processes on the
user's behalf. The assets worth protecting are the guest operating systems
reachable through the console and the host filesystem where VM records and
disk images live. The relevant attack surfaces are: identifiers and file
paths that reach the filesystem, the QEMU command line, the noVNC console
proxy, the QMP monitor, and the Tauri IPC boundary.

## Measures already in place

* The core never spawns a shell. QEMU and `qemu-img` are started with an
  argument vector, not a command string.
* The console proxy binds `127.0.0.1` on an ephemeral port and is torn down
  when the VM stops or the console window closes.
* VNC binds `127.0.0.1` by default.
* Tauri capabilities are limited to `core:default` and `dialog:default`; no
  `fs`, `shell` or `http` permission is granted to the webview.
* Disks created by the manager are marked as owned; imported disks are never
  deleted by the manager.
* The QEMU process is started detached, so the guest keeps running when the
  application is closed, and the application reconnects to it through QMP.

## Known open issues

`docs/AUDIT.md` lists the open findings of the Phase 0 audit (`F-01`..
`F-44`) together with the planned fix for each. The path-handling,
console-proxy, VNC/host-forwarding and SSH findings are scheduled for the
0.3 cycle. Until those land, treat the console proxy and the VNC port as
accessible to other local processes, and do not expose forwarded ports to an
untrusted network.

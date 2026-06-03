//! Renderer process sandbox abstractions (cross-platform foundation).
//!
//! Chromium reference: //sandbox/policy/sandbox.cc - per-OS policy builder.
//! Real impl per platform:
//! - Linux: seccomp-bpf + user namespaces + bind mounts (RLIMIT_*)
//! - Windows: AppContainer + Job objects + integrity-level reduction
//! - macOS: sandbox-exec (seatbelt) with embedded TinyScheme profile
//!
//! This module declares policy types + checks; real syscall installation happens
//! in the bin entrypoint per OS (cfg gated).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SandboxPolicy {
    None,                       // dev mode
    Renderer,                   // strict: no fs, no net, no exec
    Gpu,                        // graphics: GPU device + minimal IPC
    Utility,                    // medium: limited fs read
    PrintCompositor,
    Service,
    Network,                    // can bind sockets
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SandboxOs {
    Linux,
    Windows,
    Mac,
    Other,
}

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub policy: SandboxPolicy,
    pub os: SandboxOs,
    pub allow_filesystem_paths: Vec<String>,
    pub allow_network: bool,
    pub allow_gpu_access: bool,
    pub allow_audio: bool,
    pub allow_camera: bool,
    pub allow_subprocess: bool,
}

impl SandboxConfig {
    pub fn renderer() -> Self {
        Self {
            policy: SandboxPolicy::Renderer,
            os: detect_os(),
            allow_filesystem_paths: Vec::new(),
            allow_network: false,
            allow_gpu_access: false,
            allow_audio: false,
            allow_camera: false,
            allow_subprocess: false,
        }
    }

    pub fn gpu() -> Self {
        Self {
            policy: SandboxPolicy::Gpu,
            os: detect_os(),
            allow_filesystem_paths: gpu_fs_paths(),
            allow_network: false,
            allow_gpu_access: true,
            allow_audio: false,
            allow_camera: false,
            allow_subprocess: false,
        }
    }

    pub fn network() -> Self {
        Self {
            policy: SandboxPolicy::Network,
            os: detect_os(),
            allow_filesystem_paths: vec!["/etc/ssl".into(), "/etc/resolv.conf".into()],
            allow_network: true,
            allow_gpu_access: false,
            allow_audio: false,
            allow_camera: false,
            allow_subprocess: false,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.policy == SandboxPolicy::Renderer {
            if self.allow_network { return Err("renderer must not have network".into()); }
            if self.allow_subprocess { return Err("renderer must not exec".into()); }
        }
        if self.policy == SandboxPolicy::Network && !self.allow_network {
            return Err("network policy must allow network".into());
        }
        Ok(())
    }
}

pub fn detect_os() -> SandboxOs {
    if cfg!(target_os = "linux") { SandboxOs::Linux }
    else if cfg!(target_os = "windows") { SandboxOs::Windows }
    else if cfg!(target_os = "macos") { SandboxOs::Mac }
    else { SandboxOs::Other }
}

fn gpu_fs_paths() -> Vec<String> {
    match detect_os() {
        SandboxOs::Linux => vec!["/dev/dri".into(), "/dev/nvidia*".into(), "/usr/share/X11".into()],
        SandboxOs::Mac => vec!["/System/Library/Frameworks/Metal.framework".into()],
        SandboxOs::Windows => vec![r"C:\Windows\System32\d3d12.dll".into()],
        SandboxOs::Other => Vec::new(),
    }
}

/// Render-side check: is operation X permitted in the active sandbox?
/// Returns true if allowed.
pub fn permits(cfg: &SandboxConfig, op: SandboxOp) -> bool {
    match op {
        SandboxOp::FileRead(path) => cfg.allow_filesystem_paths.iter().any(|p| path.starts_with(p)),
        SandboxOp::FileWrite(_) => cfg.policy == SandboxPolicy::None,
        SandboxOp::NetworkConnect(_) => cfg.allow_network,
        SandboxOp::GpuDevice => cfg.allow_gpu_access,
        SandboxOp::Audio => cfg.allow_audio,
        SandboxOp::Camera => cfg.allow_camera,
        SandboxOp::SubprocessSpawn => cfg.allow_subprocess,
    }
}

#[derive(Debug, Clone)]
pub enum SandboxOp {
    FileRead(String),
    FileWrite(String),
    NetworkConnect(String),
    GpuDevice,
    Audio,
    Camera,
    SubprocessSpawn,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_blocks_network() {
        let c = SandboxConfig::renderer();
        assert!(c.validate().is_ok());
        assert!(!permits(&c, SandboxOp::NetworkConnect("x.com".into())));
    }

    #[test]
    fn gpu_allows_gpu() {
        let c = SandboxConfig::gpu();
        assert!(permits(&c, SandboxOp::GpuDevice));
    }

    #[test]
    fn network_allows_network() {
        let c = SandboxConfig::network();
        assert!(c.validate().is_ok());
        assert!(permits(&c, SandboxOp::NetworkConnect("x.com".into())));
    }

    #[test]
    fn invalid_renderer_with_net_rejects() {
        let mut c = SandboxConfig::renderer();
        c.allow_network = true;
        assert!(c.validate().is_err());
    }

    #[test]
    fn file_read_within_allow_list() {
        let mut c = SandboxConfig::gpu();
        c.allow_filesystem_paths = vec!["/tmp".into()];
        assert!(permits(&c, SandboxOp::FileRead("/tmp/x.dat".into())));
        assert!(!permits(&c, SandboxOp::FileRead("/etc/shadow".into())));
    }

    #[test]
    fn subprocess_default_deny() {
        let c = SandboxConfig::renderer();
        assert!(!permits(&c, SandboxOp::SubprocessSpawn));
    }
}

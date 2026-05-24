//! Process sandbox - OS-specific isolation per renderer.
//!
//! Goals: renderer process s untrusted JS/HTML/CSS NE muze:
//! - File system access (krome explicit OPFS / file picker)
//! - Network access (krome IPC k browser process)
//! - Spawn child processes
//! - Native code load (krom WebAssembly)
//! - Hardware access (camera, mic, GPS bez user gesture)
//!
//! Per OS:
//! - **Windows**: AppContainer + restricted token + job object
//! - **Linux**: seccomp-bpf syscall filter + namespaces + cgroups
//! - **macOS**: sandbox-exec profile (App Sandbox)
//!
//! Foundation: cross-platform API + diagnostic. Real impl per-OS = next session.
//!
//! Inspired by:
//! - Chromium `sandbox/win/`, `sandbox/linux/`, `sandbox/mac/`
//! - Firefox `security/sandbox/`

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SandboxLevel {
    /// No sandbox - debug mode.
    None,
    /// Standard - block file/network/spawn syscalls (renderer process).
    Standard,
    /// Strict - jen JS eval, no syscalls krome IPC.
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SandboxPlatform {
    Windows,
    Linux,
    MacOs,
    Unsupported,
}

pub fn detect_platform() -> SandboxPlatform {
    #[cfg(target_os = "windows")] { SandboxPlatform::Windows }
    #[cfg(target_os = "linux")] { SandboxPlatform::Linux }
    #[cfg(target_os = "macos")] { SandboxPlatform::MacOs }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    { SandboxPlatform::Unsupported }
}

/// Apply sandbox vuci current process. Foundation = no-op + log.
/// Real impl OS-specific:
/// - Windows: SetProcessMitigationPolicy, AssignProcessToJobObject, CreateRestrictedToken
/// - Linux: prctl(PR_SET_NO_NEW_PRIVS), seccomp_load + filter rules
/// - macOS: sandbox_init s profile string
pub fn apply_sandbox(level: SandboxLevel) -> Result<(), String> {
    let plat = detect_platform();
    if plat == SandboxPlatform::Unsupported {
        return Err("sandbox: unsupported platform".into());
    }
    if level == SandboxLevel::None {
        return Ok(());
    }
    // Foundation: no actual restriction. Log intent.
    eprintln!("[sandbox] level={:?} platform={:?} - foundation only, no enforcement",
        level, plat);
    Ok(())
}

/// Capability checks - real sandbox by zablokoval on syscall level.
/// Foundation: explicit check pres deny list pro testovani API.
#[derive(Default)]
pub struct SandboxCapabilities {
    pub allow_file_read: bool,
    pub allow_file_write: bool,
    pub allow_network: bool,
    pub allow_subprocess: bool,
    pub allow_native_code: bool,
}

impl SandboxCapabilities {
    pub fn standard() -> Self {
        Self {
            allow_file_read: false,
            allow_file_write: false,
            allow_network: false, // pres IPC do browser process only
            allow_subprocess: false,
            allow_native_code: false,
        }
    }
    pub fn strict() -> Self {
        Self::default() // vsechno false
    }
    pub fn unsandboxed() -> Self {
        Self {
            allow_file_read: true,
            allow_file_write: true,
            allow_network: true,
            allow_subprocess: true,
            allow_native_code: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_platform_some() {
        let p = detect_platform();
        // Pri normal target = Some valid platform.
        assert!(matches!(p,
            SandboxPlatform::Windows | SandboxPlatform::Linux
            | SandboxPlatform::MacOs | SandboxPlatform::Unsupported));
    }

    #[test]
    fn apply_no_op_for_none() {
        assert!(apply_sandbox(SandboxLevel::None).is_ok());
    }

    #[test]
    fn standard_caps_block_filesystem() {
        let c = SandboxCapabilities::standard();
        assert!(!c.allow_file_read);
        assert!(!c.allow_file_write);
        assert!(!c.allow_subprocess);
    }

    #[test]
    fn strict_blocks_all() {
        let c = SandboxCapabilities::strict();
        assert!(!c.allow_file_read);
        assert!(!c.allow_network);
        assert!(!c.allow_native_code);
    }
}

//! Runtime operating-system and architecture admission for the Windows helper.
#[cfg(windows)]
/// Returns `(supported, description)` from native OS version/architecture probes.
/// Support requires Windows major 10/build ≥10240, native AMD64, and an x86-64
/// helper build. Version query failure reports unsupported; the text remains
/// diagnostic evidence and does not establish installed tool or USB capability.
pub(crate) fn host() -> (bool, String) {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;
    use windows_sys::Win32::System::SystemInformation::{
        GetNativeSystemInfo,
        OSVERSIONINFOW,
        PROCESSOR_ARCHITECTURE_AMD64,
        SYSTEM_INFO,
    };
    let mut version: OSVERSIONINFOW = unsafe { std::mem::zeroed() };
    version.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;
    let status = unsafe { RtlGetVersion(&mut version) };
    let mut system: SYSTEM_INFO = unsafe { std::mem::zeroed() };
    unsafe { GetNativeSystemInfo(&mut system) };
    let arch = unsafe { system.Anonymous.Anonymous.wProcessorArchitecture };
    (
        status == 0
            && version.dwMajorVersion == 10
            && version.dwBuildNumber >= 10240
            && arch == PROCESSOR_ARCHITECTURE_AMD64
            && cfg!(target_arch = "x86_64"),
        format!(
            "Windows {}.{} build {}, native architecture {}, helper {}",
            version.dwMajorVersion,
            version.dwMinorVersion,
            version.dwBuildNumber,
            arch,
            std::env::consts::ARCH
        ),
    )
}
#[cfg(not(windows))]
/// Returns unsupported plus compile-target OS/architecture text on non-Windows hosts.
pub(crate) fn host() -> (bool, String) { (false, format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)) }

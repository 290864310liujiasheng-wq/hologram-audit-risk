// v4 Phase 5 — Credential storage
// Uses DPAPI on Windows via direct FFI (avoids heavy windows crate dependencies).
#![allow(non_snake_case)] // Win32 FFI naming conventions

use std::ffi::c_void;
use std::collections::BTreeMap;
use std::path::PathBuf;

type CryptProtectDataFn = unsafe extern "system" fn(
    *const DATA_BLOB, *const u16, *const DATA_BLOB, *const c_void,
    *const c_void, u32, *mut DATA_BLOB,
) -> i32;

type CryptUnprotectDataFn = unsafe extern "system" fn(
    *const DATA_BLOB, *mut u16, *const DATA_BLOB, *const c_void,
    *const c_void, u32, *mut DATA_BLOB,
) -> i32;

type LocalFreeFn = unsafe extern "system" fn(isize) -> isize;

#[repr(C)]
struct DATA_BLOB {
    cbData: u32,
    pbData: *mut u8,
}

const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;
const CRYPTPROTECT_LOCAL_MACHINE: u32 = 0x4;

/// Load DPAPI functions from crypt32.dll at runtime.
fn dpapi_encrypt(data: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        // SAFETY: crypt32.dll and kernel32.dll are always present on Windows
        let crypt32 = unsafe { libloading::Library::new("crypt32.dll") }
            .map_err(|e| format!("cannot load crypt32: {}", e))?;
        let kernel32 = unsafe { libloading::Library::new("kernel32.dll") }
            .map_err(|e| format!("kernel32: {}", e))?;

        // Hold references until the end of scope
        let CryptProtectData: libloading::Symbol<CryptProtectDataFn> = unsafe { crypt32.get(b"CryptProtectData") }
            .map_err(|e| format!("CryptProtectData: {}", e))?;
        let LocalFree: libloading::Symbol<LocalFreeFn> = unsafe { kernel32.get(b"LocalFree") }
            .map_err(|e| format!("LocalFree: {}", e))?;

        let mut blob_in = DATA_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut blob_out = DATA_BLOB { cbData: 0, pbData: std::ptr::null_mut() };

        let ret = unsafe {
            CryptProtectData(&mut blob_in, std::ptr::null(), std::ptr::null(),
                std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN | CRYPTPROTECT_LOCAL_MACHINE,
                &mut blob_out)
        };
        if ret == 0 {
            return Err("DPAPI encrypt failed".into());
        }
        let encrypted = unsafe { std::slice::from_raw_parts(blob_out.pbData, blob_out.cbData as usize).to_vec() };
        unsafe { LocalFree(blob_out.pbData as isize); }
        Ok(encrypted)
    }
    #[cfg(not(windows))]
    { Err("unsupported platform".into()) }
}

fn dpapi_decrypt(data: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        let crypt32 = unsafe { libloading::Library::new("crypt32.dll") }
            .map_err(|e| format!("cannot load crypt32: {}", e))?;
        let kernel32 = unsafe { libloading::Library::new("kernel32.dll") }
            .map_err(|e| format!("kernel32: {}", e))?;

        let CryptUnprotectData: libloading::Symbol<CryptUnprotectDataFn> = unsafe { crypt32.get(b"CryptUnprotectData") }
            .map_err(|e| format!("CryptUnprotectData: {}", e))?;
        let LocalFree: libloading::Symbol<LocalFreeFn> = unsafe { kernel32.get(b"LocalFree") }
            .map_err(|e| format!("LocalFree: {}", e))?;

        let mut blob_in = DATA_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut blob_out = DATA_BLOB { cbData: 0, pbData: std::ptr::null_mut() };

        let ret = unsafe {
            CryptUnprotectData(&mut blob_in, std::ptr::null_mut(), std::ptr::null(),
                std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN,
                &mut blob_out)
        };
        if ret == 0 {
            return Err("DPAPI decrypt failed".into());
        }
        let plain = unsafe { std::slice::from_raw_parts(blob_out.pbData, blob_out.cbData as usize).to_vec() };
        unsafe { LocalFree(blob_out.pbData as isize); }
        Ok(plain)
    }
    #[cfg(not(windows))]
    { Err("unsupported platform".into()) }
}

fn cred_path() -> PathBuf {
    app_support_dir().join("credentials.enc")
}

fn credential_manifest_path() -> PathBuf {
    app_support_dir().join("credential-providers.json")
}

fn app_support_dir() -> PathBuf {
    if let Some(override_dir) = std::env::var_os("HOLOGRAM_APP_SUPPORT_DIR") {
        return PathBuf::from(override_dir);
    }

    #[cfg(windows)]
    {
        return std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("com.hologram.app");
    }

    #[cfg(target_os = "macos")]
    {
        return std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("Library")
            .join("Application Support")
            .join("com.hologram.app");
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(".local")
                    .join("share")
            })
            .join("com.hologram.app")
    }
}

fn load_plaintext_credentials() -> Result<BTreeMap<String, String>, String> {
    let encrypted = match std::fs::read(cred_path()) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(format!("read credentials: {}", e)),
    };
    let plain = dpapi_decrypt(&encrypted)?;
    parse_plaintext_credentials(&plain)
}

fn save_plaintext_credentials(entries: &BTreeMap<String, String>) -> Result<(), String> {
    let dir = app_support_dir();
    std::fs::create_dir_all(&dir).ok();

    let serialized = entries
        .iter()
        .map(|(provider, key)| format!("{}={}", provider, key))
        .collect::<Vec<_>>()
        .join("\n");
    let encrypted = dpapi_encrypt(serialized.as_bytes())?;
    std::fs::write(cred_path(), encrypted)
        .map_err(|e| format!("write credentials: {}", e))
}

fn parse_plaintext_credentials(plain: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    let s = String::from_utf8(plain.to_vec()).map_err(|e| format!("invalid cred: {}", e))?;
    for line in s.lines() {
        if let Some((prov, key)) = line.split_once('=') {
            out.insert(prov.to_string(), key.to_string());
        }
    }
    Ok(out)
}

#[cfg(target_os = "macos")]
fn security_cli_path() -> String {
    std::env::var("HOLOGRAM_SECURITY_CLI").unwrap_or_else(|_| "/usr/bin/security".into())
}

#[cfg(target_os = "macos")]
fn keychain_service() -> &'static str {
    "com.hologram.app"
}

#[cfg(target_os = "macos")]
fn run_security(args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new(security_cli_path())
        .args(args)
        .output()
        .map_err(|e| format!("security command failed: {}", e))
}

#[cfg(target_os = "macos")]
fn load_provider_manifest() -> Result<Vec<String>, String> {
    let raw = match std::fs::read_to_string(credential_manifest_path()) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("read credential manifest: {}", e)),
    };

    serde_json::from_str(&raw).map_err(|e| format!("parse credential manifest: {}", e))
}

#[cfg(target_os = "macos")]
fn save_provider_manifest(providers: &[String]) -> Result<(), String> {
    let dir = app_support_dir();
    std::fs::create_dir_all(&dir).ok();
    let payload = serde_json::to_string(providers)
        .map_err(|e| format!("serialize credential manifest: {}", e))?;
    std::fs::write(credential_manifest_path(), payload)
        .map_err(|e| format!("write credential manifest: {}", e))
}

#[cfg(target_os = "macos")]
fn remember_provider(provider: &str) -> Result<(), String> {
    let mut providers = load_provider_manifest()?;
    if !providers.iter().any(|existing| existing == provider) {
        providers.push(provider.to_string());
        providers.sort();
        save_provider_manifest(&providers)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn forget_provider(provider: &str) -> Result<(), String> {
    let mut providers = load_provider_manifest()?;
    providers.retain(|existing| existing != provider);
    if providers.is_empty() {
        let _ = std::fs::remove_file(credential_manifest_path());
        return Ok(());
    }
    save_provider_manifest(&providers)
}

#[cfg(target_os = "macos")]
fn is_missing_keychain_item(output: &std::process::Output) -> bool {
    if output.status.success() {
        return false;
    }
    let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
    stderr.contains("could not be found")
        || stderr.contains("item not found")
        || output.status.code() == Some(44)
}

/// Store an API key for a provider.
pub fn store_api_key(provider: &str, key: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let output = run_security(&[
            "add-generic-password",
            "-U",
            "-s",
            keychain_service(),
            "-a",
            provider,
            "-w",
            key,
        ])?;
        if !output.status.success() {
            return Err(format!(
                "security add-generic-password failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        remember_provider(provider)?;
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut entries = load_plaintext_credentials()?;
        entries.insert(provider.to_string(), key.to_string());
        save_plaintext_credentials(&entries)
    }
}

/// Retrieve an API key for a provider.
pub fn get_api_key(provider: &str) -> Result<Option<String>, String> {
    #[cfg(target_os = "macos")]
    {
        let output = run_security(&[
            "find-generic-password",
            "-s",
            keychain_service(),
            "-a",
            provider,
            "-w",
        ])?;
        if output.status.success() {
            return Ok(Some(String::from_utf8_lossy(&output.stdout).trim().to_string()));
        }
        if is_missing_keychain_item(&output) {
            return Ok(None);
        }
        return Err(format!(
            "security find-generic-password failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let entries = load_plaintext_credentials()?;
        Ok(entries.get(provider).cloned())
    }
}

/// Check whether secure storage currently contains a key for a provider.
pub fn has_api_key(provider: &str) -> Result<bool, String> {
    Ok(get_api_key(provider)?.is_some())
}

/// Delete all stored credentials.
pub fn clear_credentials() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        for provider in load_provider_manifest()? {
            let output = run_security(&[
                "delete-generic-password",
                "-s",
                keychain_service(),
                "-a",
                &provider,
            ])?;
            if !output.status.success() && !is_missing_keychain_item(&output) {
                return Err(format!(
                    "security delete-generic-password failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            let _ = forget_provider(&provider);
        }
        let _ = std::fs::remove_file(credential_manifest_path());
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = std::fs::remove_file(cred_path());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("hologram-{}-{}", label, stamp))
    }

    #[test]
    fn parse_plaintext_credentials_reads_multiple_entries() {
        let parsed = parse_plaintext_credentials(b"anthropic=key-1\ndeepseek=key-2\n").unwrap();
        assert_eq!(parsed.get("anthropic").map(String::as_str), Some("key-1"));
        assert_eq!(parsed.get("deepseek").map(String::as_str), Some("key-2"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_store_get_and_clear_roundtrip_through_security_cli() {
        let root = unique_temp_dir("credential-test");
        let script_path = root.join("fake-security.sh");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            &script_path,
            r#"#!/bin/sh
set -eu
ROOT="${HOLOGRAM_TEST_SECURITY_ROOT:?}"
STORE="$ROOT/store"
LOG="$ROOT/security.log"
mkdir -p "$STORE"
printf '%s\n' "$*" >> "$LOG"
cmd="$1"
shift || true
case "$cmd" in
  add-generic-password)
    account=""
    password=""
    while [ $# -gt 0 ]; do
      case "$1" in
        -a) account="$2"; shift 2 ;;
        -w) password="$2"; shift 2 ;;
        -s|-U) shift 1 || true ;;
        *) shift 1 ;;
      esac
    done
    printf '%s' "$password" > "$STORE/$account"
    ;;
  find-generic-password)
    account=""
    while [ $# -gt 0 ]; do
      case "$1" in
        -a) account="$2"; shift 2 ;;
        -s|-w) shift 1 || true ;;
        *) shift 1 ;;
      esac
    done
    if [ -f "$STORE/$account" ]; then
      cat "$STORE/$account"
    else
      echo 'The specified item could not be found in the keychain.' >&2
      exit 44
    fi
    ;;
  delete-generic-password)
    account=""
    while [ $# -gt 0 ]; do
      case "$1" in
        -a) account="$2"; shift 2 ;;
        -s) shift 2 ;;
        *) shift 1 ;;
      esac
    done
    rm -f "$STORE/$account"
    ;;
  *)
    echo "unsupported command: $cmd" >&2
    exit 1
    ;;
esac
"#,
        ).unwrap();
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).unwrap();
        }

        std::env::set_var("HOLOGRAM_APP_SUPPORT_DIR", &root);
        std::env::set_var("HOLOGRAM_SECURITY_CLI", &script_path);
        std::env::set_var("HOLOGRAM_TEST_SECURITY_ROOT", &root);

        store_api_key("anthropic", "sk-test").unwrap();
        let restored = get_api_key("anthropic").unwrap();
        assert_eq!(restored.as_deref(), Some("sk-test"));

        let manifest_raw = fs::read_to_string(credential_manifest_path()).unwrap();
        assert!(manifest_raw.contains("anthropic"));

        clear_credentials().unwrap();
        assert_eq!(get_api_key("anthropic").unwrap(), None);
        assert!(!Path::new(&credential_manifest_path()).exists());

        std::env::remove_var("HOLOGRAM_APP_SUPPORT_DIR");
        std::env::remove_var("HOLOGRAM_SECURITY_CLI");
        std::env::remove_var("HOLOGRAM_TEST_SECURITY_ROOT");
        let _ = fs::remove_dir_all(&root);
    }
}

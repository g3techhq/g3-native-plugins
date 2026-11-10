//! Windows: run the new installer once this app has exited, then start the
//! new version.
//!
//! A running `.exe` cannot be overwritten, so the installer must not start
//! until this process is gone, and something has to outlive it to start the
//! installer and then the app. That is a hidden PowerShell, present on every
//! Windows this runs on. It goes through `Start-Process`, which is
//! ShellExecute, so an installer that needs elevation (a per-machine install
//! under Program Files) gets its UAC prompt rather than failing.
//!
//! `dx`'s NSIS installer does not remember where it was installed, so it is
//! told: `/D=` with this executable's folder, or a silent update would land in
//! the default folder beside the old copy.

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

pub(super) fn installers() -> &'static [&'static str] {
    &["nsis", "msi"]
}

pub(super) fn clean_up() {}

/// Which installer a file is, by content: an MSI is an OLE compound file, an
/// NSIS installer an executable.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Installer {
    Nsis,
    Msi,
}

pub(super) fn kind(payload: &Path, hint: Option<&str>) -> Result<Installer, String> {
    let mut magic = [0u8; 4];
    let read = std::fs::File::open(payload)
        .and_then(|mut file| std::io::Read::read(&mut file, &mut magic))
        .map_err(|error| format!("Failed to read the installer: {error}"))?;
    match &magic[..read] {
        [0xD0, 0xCF, 0x11, 0xE0] => Ok(Installer::Msi),
        [b'M', b'Z', ..] => Ok(Installer::Nsis),
        _ => Err(format!(
            "The update is not a Windows installer{}",
            hint.map(|hint| format!(" (the release called it {hint})"))
                .unwrap_or_default()
        )),
    }
}

pub(super) fn install(payload: &Path, hint: Option<&str>) -> Result<(), String> {
    let installer = kind(payload, hint)?;
    let exe = std::env::current_exe()
        .map_err(|error| format!("Failed to find this app's executable: {error}"))?;
    let folder = exe.parent().ok_or("This app's executable has no folder")?;
    let script = script(std::process::id(), &installer, payload, folder, &exe);
    spawn(&script)
}

/// The helper's script. Paths go in single-quoted strings, where only a
/// single quote needs escaping (by doubling it).
pub(super) fn script(
    pid: u32,
    installer: &Installer,
    payload: &Path,
    folder: &Path,
    exe: &Path,
) -> String {
    let quote = |path: &Path| format!("'{}'", path.display().to_string().replace('\'', "''"));
    let run = match installer {
        // NSIS reads `/D=` as the rest of the command line, unquoted, so it
        // goes last; Start-Process joins the array with spaces.
        Installer::Nsis => format!(
            "Start-Process -FilePath {} -ArgumentList '/S', {} -Wait",
            quote(payload),
            quote(Path::new(&format!("/D={}", folder.display()))),
        ),
        // msiexec wants the path quoted inside its one argument string.
        Installer::Msi => format!(
            "Start-Process -FilePath 'msiexec.exe' -ArgumentList {} -Wait",
            quote(Path::new(&format!(
                "/i \"{}\" /passive /norestart",
                payload.display()
            ))),
        ),
    };
    // The app restarts whether or not the installer succeeded: the old
    // version is still there if it did not, which beats no app at all.
    format!(
        "$ErrorActionPreference = 'Continue'\n\
         Wait-Process -Id {pid} -Timeout 60 -ErrorAction SilentlyContinue\n\
         try {{ {run} }} catch {{ }}\n\
         Start-Process -FilePath {}\n",
        quote(exe),
    )
}

fn spawn(script: &str) -> Result<(), String> {
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    // -EncodedCommand takes UTF-16LE in base64, which no quoting rule of
    // the Windows command line can mangle.
    let encoded = encode_utf16_base64(script);
    let command = |flags: u32| {
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-EncodedCommand",
                &encoded,
            ])
            // Nothing to read from it, and its progress records would land in
            // whatever console this app has.
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    // Out of this process's job, so a terminal or launcher that kills its
    // job when the app exits does not take the helper with it. Not every job
    // allows breaking away; then the helper has to stay in it.
    command(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| command(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP))
        .map(|_| ())
        .map_err(|error| format!("Failed to start the installer: {error}"))
}

pub(super) fn encode_utf16_base64(text: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for index in 0..4 {
            if index <= chunk.len() {
                output.push(ALPHABET[(value >> (18 - 6 * index) & 63) as usize] as char);
            } else {
                output.push('=');
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_helper_waits_installs_into_this_folder_and_restarts() {
        let script = script(
            42,
            &Installer::Nsis,
            Path::new(r"C:\Users\O'Neil\cache\Tawny-setup.exe"),
            Path::new(r"C:\Program Files\Tawny"),
            Path::new(r"C:\Program Files\Tawny\tawny.exe"),
        );
        assert!(script.contains("Wait-Process -Id 42"));
        assert!(script.contains(r"-FilePath 'C:\Users\O''Neil\cache\Tawny-setup.exe'"));
        assert!(script.contains(r"-ArgumentList '/S', '/D=C:\Program Files\Tawny' -Wait"));
        assert!(
            script.ends_with("Start-Process -FilePath 'C:\\Program Files\\Tawny\\tawny.exe'\n")
        );
        let script = script_msi();
        assert!(script.contains(r#"'/i "C:\cache\Tawny.msi" /passive /norestart'"#));
    }

    /// Runs the real helper under PowerShell, with batch files standing in
    /// for the installer and the app, in a folder whose name needs quoting.
    /// Ignored by default because it starts PowerShell; run it with
    /// `cargo test --features updater -- --ignored helper_runs`.
    #[test]
    #[ignore = "starts PowerShell"]
    fn the_helper_runs_under_powershell() {
        let root = std::env::temp_dir().join(format!("g3 helper's {}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let installer = root.join("setup.cmd");
        let app = root.join("app.cmd");
        let args = root.join("args.txt");
        let relaunched = root.join("relaunched.txt");
        std::fs::write(&installer, format!("@echo %*> \"{}\"\r\n", args.display())).unwrap();
        std::fs::write(&app, format!("@echo yes> \"{}\"\r\n", relaunched.display())).unwrap();
        // A child that exits at once stands in for the app being waited on.
        let mut waited = std::process::Command::new("cmd")
            .args(["/C", "exit"])
            .spawn()
            .unwrap();
        let pid = waited.id();
        waited.wait().unwrap();
        spawn(&script(pid, &Installer::Nsis, &installer, &root, &app)).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !relaunched.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let passed = std::fs::read_to_string(&args).unwrap_or_default();
        assert_eq!(passed.trim(), format!("/S /D={}", root.display()));
        assert!(relaunched.exists(), "the app was not started again");
        let _ = std::fs::remove_dir_all(root);
    }

    fn script_msi() -> String {
        script(
            1,
            &Installer::Msi,
            Path::new(r"C:\cache\Tawny.msi"),
            Path::new(r"C:\Tawny"),
            Path::new(r"C:\Tawny\tawny.exe"),
        )
    }

    #[test]
    fn installers_are_told_apart_by_content() {
        let root = std::env::temp_dir().join(format!("g3-kind-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("setup.bin");
        std::fs::write(&exe, b"MZ\x90\x00").unwrap();
        assert_eq!(kind(&exe, None).unwrap(), Installer::Nsis);
        let msi = root.join("setup.exe");
        std::fs::write(&msi, [0xD0, 0xCF, 0x11, 0xE0, 0xA1]).unwrap();
        assert_eq!(kind(&msi, None).unwrap(), Installer::Msi);
        std::fs::write(&msi, b"<html>").unwrap();
        assert!(kind(&msi, Some("nsis")).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn encoded_commands_are_utf16_base64() {
        // [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('Hi!'))
        assert_eq!(encode_utf16_base64("Hi!"), "SABpACEA");
        assert_eq!(encode_utf16_base64("é"), "6QA=");
    }
}

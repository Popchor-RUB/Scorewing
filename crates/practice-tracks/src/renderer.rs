//! MuseScore discovery and bounded, cancellable child processes.
use crate::{RendererConfig, RendererEnvironment, Result, SoundCatalog};
use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const LOG_LIMIT: usize = 16 * 1024;
pub(crate) const CONVERSION_TIMEOUT: Duration = Duration::from_secs(600);

pub fn environment(config: RendererConfig) -> RendererEnvironment {
    let mut result = RendererEnvironment {
        executable: None,
        soundfont: None,
        version: None,
        presets: vec![],
        error: None,
    };
    let checked = (|| {
        let executable = find_executable(config.executable.as_deref())?;
        result.executable = Some(executable.to_string_lossy().into_owned());
        result.version = Some(check_version(&executable, &AtomicBool::new(false))?);
        let soundfont = find_soundfont(&executable, config.soundfont.as_deref())?;
        result.soundfont = Some(soundfont.to_string_lossy().into_owned());
        result.presets = SoundCatalog::read(&soundfont)?.presets;
        Ok::<_, String>(())
    })();
    if let Err(error) = checked {
        result.error = Some(error);
    }
    result
}

pub(crate) fn find_executable(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return executable_candidate(path)
            .ok_or_else(|| format!("MuseScore executable not found: {}", path.display()));
    }
    for name in ["MuseScore4", "mscore4", "musescore4", "mscore", "musescore"] {
        if let Some(path) = executable_candidate(Path::new(name)) {
            return Ok(path);
        }
    }
    let mut candidates = vec![];
    if cfg!(target_os = "macos") {
        for base in [
            Some(PathBuf::from("/Applications")),
            std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Applications")),
        ]
        .into_iter()
        .flatten()
        {
            if let Ok(entries) = fs::read_dir(base) {
                for entry in entries.flatten() {
                    if entry.file_name().to_string_lossy().starts_with("MuseScore") {
                        candidates.push(entry.path().join("Contents/MacOS/mscore"));
                    }
                }
            }
        }
    }
    if cfg!(windows) {
        for variable in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(variable) {
                if let Ok(entries) = fs::read_dir(base) {
                    for entry in entries.flatten() {
                        if entry.file_name().to_string_lossy().starts_with("MuseScore") {
                            candidates.push(entry.path().join("bin/MuseScore4.exe"));
                        }
                    }
                }
            }
        }
    }
    candidates.sort();
    candidates.reverse();
    candidates
        .into_iter()
        .find_map(|p| executable_candidate(&p))
        .ok_or_else(|| {
            "MuseScore Studio 4.7.x was not found. Select its executable in renderer settings."
                .into()
        })
}

fn executable_candidate(path: &Path) -> Option<PathBuf> {
    let candidate = if path.components().count() > 1 || path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .flat_map(|base| {
                let plain = base.join(path);
                if cfg!(windows) {
                    vec![plain.clone(), plain.with_extension("exe")]
                } else {
                    vec![plain]
                }
            })
            .find(|p| p.is_file())?
    };
    if !candidate.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&candidate).ok()?.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    candidate.canonicalize().ok()
}

pub(crate) fn find_soundfont(executable: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if path.file_name().and_then(OsStr::to_str) != Some("MS Basic.sf3") {
            return Err(
                "Select the matching MuseScore installation's MS Basic.sf3 SoundFont.".into(),
            );
        }
        return path
            .canonicalize()
            .ok()
            .filter(|p| p.is_file())
            .ok_or_else(|| format!("MS Basic SoundFont not found: {}", path.display()));
    }
    let parent = executable
        .parent()
        .ok_or("Invalid MuseScore executable path")?;
    let mut candidates = vec![
        parent.join("../Resources/sound/MS Basic.sf3"),
        parent.join("sound/MS Basic.sf3"),
    ];
    for base in [
        parent.to_path_buf(),
        parent.parent().unwrap_or(parent).to_path_buf(),
        PathBuf::from("/usr"),
        PathBuf::from("/usr/local"),
    ] {
        let share = base.join("share");
        if let Ok(entries) = fs::read_dir(share) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.starts_with("mscore") || name.starts_with("musescore") {
                    candidates.push(entry.path().join("sound/MS Basic.sf3"));
                }
            }
        }
    }
    candidates.into_iter().find(|p| p.is_file()).and_then(|p| p.canonicalize().ok()).ok_or_else(|| "The matching MS Basic.sf3 was not found. Select it in renderer settings (AppImages require the matching extracted SoundFont).".into())
}

pub(crate) fn check_version(executable: &Path, cancelled: &AtomicBool) -> Result<String> {
    #[cfg(windows)]
    {
        let portable_launcher = executable.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .to_ascii_lowercase()
                .contains("portable")
        });
        let portable_layout = executable.ancestors().any(|root| {
            let app = root.join("App");
            let portable_root = root.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .to_ascii_lowercase()
                    .contains("musescoreportable")
            });
            (root.join("Data/settings").is_dir() || portable_root) && executable.starts_with(app)
        });
        if portable_launcher || portable_layout {
            return Err("Windows portable MuseScore installations use a different preference store and are not supported. Select an installed MuseScore Studio 4.7.x release.".into());
        }
    }
    let output = run(
        executable,
        &[OsString::from("--version")],
        Duration::from_secs(30),
        cancelled,
    )?;
    if output.to_ascii_lowercase().contains("development") {
        return Err("MuseScore development builds use a different preference store. Select a standard MuseScore Studio 4.7.x release.".into());
    }
    output
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '.')
        .find(|s| {
            let pieces: Vec<_> = s.split('.').collect();
            pieces.len() == 3
                && pieces[0] == "4"
                && pieces[1] == "7"
                && !pieces[2].is_empty()
                && pieces[2].bytes().all(|b| b.is_ascii_digit())
        })
        .map(str::to_owned)
        .ok_or_else(|| format!("MuseScore Studio 4.7.x is required; version output: {output}"))
}

pub(crate) fn run(
    executable: &Path,
    arguments: &[OsString],
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("Export cancelled.".into());
    }
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env(
            "MUSESAMPLER_PATH",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        );
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000204); // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_SUSPENDED
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot launch MuseScore {}: {e}", executable.display()))?;
    #[cfg(windows)]
    let job = match windows_job::Job::attach(&child) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let stdout = drain(child.stdout.take().expect("piped stdout"));
    let stderr = drain(child.stderr.take().expect("piped stderr"));
    let started = Instant::now();
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(error) => break Err(format!("Cannot wait for MuseScore: {error}")),
            Ok(None) => {}
        }
        if cancelled.load(Ordering::Relaxed) {
            break Err("Export cancelled.".into());
        }
        if started.elapsed() >= timeout {
            break Err(format!(
                "MuseScore timed out after {} seconds.",
                timeout.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };
    // Descendants can keep pipes open even after the direct process exits.
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    drop(job); // Kill-on-close also terminates descendants after the direct child has exited.
    if outcome.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let diagnostic = format!(
        "{}{}",
        stdout.join().unwrap_or_default(),
        stderr.join().unwrap_or_default()
    );
    match outcome {
        Err(error) => Err(if diagnostic.is_empty() {
            error
        } else {
            format!("{error}\n{diagnostic}")
        }),
        Ok(status) if !status.success() => {
            Err(format!("MuseScore exited with {status}: {diagnostic}"))
        }
        Ok(_) => Ok(diagnostic),
    }
}

#[cfg(windows)]
mod windows_job {
    use super::Result;
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    pub(super) struct Job(HANDLE);
    struct Handle(HANDLE);
    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    impl Job {
        pub(super) fn attach(child: &Child) -> Result<Self> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return Err(failure("create a Windows renderer job"));
                }
                let job = Job(handle);
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&limits) as u32,
                ) == 0
                {
                    return Err(failure("configure Windows renderer containment"));
                }
                if AssignProcessToJobObject(job.0, child.as_raw_handle().cast()) == 0 {
                    return Err(failure("contain the suspended Windows renderer"));
                }
                // std Child does not expose the initial thread handle. Before the loader runs,
                // CREATE_SUSPENDED guarantees this process has only its primary thread.
                let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
                if snapshot == INVALID_HANDLE_VALUE {
                    return Err(failure("inspect the suspended renderer thread"));
                }
                let snapshot = Handle(snapshot);
                let mut entry: THREADENTRY32 = std::mem::zeroed();
                entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
                let mut more = Thread32First(snapshot.0, &mut entry);
                while more != 0 {
                    if entry.th32OwnerProcessID == child.id() {
                        let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                        if thread.is_null() {
                            return Err(failure("open the suspended renderer thread"));
                        }
                        let thread = Handle(thread);
                        if ResumeThread(thread.0) == u32::MAX {
                            return Err(failure("resume the contained renderer"));
                        }
                        return Ok(job);
                    }
                    more = Thread32Next(snapshot.0, &mut entry);
                }
                Err("Cannot find the suspended MuseScore primary thread; renderer was stopped safely.".into())
            }
        }
    }

    fn failure(action: &str) -> String {
        format!("Cannot {action}: {}", std::io::Error::last_os_error())
    }
}

fn drain(mut input: impl Read + Send + 'static) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut tail = VecDeque::with_capacity(LOG_LIMIT);
        let mut buffer = [0; 4096];
        while let Ok(count) = input.read(&mut buffer) {
            if count == 0 {
                break;
            }
            for byte in &buffer[..count] {
                if tail.len() == LOG_LIMIT {
                    tail.pop_front();
                }
                tail.push_back(*byte);
            }
        }
        String::from_utf8_lossy(&tail.into_iter().collect::<Vec<_>>()).into_owned()
    })
}

pub(crate) fn ensure_musescore_closed(executable: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let entries = fs::read_dir("/proc")
            .map_err(|e| format!("Cannot check for running MuseScore: {e}"))?;
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
                continue;
            }
            let comm = fs::read_to_string(entry.path().join("comm"))
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            let same = fs::read_link(entry.path().join("exe")).is_ok_and(|p| p == executable);
            if same || comm.contains("musescore") || comm.starts_with("mscore") {
                return Err("Close MuseScore before exporting, and keep it closed until rendering finishes.".into());
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let (program, arguments): (&str, &[&str]) = if cfg!(windows) {
            ("tasklist", &["/FO", "CSV", "/NH"])
        } else {
            ("ps", &["-ax", "-o", "comm="])
        };
        let output = run(
            Path::new(program),
            &arguments.iter().map(OsString::from).collect::<Vec<_>>(),
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )?;
        for line in output.lines() {
            let line = line.to_ascii_lowercase();
            if line.contains("musescore") || line.contains("mscore") {
                return Err("Close MuseScore before exporting, and keep it closed until rendering finishes.".into());
            }
        }
        let _ = executable;
    }
    Ok(())
}

pub(crate) fn validate_mp3(path: &Path) -> Result<()> {
    let invalid = || format!("MuseScore did not create a valid MP3: {}", path.display());
    let mut file = File::open(path).map_err(|_| invalid())?;
    if file.metadata().map_err(|_| invalid())?.len() < 100 {
        return Err(invalid());
    }
    let mut header = [0; 10];
    file.read_exact(&mut header).map_err(|_| invalid())?;
    let offset = if &header[..3] == b"ID3" {
        if header[6..10].iter().any(|b| b & 0x80 != 0) {
            return Err(invalid());
        }
        10 + header[6..10]
            .iter()
            .fold(0_u64, |n, b| (n << 7) | *b as u64)
            + if header[5] & 0x10 != 0 { 10 } else { 0 }
    } else {
        0
    };
    file.seek(SeekFrom::Start(offset)).map_err(|_| invalid())?;
    let mut bytes = vec![];
    file.take(65_536)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if !bytes.windows(4).any(|b| {
        b[0] == 0xff
            && b[1] & 0xe6 == 0xe2
            && b[1] & 0x18 != 0x08
            && ![0, 0xf0].contains(&(b[2] & 0xf0))
            && b[2] & 0x0c != 0x0c
    }) {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mp3_requires_audio_not_just_a_tag() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.mp3");
        fs::write(&path, vec![0; 200]).unwrap();
        assert!(validate_mp3(&path).is_err());
        let mut bytes = vec![0; 200];
        bytes[..4].copy_from_slice(&[0xff, 0xfb, 0xb0, 0]);
        fs::write(&path, bytes).unwrap();
        assert!(validate_mp3(&path).is_ok());
        fs::write(&path, [b"ID3".as_slice(), &[0; 197]].concat()).unwrap();
        assert!(validate_mp3(&path).is_err());
    }
    #[cfg(unix)]
    fn script(body: &str) -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("renderer");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        (dir, path)
    }
    #[test]
    #[cfg(unix)]
    fn process_version_failure_timeout_and_cancel() {
        let (_dir, path) = script("echo 'MuseScore Studio 4.7.5'");
        assert_eq!(
            check_version(&path, &AtomicBool::new(false)).unwrap(),
            "4.7.5"
        );
        let (_dir, path) = script("echo 4.8.0");
        assert!(check_version(&path, &AtomicBool::new(false)).is_err());
        let (_dir, path) = script("echo 'MuseScore4Development 4.7.5'");
        assert!(check_version(&path, &AtomicBool::new(false))
            .unwrap_err()
            .contains("development"));
        let (_dir, path) = script("echo broken >&2; exit 7");
        assert!(
            run(&path, &[], Duration::from_secs(1), &AtomicBool::new(false))
                .unwrap_err()
                .contains("broken")
        );
        let (_dir, path) = script("sleep 30 & wait");
        let before = Instant::now();
        assert!(run(
            &path,
            &[],
            Duration::from_millis(100),
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .contains("timed out"));
        assert!(before.elapsed() < Duration::from_secs(3));
        assert!(
            run(&path, &[], Duration::from_secs(1), &AtomicBool::new(true))
                .unwrap_err()
                .contains("cancelled")
        );
    }
    #[test]
    #[cfg(windows)]
    fn exiting_windows_parent_cannot_leave_descendants_holding_output_pipes() {
        let command = std::env::var_os("ComSpec")
            .map(PathBuf::from)
            .expect("Windows must provide its command processor");
        let powershell =
            PathBuf::from(std::env::var_os("SystemRoot").expect("Windows must provide SystemRoot"))
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        assert!(
            powershell.is_file(),
            "Windows PowerShell is required for this process regression test"
        );
        let started = Instant::now();
        run(
            &command,
            &[
                "/C".into(),
                "start".into(),
                "".into(),
                "/B".into(),
                powershell.into_os_string(),
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                "Start-Sleep -Seconds 30".into(),
            ],
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "A descendant retained the renderer's inherited output pipes after its parent exited"
        );
    }
    #[test]
    fn log_tail_is_bounded() {
        let log = drain(std::io::Cursor::new(vec![b'x'; LOG_LIMIT * 3]))
            .join()
            .unwrap();
        assert_eq!(log.len(), LOG_LIMIT);
    }
}

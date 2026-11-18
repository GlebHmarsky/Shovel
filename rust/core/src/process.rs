//! Contained native Windows processes. Call from a worker, never the UI thread.
//! Accepted commands cannot be cancelled once CreateProcessW succeeds. Read and
//! automatic-fetch requests retain cancellation/deadlines and descendant cleanup.
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs::File,
    io::{Read, Write},
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::SECURITY_ATTRIBUTES,
    System::{JobObjects::*, Pipes::CreatePipe, Threading::*},
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

// Background reads cannot exhaust capacity reserved for accepted user work.
// A permit covers the entire process tree and both output drains.
struct Admission {
    count: Mutex<usize>,
    changed: Condvar,
    limit: usize,
}
static READS: Admission = Admission {
    count: Mutex::new(0),
    changed: Condvar::new(),
    limit: 2,
};
static COMMANDS: Admission = Admission {
    count: Mutex::new(0),
    changed: Condvar::new(),
    limit: 4,
};
struct Permit(&'static Admission);
impl Drop for Permit {
    fn drop(&mut self) {
        *self.0.count.lock().unwrap() -= 1;
        self.0.changed.notify_one();
    }
}
fn admit(lifetime: Lifetime, cancel: &Cancellation) -> Option<Permit> {
    let lane = if lifetime == Lifetime::AcceptedCommand {
        &COMMANDS
    } else {
        &READS
    };
    let mut count = lane.count.lock().unwrap();
    loop {
        if cancel.is_cancelled() {
            return None;
        }
        if *count < lane.limit {
            *count += 1;
            return Some(Permit(lane));
        }
        count = lane
            .changed
            .wait_timeout(count, Duration::from_millis(50))
            .unwrap()
            .0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifetime {
    Read,
    AutomaticFetch,
    AcceptedCommand,
}

pub struct Request {
    pub executable: PathBuf,
    pub directory: PathBuf,
    pub arguments: Vec<OsString>,
    pub environment: BTreeMap<OsString, Option<OsString>>,
    pub input: Vec<u8>,
    pub capture_limit: usize,
    pub deadline: Duration,
    pub lifetime: Lifetime,
}
impl Request {
    pub fn new(
        executable: PathBuf,
        directory: PathBuf,
        arguments: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Self {
        Self {
            executable,
            directory,
            arguments: arguments.into_iter().map(Into::into).collect(),
            environment: BTreeMap::new(),
            input: Vec::new(),
            capture_limit: 256 * 1024,
            deadline: Duration::from_secs(15),
            lifetime: Lifetime::Read,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Exited,
    Cancelled,
    TimedOut,
    OutputLimit,
}
impl Outcome {
    /// An accepted process runs to its native exit even when the diagnostic buffer fills.
    pub fn accepted_succeeded(self, exit_code: u32) -> bool {
        exit_code == 0 && matches!(self, Self::Exited | Self::OutputLimit)
    }
}
#[derive(Debug)]
pub struct Result {
    pub outcome: Outcome,
    pub exit_code: u32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub elapsed: Duration,
}
impl Result {
    pub fn success(&self) -> bool {
        self.outcome == Outcome::Exited && self.exit_code == 0
    }
    pub fn command_succeeded(&self) -> bool {
        self.outcome.accepted_succeeded(self.exit_code)
    }
}
pub type OutputCallback = Arc<dyn Fn(bool, &[u8]) + Send + Sync>;

fn invalid() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid process request")
}
fn wide(value: &OsStr) -> std::io::Result<Vec<u16>> {
    let mut result: Vec<u16> = value.encode_wide().collect();
    if result.contains(&0) {
        return Err(invalid());
    }
    result.push(0);
    Ok(result)
}
fn quote(value: &OsStr) -> std::io::Result<Vec<u16>> {
    let raw: Vec<u16> = value.encode_wide().collect();
    if raw.contains(&0) {
        return Err(invalid());
    }
    let mut out = vec![b'"' as u16];
    let mut slashes = 0;
    for value in raw {
        if value == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        if value == b'"' as u16 {
            out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2 + 1));
        } else {
            out.extend(std::iter::repeat_n(b'\\' as u16, slashes));
        }
        slashes = 0;
        out.push(value);
    }
    out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    out.push(b'"' as u16);
    Ok(out)
}
fn compare_environment_names(left: &[u16], right: &[u16]) -> std::cmp::Ordering {
    use windows_sys::Win32::Globalization::{
        CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN, CompareStringOrdinal,
    };
    let (Ok(left_len), Ok(right_len)) = (left.len().try_into(), right.len().try_into()) else {
        return left.cmp(right);
    };
    // Windows compares variable names ordinally. Each count is the exact size
    // of its UTF-16 buffer, including any unpaired surrogate code units.
    match unsafe { CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1) } {
        CSTR_LESS_THAN => std::cmp::Ordering::Less,
        CSTR_EQUAL => std::cmp::Ordering::Equal,
        CSTR_GREATER_THAN => std::cmp::Ordering::Greater,
        _ => left.cmp(right),
    }
}
fn environment(request: &Request) -> std::io::Result<Vec<u16>> {
    let mut values = Vec::new();
    for (name, value) in std::env::vars_os() {
        values.push((wide(&name)?, Some(value)));
    }
    for (name, value) in &request.environment {
        let name = wide(name)?;
        if name.len() == 1 || name.contains(&u16::from(b'=')) {
            return Err(invalid());
        }
        values.push((name, value.clone()));
    }
    // Stable ordering puts explicit overrides after inherited names, including
    // alternate spellings. A final removal omits the inherited entry entirely.
    values.sort_by(|a, b| compare_environment_names(&a.0, &b.0));
    let mut block = Vec::new();
    for group in values.chunk_by(|a, b| compare_environment_names(&a.0, &b.0).is_eq()) {
        if let Some((name, Some(value))) = group.last() {
            block.extend_from_slice(&name[..name.len() - 1]);
            block.push(u16::from(b'='));
            block.extend(wide(value)?);
        }
    }
    block.push(0);
    if block.len() == 1 {
        block.push(0);
    }
    Ok(block)
}
fn owned(handle: HANDLE) -> std::io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: callers pass a newly created owned Windows handle exactly once.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}
fn pipe(parent_reads: bool) -> std::io::Result<(OwnedHandle, OwnedHandle)> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let mut read = std::ptr::null_mut();
    let mut write = std::ptr::null_mut();
    // SAFETY: valid output pointers and a correctly sized security structure.
    if unsafe { CreatePipe(&mut read, &mut write, &attributes, 0) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let read = owned(read)?;
    let write = owned(write)?;
    let parent = if parent_reads { &read } else { &write };
    // Only the child's end may appear in inherited handle lists.
    if unsafe { SetHandleInformation(parent.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((read, write))
}

struct Attributes {
    allocation: Vec<usize>,
    job_value: Box<HANDLE>,
    initialized: bool,
}
impl Attributes {
    fn new(job: &OwnedHandle, handles: &mut [HANDLE; 3]) -> std::io::Result<Self> {
        let mut size = 0;
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut size) };
        if size == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut result = Self {
            allocation: vec![0; size.div_ceil(size_of::<usize>())],
            job_value: Box::new(job.as_raw_handle()),
            initialized: false,
        };
        if unsafe { InitializeProcThreadAttributeList(result.ptr(), 2, 0, &mut size) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        result.initialized = true;
        // The pointed-to values remain alive until the attribute list is deleted.
        let job_pointer = (&mut *result.job_value as *mut HANDLE).cast();
        if unsafe {
            UpdateProcThreadAttribute(
                result.ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_mut_ptr().cast(),
                size_of::<[HANDLE; 3]>(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        } == 0
            || unsafe {
                UpdateProcThreadAttribute(
                    result.ptr(),
                    0,
                    PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                    job_pointer,
                    size_of::<HANDLE>(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(result)
    }
    fn ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.allocation.as_mut_ptr().cast()
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { DeleteProcThreadAttributeList(self.ptr()) };
        }
    }
}

fn drain(
    handle: OwnedHandle,
    limit: usize,
    overflow: Arc<AtomicBool>,
    callback: Option<OutputCallback>,
    is_error: bool,
) -> std::io::Result<thread::JoinHandle<std::io::Result<Vec<u8>>>> {
    thread::Builder::new().spawn(move || {
        let mut file = File::from(handle);
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            let remaining = limit.saturating_sub(bytes.len());
            let captured = remaining.min(count);
            bytes.extend_from_slice(&buffer[..captured]);
            if captured > 0
                && let Some(callback) = &callback
            {
                callback(is_error, &buffer[..captured]);
            }
            if count > remaining {
                overflow.store(true, Ordering::Release);
            }
        }
        Ok(bytes)
    })
}

pub fn run(
    request: &Request,
    cancel: &Cancellation,
    callback: Option<OutputCallback>,
) -> std::io::Result<Result> {
    let start = Instant::now();
    if !request.executable.is_absolute()
        || !request.directory.is_absolute()
        || request.capture_limit == 0
        || request.capture_limit > 64 * 1024 * 1024
        || request.input.len() > 64 * 1024 * 1024
    {
        return Err(invalid());
    }
    let permit = admit(request.lifetime, cancel);
    if permit.is_none() {
        return Ok(Result {
            outcome: Outcome::Cancelled,
            exit_code: 0,
            stdout: vec![],
            stderr: vec![],
            elapsed: start.elapsed(),
        });
    }
    let _permit = permit;
    // Match the native baseline: admission waiting contributes to displayed
    // elapsed time, but does not consume the process execution deadline.
    let admitted_at = Instant::now();
    let executable = wide(request.executable.as_os_str())?;
    let directory = wide(request.directory.as_os_str())?;
    let mut command = quote(request.executable.as_os_str())?;
    for argument in &request.arguments {
        command.push(b' ' as u16);
        command.extend(quote(argument)?);
    }
    command.push(0);
    if command.len() > 32767 {
        return Err(invalid());
    }
    let block = environment(request)?;
    let job = owned(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) })?;
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    let (stdout_read, stdout_write) = pipe(true)?;
    let (stderr_read, stderr_write) = pipe(true)?;
    let (stdin_read, stdin_write) = pipe(false)?;
    let mut handles = [
        stdin_read.as_raw_handle(),
        stdout_write.as_raw_handle(),
        stderr_write.as_raw_handle(),
    ];
    let mut attributes = Attributes::new(&job, &mut handles)?;
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.ptr();
    // Reserve the capture threads before starting a native command. If Windows
    // cannot create a thread, an accepted child has not yet started.
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout = drain(
        stdout_read,
        request.capture_limit,
        overflow.clone(),
        callback.clone(),
        false,
    )?;
    let stderr = drain(
        stderr_read,
        request.capture_limit,
        overflow.clone(),
        callback,
        true,
    )?;
    let writer = if request.input.is_empty() {
        drop(stdin_write);
        None
    } else {
        let input = request.input.clone();
        Some(thread::Builder::new().spawn(move || File::from(stdin_write).write_all(&input))?)
    };
    let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
    // Atomic job assignment means no child may run outside owned containment.
    let started = unsafe {
        CreateProcessW(
            executable.as_ptr(),
            command.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            block.as_ptr().cast(),
            directory.as_ptr(),
            &startup.StartupInfo,
            &mut process,
        )
    };
    let start_error = (started == 0).then(std::io::Error::last_os_error);
    drop(attributes);
    drop(stdin_read);
    drop(stdout_write);
    drop(stderr_write);
    if let Some(error) = start_error {
        // Closing the unused child handles wakes every pipe worker.
        let _ = stdout.join();
        let _ = stderr.join();
        if let Some(writer) = writer {
            let _ = writer.join();
        }
        return Err(error);
    }
    let process_handle = owned(process.hProcess)?;
    drop(owned(process.hThread)?);
    let mut outcome = Outcome::Exited;
    let wait_ms = if request.lifetime == Lifetime::AcceptedCommand {
        INFINITE
    } else {
        20
    };
    loop {
        let wait = unsafe { WaitForSingleObject(process_handle.as_raw_handle(), wait_ms) };
        if wait == WAIT_OBJECT_0 {
            break;
        }
        if wait == WAIT_FAILED {
            return Err(std::io::Error::last_os_error());
        }
        if request.lifetime != Lifetime::AcceptedCommand {
            outcome = if cancel.is_cancelled() {
                Outcome::Cancelled
            } else if overflow.load(Ordering::Acquire) {
                Outcome::OutputLimit
            } else if admitted_at.elapsed() >= request.deadline {
                Outcome::TimedOut
            } else {
                Outcome::Exited
            };
            if outcome != Outcome::Exited {
                if unsafe { TerminateJobObject(job.as_raw_handle(), 1) } == 0 {
                    return Err(std::io::Error::last_os_error());
                }
                unsafe { WaitForSingleObject(process_handle.as_raw_handle(), INFINITE) };
                break;
            }
        }
    }
    let mut exit_code = 0;
    if unsafe { GetExitCodeProcess(process_handle.as_raw_handle(), &mut exit_code) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // A completed command cannot leave pipe-inheriting descendants alive.
    drop(job);
    let join_error = || std::io::Error::other("process capture worker failed");
    let stdout = stdout.join().map_err(|_| join_error())??;
    let stderr = stderr.join().map_err(|_| join_error())??;
    if let Some(writer) = writer {
        let input_result = writer.join().map_err(|_| join_error())?;
        if outcome == Outcome::Exited && exit_code == 0 {
            input_result?;
        }
    }
    if overflow.load(Ordering::Acquire) && outcome == Outcome::Exited {
        outcome = Outcome::OutputLimit;
    }
    Ok(Result {
        outcome,
        exit_code,
        stdout,
        stderr,
        elapsed: start.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_fixture() {
        let Ok(mode) = std::env::var("SHOVEL_PROCESS_FIXTURE") else {
            return;
        };
        if mode == "finish" || mode == "cancel" {
            std::fs::write(
                std::env::var_os("SHOVEL_FIXTURE_STARTED").unwrap(),
                b"started",
            )
            .unwrap();
            thread::sleep(Duration::from_millis(if mode == "finish" {
                250
            } else {
                5000
            }));
            std::fs::write(
                std::env::var_os("SHOVEL_FIXTURE_FINISHED").unwrap(),
                b"finished",
            )
            .unwrap();
        } else if mode == "hold" {
            std::fs::write("started", "").unwrap();
            let end = Instant::now() + Duration::from_secs(15);
            while !std::path::Path::new("release").exists() && Instant::now() < end {
                thread::sleep(Duration::from_millis(5));
            }
        } else if mode == "descendant-root" {
            use std::os::windows::process::CommandExt;
            // Deliberately outlive this fixture root: the outer Windows job
            // must terminate the descendant, verified through its process handle.
            #[allow(clippy::zombie_processes)]
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "process::tests::child_fixture", "--nocapture"])
                .env("SHOVEL_PROCESS_FIXTURE", "descendant-leaf")
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .unwrap();
            std::fs::write("descendant-pid", child.id().to_string()).unwrap();
            let end = Instant::now() + Duration::from_secs(20);
            while !std::path::Path::new("root-release").exists() && Instant::now() < end {
                thread::sleep(Duration::from_millis(5));
            }
        } else if mode == "descendant-leaf" {
            println!("descendant owns stdout");
            eprintln!("descendant owns stderr");
            std::fs::write("descendant-started", "").unwrap();
            thread::sleep(Duration::from_secs(30));
            std::fs::write("descendant-finished", "").unwrap();
        } else if mode == "inherited-event" {
            let handle = std::env::var("SHOVEL_FIXTURE_HANDLE")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            unsafe {
                SetEvent(handle as *mut std::ffi::c_void);
            }
        } else if mode == "read-stdin" {
            let mut bytes = Vec::new();
            std::io::stdin().read_to_end(&mut bytes).unwrap();
            std::fs::write("received-stdin", bytes).unwrap();
        } else if mode == "duplex" {
            // Both pipes must drain before this child consumes its input.
            std::io::stdout().write_all(&vec![0; 128 * 1024]).unwrap();
            std::io::stderr().write_all(&vec![0xff; 96 * 1024]).unwrap();
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input).unwrap();
            std::fs::write("received-stdin", input).unwrap();
        } else if mode == "reject-input" {
            eprintln!("fixture rejected input");
            std::process::exit(7);
        } else if mode == "ignore-input" {
            std::process::exit(0);
        } else if mode == "overflow" {
            println!("{}", "x".repeat(65536));
        } else if mode == "arguments" {
            let arguments: Vec<Vec<u16>> = std::env::args_os()
                .skip_while(|arg| arg != "--")
                .skip(1)
                .map(|arg| arg.encode_wide().collect())
                .collect();
            println!("\nARGV:{}", serde_json::to_string(&arguments).unwrap());
        } else if mode == "environment" {
            println!("{}", std::env::var("SHOVEL_LITERAL_VALUE").unwrap());
        } else if mode == "environment-parent" {
            let mut nested = request(&std::env::current_dir().unwrap(), "environment-names");
            nested
                .environment
                .insert("shovel_fixture_omitted".into(), None);
            nested
                .environment
                .insert("shovel_fixture_strasse".into(), Some("double s".into()));
            let result = run(&nested, &Cancellation::default(), None).unwrap();
            assert!(
                result.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        } else if mode == "environment-names" {
            for (key, value) in fixture_environment_names() {
                assert_eq!(std::env::var_os(&key), Some(value));
            }
            assert!(std::env::var_os("SHOVEL_FIXTURE_OMITTED").is_none());
        }
    }
    fn fixture_environment_names() -> Vec<(OsString, OsString)> {
        use std::os::windows::ffi::OsStringExt;
        let mut raw: Vec<u16> = "SHOVEL_FIXTURE_".encode_utf16().collect();
        raw.push(0xd800);
        let first = OsString::from_wide(&raw);
        *raw.last_mut().unwrap() = 0xd801;
        vec![
            ("SHOVEL_FIXTURE_Straße".into(), "sharp s".into()),
            ("SHOVEL_FIXTURE_STRASSE".into(), "double s".into()),
            (first, "first surrogate".into()),
            (OsString::from_wide(&raw), OsString::from_wide(&[0xd802])),
        ]
    }
    fn request(temp: &std::path::Path, mode: &str) -> Request {
        let mut request = Request::new(
            std::env::current_exe().unwrap(),
            temp.to_path_buf(),
            ["--exact", "process::tests::child_fixture", "--nocapture"],
        );
        request
            .environment
            .insert("SHOVEL_PROCESS_FIXTURE".into(), Some(mode.into()));
        request.environment.insert(
            "SHOVEL_FIXTURE_STARTED".into(),
            Some(temp.join("started").into_os_string()),
        );
        request.environment.insert(
            "SHOVEL_FIXTURE_FINISHED".into(),
            Some(temp.join("finished").into_os_string()),
        );
        request
    }
    #[test]
    fn accepted_command_finishes_after_cancel_and_deadline() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_path_buf();
        let mut request = request(&path, "finish");
        request.lifetime = Lifetime::AcceptedCommand;
        request.deadline = Duration::from_millis(1);
        let cancel = Cancellation::default();
        let other = cancel.clone();
        let canceller = thread::spawn(move || {
            let start = Instant::now();
            while !path.join("started").exists() && start.elapsed() < Duration::from_secs(5) {
                thread::sleep(Duration::from_millis(5));
            }
            other.cancel();
        });
        let result = run(&request, &cancel, None).unwrap();
        canceller.join().unwrap();
        assert!(result.success(), "{result:?}");
        assert!(temp.path().join("finished").exists());
    }
    #[test]
    fn obsolete_read_can_be_cancelled_without_finishing() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().to_path_buf();
        let request = request(&path, "cancel");
        let cancel = Cancellation::default();
        let other = cancel.clone();
        let canceller = thread::spawn(move || {
            let start = Instant::now();
            while !path.join("started").exists() && start.elapsed() < Duration::from_secs(5) {
                thread::sleep(Duration::from_millis(5));
            }
            other.cancel();
        });
        let result = run(&request, &cancel, None).unwrap();
        canceller.join().unwrap();
        assert_eq!(result.outcome, Outcome::Cancelled);
        assert!(!temp.path().join("finished").exists());
    }
    #[test]
    fn native_process_receives_immediate_eof_or_exact_input() {
        let temp = tempfile::tempdir().unwrap();
        for input in [vec![], vec![0, 10, 255, 65]] {
            let mut request = request(temp.path(), "read-stdin");
            request.input.clone_from(&input);
            request.lifetime = Lifetime::AcceptedCommand;
            assert!(
                run(&request, &Cancellation::default(), None)
                    .unwrap()
                    .command_succeeded()
            );
            assert_eq!(
                std::fs::read(temp.path().join("received-stdin")).unwrap(),
                input
            );
        }
    }
    #[test]
    fn failed_native_start_drains_the_prepared_pipes() {
        let temp = tempfile::tempdir().unwrap();
        let mut missing = request(temp.path(), "read-stdin");
        missing.executable = temp.path().join("missing.exe");
        missing.lifetime = Lifetime::AcceptedCommand;
        missing.input = vec![b'x'; 512 * 1024];
        assert_eq!(
            run(&missing, &Cancellation::default(), None)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
        assert!(
            run(
                &request(temp.path(), "read-stdin"),
                &Cancellation::default(),
                None
            )
            .unwrap()
            .success()
        );
    }

    #[test]
    fn simultaneous_output_and_input_do_not_block_the_native_pipes() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = request(temp.path(), "duplex");
        request.input = (0..192 * 1024).map(|index| (index % 256) as u8).collect();
        request.capture_limit = 256 * 1024;
        request.deadline = Duration::from_secs(8);
        let streamed = Arc::new(Mutex::new([Vec::new(), Vec::new()]));
        let captured = streamed.clone();
        let result = run(
            &request,
            &Cancellation::default(),
            Some(Arc::new(move |stderr, bytes| {
                captured.lock().unwrap()[usize::from(stderr)].extend_from_slice(bytes);
            })),
        )
        .unwrap();
        assert!(result.success(), "{:?}", result.outcome);
        assert_eq!(
            std::fs::read(temp.path().join("received-stdin")).unwrap(),
            request.input
        );
        assert_eq!(
            result.stdout.iter().filter(|byte| **byte == 0).count(),
            128 * 1024
        );
        assert_eq!(result.stderr, vec![0xff; 96 * 1024]);
        let streamed = streamed.lock().unwrap();
        assert_eq!(streamed[0], result.stdout);
        assert_eq!(streamed[1], result.stderr);
    }

    #[test]
    fn incomplete_stdin_is_not_reported_as_success_or_hidden_native_failure() {
        let temp = tempfile::tempdir().unwrap();
        for (mode, failed) in [("ignore-input", false), ("reject-input", true)] {
            let mut request = request(temp.path(), mode);
            request.input = vec![b'x'; 2 * 1024 * 1024];
            request.lifetime = Lifetime::AcceptedCommand;
            let result = run(&request, &Cancellation::default(), None);
            if failed {
                let result = result.unwrap();
                assert_eq!(result.exit_code, 7);
                assert_eq!(result.outcome, Outcome::Exited);
                assert!(
                    String::from_utf8(result.stderr)
                        .unwrap()
                        .contains("fixture rejected input")
                );
            } else {
                assert!(
                    result.is_err(),
                    "undelivered input cannot count as a successful command"
                );
            }
        }
    }
    #[test]
    fn root_exit_terminates_pipe_inheriting_descendant_and_drains_output() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = request(temp.path(), "descendant-root");
        request.lifetime = Lifetime::AcceptedCommand;
        struct Release(PathBuf);
        impl Drop for Release {
            fn drop(&mut self) {
                let _ = std::fs::write(&self.0, "");
            }
        }
        let release = Release(temp.path().join("root-release"));
        let worker = thread::spawn(move || run(&request, &Cancellation::default(), None));
        let end = Instant::now() + Duration::from_secs(10);
        while !temp.path().join("descendant-started").exists() {
            assert!(Instant::now() < end, "descendant did not start");
            thread::sleep(Duration::from_millis(5));
        }
        let pid = std::fs::read_to_string(temp.path().join("descendant-pid"))
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let descendant = owned(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) }).unwrap();
        assert_eq!(
            unsafe { WaitForSingleObject(descendant.as_raw_handle(), 0) },
            WAIT_TIMEOUT
        );
        drop(release);
        let result = worker.join().unwrap().unwrap();
        assert!(result.success(), "{result:?}");
        assert_eq!(
            unsafe { WaitForSingleObject(descendant.as_raw_handle(), 2000) },
            WAIT_OBJECT_0
        );
        assert!(!temp.path().join("descendant-finished").exists());
        assert!(
            String::from_utf8(result.stdout)
                .unwrap()
                .contains("descendant owns stdout")
        );
        assert!(
            String::from_utf8(result.stderr)
                .unwrap()
                .contains("descendant owns stderr")
        );
    }
    #[test]
    fn unrelated_inheritable_handle_is_excluded_from_child() {
        let temp = tempfile::tempdir().unwrap();
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        let event = owned(unsafe { CreateEventW(&attributes, 1, 0, std::ptr::null()) }).unwrap();
        let mut request = request(temp.path(), "inherited-event");
        request.environment.insert(
            "SHOVEL_FIXTURE_HANDLE".into(),
            Some((event.as_raw_handle() as usize).to_string().into()),
        );
        let result = run(&request, &Cancellation::default(), None).unwrap();
        assert!(result.success());
        assert_eq!(
            unsafe { WaitForSingleObject(event.as_raw_handle(), 0) },
            WAIT_TIMEOUT
        );
    }
    // Each saturation test deliberately owns every permit in a process-wide
    // lane. Give it an independent turn when libtest runs tests concurrently.
    static SATURATION_FIXTURE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct Held {
        temp: tempfile::TempDir,
        workers: Vec<thread::JoinHandle<std::io::Result<Result>>>,
        paths: Vec<PathBuf>,
    }
    impl Held {
        fn new(lifetime: Lifetime, count: usize) -> Self {
            let mut held = Self {
                temp: tempfile::tempdir().unwrap(),
                workers: vec![],
                paths: vec![],
            };
            for index in 0..count {
                let path = held.temp.path().join(index.to_string());
                std::fs::create_dir(&path).unwrap();
                let mut request = request(&path, "hold");
                request.lifetime = lifetime;
                held.paths.push(path.clone());
                held.workers.push(thread::spawn(move || {
                    run(&request, &Cancellation::default(), None)
                }));
                let end = Instant::now() + Duration::from_secs(5);
                while !path.join("started").exists() {
                    assert!(Instant::now() < end, "held native process did not start");
                    thread::sleep(Duration::from_millis(5));
                }
            }
            held
        }
        fn release(&self) {
            for path in &self.paths {
                let _ = std::fs::write(path.join("release"), "");
            }
        }
        fn finish(mut self) {
            self.release();
            for worker in self.workers.drain(..) {
                assert!(worker.join().unwrap().unwrap().success());
            }
        }
    }
    impl Drop for Held {
        fn drop(&mut self) {
            self.release();
            for worker in self.workers.drain(..) {
                let _ = worker.join();
            }
        }
    }
    #[test]
    fn queued_read_receives_full_execution_deadline_after_admission() {
        let _fixture = SATURATION_FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
        let held = Held::new(Lifetime::Read, 2);
        let temp = tempfile::tempdir().unwrap();
        let mut request = request(temp.path(), "finish");
        request.deadline = Duration::from_millis(800);
        let (ready, began) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            ready.send(()).unwrap();
            run(&request, &Cancellation::default(), None)
        });
        began.recv().unwrap();
        thread::sleep(Duration::from_millis(1100));
        assert!(!temp.path().join("started").exists());
        held.finish();
        let result = worker.join().unwrap().unwrap();
        assert!(result.success(), "{result:?}");
        assert!(result.elapsed >= Duration::from_millis(1100));
        assert!(temp.path().join("finished").exists());
    }
    #[test]
    fn saturated_lanes_remain_independent_and_cancelled_waiters_never_start() {
        let _fixture = SATURATION_FIXTURE.lock().unwrap_or_else(|e| e.into_inner());
        for (blocked, count, available) in [
            (Lifetime::Read, 2, Lifetime::AcceptedCommand),
            (Lifetime::AcceptedCommand, 4, Lifetime::Read),
        ] {
            let held = Held::new(blocked, count);
            let temp = tempfile::tempdir().unwrap();
            let mut available_request = request(temp.path(), "finish");
            available_request.lifetime = available;
            assert!(
                run(&available_request, &Cancellation::default(), None)
                    .unwrap()
                    .success()
            );
            let waiting = tempfile::tempdir().unwrap();
            let mut waiting_request = request(waiting.path(), "finish");
            waiting_request.lifetime = blocked;
            let cancel = Cancellation::default();
            let worker_cancel = cancel.clone();
            let (ready, began) = std::sync::mpsc::channel();
            let worker = thread::spawn(move || {
                ready.send(()).unwrap();
                run(&waiting_request, &worker_cancel, None)
            });
            began.recv().unwrap();
            thread::sleep(Duration::from_millis(100));
            assert!(!worker.is_finished());
            cancel.cancel();
            let result = worker.join().unwrap().unwrap();
            assert_eq!(result.outcome, Outcome::Cancelled);
            assert!(!waiting.path().join("started").exists());
            held.finish();
        }
    }
    #[test]
    fn bounded_capture_drains_accepted_commands() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = request(temp.path(), "overflow");
        request.capture_limit = 128;
        request.lifetime = Lifetime::AcceptedCommand;
        let result = run(&request, &Cancellation::default(), None).unwrap();
        assert_eq!(result.outcome, Outcome::OutputLimit);
        assert_eq!(result.stdout.len(), 128);
    }
    #[test]
    fn actual_unicode_empty_quoted_and_trailing_slash_arguments_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = request(temp.path(), "arguments");
        let arguments = [
            "",
            "a b",
            "שלום 😀",
            "--leading",
            "quote\"inside",
            "C:\\space dir\\",
            "two\\\\\"quotes",
            "& $() ; %PATH%",
        ];
        // libtest accepts multiple exact filters after --. None of these
        // payload filters names a test; the earlier exact fixture filter does.
        request.arguments.push("--".into());
        request
            .arguments
            .extend(arguments.iter().map(OsString::from));
        let result = run(&request, &Cancellation::default(), None).unwrap();
        assert!(result.success(), "{result:?}");
        let output = String::from_utf8(result.stdout).unwrap();
        let line = output
            .lines()
            .find_map(|line| line.strip_prefix("ARGV:"))
            .expect("native fixture did not report arguments");
        let observed: Vec<Vec<u16>> = serde_json::from_str(line).unwrap();
        let expected: Vec<Vec<u16>> = arguments
            .iter()
            .map(|s| OsStr::new(s).encode_wide().collect())
            .collect();
        assert_eq!(observed, expected);
    }
    #[test]
    fn child_receives_distinct_native_environment_names_and_values() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = request(temp.path(), "environment-parent");
        for (name, value) in fixture_environment_names() {
            request.environment.insert(name, Some(value));
        }
        request
            .environment
            .insert("SHOVEL_FIXTURE_OMITTED".into(), Some("earlier".into()));
        request.environment.insert(
            "SHOVEL_FIXTURE_STRASSE".into(),
            Some("inherited value".into()),
        );
        let result = run(&request, &Cancellation::default(), None).unwrap();
        assert!(
            result.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    #[test]
    fn malformed_environment_names_are_rejected_even_on_removal() {
        let temp = tempfile::tempdir().unwrap();
        for name in ["", "=C:", "has=equals", "embedded\0nul"] {
            let mut request = request(temp.path(), "environment");
            request.environment.insert(name.into(), None);
            let Err(error) = environment(&request) else {
                panic!("invalid removal name was accepted");
            };
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        }
    }
    #[test]
    fn environment_is_literal_and_argument_quoting_matches_crt() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = request(temp.path(), "environment");
        let literal = "Unicode שלום & $() ` ; spaces ";
        request
            .environment
            .insert("SHOVEL_LITERAL_VALUE".into(), Some(literal.into()));
        let result = run(&request, &Cancellation::default(), None).unwrap();
        assert!(result.success());
        assert!(String::from_utf8(result.stdout).unwrap().contains(literal));
        assert_eq!(
            String::from_utf16(&quote(OsStr::new("")).unwrap()).unwrap(),
            "\"\""
        );
        assert_eq!(
            String::from_utf16(&quote(OsStr::new("C:\\space dir\\")).unwrap()).unwrap(),
            "\"C:\\space dir\\\\\""
        );
    }
}

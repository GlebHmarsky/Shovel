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
fn environment(request: &Request) -> std::io::Result<Vec<u16>> {
    let mut values: BTreeMap<String, (OsString, OsString)> = std::env::vars_os()
        .map(|(k, v)| (k.to_string_lossy().to_uppercase(), (k, v)))
        .collect();
    for (name, value) in &request.environment {
        if name.is_empty() || name.to_string_lossy().contains('=') {
            return Err(invalid());
        }
        let key = name.to_string_lossy().to_uppercase();
        if let Some(value) = value {
            values.insert(key, (name.clone(), value.clone()));
        } else {
            values.remove(&key);
        }
    }
    let mut block = Vec::new();
    for (_, (name, value)) in values {
        let mut entry = name;
        entry.push("=");
        entry.push(value);
        block.extend(wide(&entry)?);
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
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
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
    let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
    // Atomic job assignment means no child may run outside owned containment.
    if unsafe {
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
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    let process_handle = owned(process.hProcess)?;
    let thread_handle = owned(process.hThread)?;
    drop(thread_handle);
    drop(attributes);
    drop(stdin_read);
    drop(stdout_write);
    drop(stderr_write);
    let input = request.input.clone();
    let writer = thread::spawn(move || {
        let mut file = File::from(stdin_write);
        file.write_all(&input)
    });
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout = drain(
        stdout_read,
        request.capture_limit,
        overflow.clone(),
        callback.clone(),
        false,
    );
    let stderr = drain(
        stderr_read,
        request.capture_limit,
        overflow.clone(),
        callback,
        true,
    );
    let mut outcome = Outcome::Exited;
    loop {
        let wait = unsafe { WaitForSingleObject(process_handle.as_raw_handle(), 20) };
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
            } else if start.elapsed() >= request.deadline {
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
    let input_result = writer.join().map_err(|_| join_error())?;
    if outcome == Outcome::Exited && exit_code == 0 {
        input_result?;
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
        } else if mode == "overflow" {
            println!("{}", "x".repeat(65536));
        } else if mode == "environment" {
            println!("{}", std::env::var("SHOVEL_LITERAL_VALUE").unwrap());
        }
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

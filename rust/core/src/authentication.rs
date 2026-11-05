//! Per-operation current-user askpass pipe. The helper validates the server PID;
//! the broker validates a random nonce before any prompt reaches the UI.
//! Overlapped requests always drain before their buffers/handles are released.
use crate::process::Cancellation;
use rsa::rand_core::{OsRng, RngCore};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io,
    mem::{size_of, zeroed},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::Path,
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{IO::*, Pipes::*, Threading::*},
};
use zeroize::Zeroizing;

const PREFIX: &str = "ShovelRust.AskPass.";
pub const PIPE_VARIABLE: &str = "SHOVEL_RUST_ASKPASS_PIPE";
pub const TOKEN_VARIABLE: &str = "SHOVEL_RUST_ASKPASS_TOKEN";
pub const SERVER_VARIABLE: &str = "SHOVEL_RUST_ASKPASS_SERVER";
const MAX_PROMPT: usize = 16384;
const MAX_ANSWER: usize = 4096;
pub type Secret = Zeroizing<Vec<u8>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Secret,
    Confirmation,
    Information,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Unavailable,
    TimedOut,
    PromptLimit,
}
pub struct Prompt {
    pub kind: Kind,
    pub message: String,
    pub answer: Sender<Option<Secret>>,
    pub closed: Cancellation,
}
pub type PromptHandler = Arc<dyn Fn(Prompt) + Send + Sync>;
pub struct Session {
    pub environment: BTreeMap<OsString, Option<OsString>>,
    stop: Cancellation,
    worker: Option<thread::JoinHandle<()>>,
    failure: Arc<Mutex<Option<Failure>>>,
}
fn unavailable() -> io::Error {
    io::Error::other("authentication unavailable")
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
    }
}
struct Local(*mut std::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}
fn pipe(name: &str) -> io::Result<OwnedHandle> {
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = owned(token)?;
    let mut required = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut required,
        )
    };
    if required == 0 || required > 65536 {
        return Err(unavailable());
    }
    let mut buffer = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut sid = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let _sid_memory = Local(sid.cast());
    let mut length = 0;
    while length < 256 && unsafe { *sid.add(length) } != 0 {
        length += 1;
    }
    if length == 256 {
        return Err(unavailable());
    }
    let sid_text = String::from_utf16(unsafe { std::slice::from_raw_parts(sid, length) })
        .map_err(|_| unavailable())?;
    let sddl = wide(&format!("D:P(A;;GA;;;{sid_text})"));
    let mut descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let _descriptor = Local(descriptor);
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let full = wide(&format!("\\\\.\\pipe\\{name}"));
    owned(unsafe {
        CreateNamedPipeW(
            full.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            32768,
            32768,
            0,
            &attributes,
        )
    })
}
fn overlapped(
    handle: HANDLE,
    stop: &Cancellation,
    deadline: Option<Instant>,
    start: impl FnOnce(*mut OVERLAPPED) -> bool,
) -> io::Result<u32> {
    let event = owned(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) })?;
    let mut operation: OVERLAPPED = unsafe { zeroed() };
    operation.hEvent = event.as_raw_handle();
    let immediate = start(&mut operation);
    let error = if immediate {
        0
    } else {
        unsafe { GetLastError() }
    };
    if !immediate && error != ERROR_IO_PENDING {
        return Err(io::Error::from_raw_os_error(error as i32));
    }
    loop {
        if stop.is_cancelled() || deadline.is_some_and(|d| Instant::now() >= d) {
            unsafe { CancelIoEx(handle, &operation) };
            let mut count = 0;
            unsafe { GetOverlappedResult(handle, &operation, &mut count, 1) };
            return Err(io::ErrorKind::TimedOut.into());
        }
        let ready = unsafe { WaitForSingleObject(event.as_raw_handle(), 50) };
        if ready == WAIT_OBJECT_0 || immediate {
            let mut count = 0;
            if unsafe { GetOverlappedResult(handle, &operation, &mut count, 0) } == 0 {
                return Err(io::Error::last_os_error());
            }
            return Ok(count);
        }
        if ready != WAIT_TIMEOUT {
            unsafe { CancelIoEx(handle, &operation) };
            let mut count = 0;
            unsafe { GetOverlappedResult(handle, &operation, &mut count, 1) };
            return Err(unavailable());
        }
    }
}
fn connect(handle: HANDLE, stop: &Cancellation) -> io::Result<()> {
    let result = overlapped(handle, stop, None, |operation| unsafe {
        ConnectNamedPipe(handle, operation) != 0
    });
    match result {
        Err(e) if e.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) => Ok(()),
        other => other.map(|_| ()),
    }
}
fn read_exact(
    handle: HANDLE,
    bytes: &mut [u8],
    stop: &Cancellation,
    deadline: Instant,
) -> io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        let target = &mut bytes[offset..];
        let count = overlapped(handle, stop, Some(deadline), |operation| unsafe {
            ReadFile(
                handle,
                target.as_mut_ptr(),
                target.len() as u32,
                std::ptr::null_mut(),
                operation,
            ) != 0
        })? as usize;
        if count == 0 || count > target.len() {
            return Err(unavailable());
        }
        offset += count;
    }
    Ok(())
}
fn write_all(
    handle: HANDLE,
    bytes: &[u8],
    stop: &Cancellation,
    deadline: Instant,
) -> io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        let source = &bytes[offset..];
        let count = overlapped(handle, stop, Some(deadline), |operation| unsafe {
            WriteFile(
                handle,
                source.as_ptr(),
                source.len() as u32,
                std::ptr::null_mut(),
                operation,
            ) != 0
        })? as usize;
        if count == 0 || count > source.len() {
            return Err(unavailable());
        }
        offset += count;
    }
    Ok(())
}
fn read_frame(
    handle: HANDLE,
    max: usize,
    stop: &Cancellation,
    deadline: Instant,
) -> io::Result<Secret> {
    let mut header = [0u8; 4];
    read_exact(handle, &mut header, stop, deadline)?;
    let length = u32::from_le_bytes(header) as usize;
    if length > max {
        return Err(unavailable());
    }
    let mut bytes = Zeroizing::new(vec![0; length]);
    read_exact(handle, &mut bytes, stop, deadline)?;
    Ok(bytes)
}
fn write_frame(
    handle: HANDLE,
    bytes: &[u8],
    stop: &Cancellation,
    deadline: Instant,
) -> io::Result<()> {
    write_all(handle, &(bytes.len() as u32).to_le_bytes(), stop, deadline)?;
    write_all(handle, bytes, stop, deadline)
}
impl Session {
    pub fn start(helper: &Path, prompt: PromptHandler) -> io::Result<Self> {
        Self::start_with_timeout(helper, prompt, Duration::from_secs(120))
    }
    fn start_with_timeout(
        helper: &Path,
        prompt: PromptHandler,
        timeout: Duration,
    ) -> io::Result<Self> {
        if !helper.is_absolute() || !helper.is_file() {
            return Err(unavailable());
        }
        let name = format!("{PREFIX}{}", uuid::Uuid::new_v4().simple());
        let pipe = pipe(&name)?;
        let mut nonce = Zeroizing::new([0u8; 32]);
        OsRng.fill_bytes(&mut *nonce);
        let secret = nonce.iter().map(|b| format!("{b:02X}")).collect::<String>();
        let stop = Cancellation::default();
        let failure = Arc::new(Mutex::new(None));
        let environment = [
            ("GIT_ASKPASS", Some(helper.as_os_str().to_owned())),
            ("SSH_ASKPASS", Some(helper.as_os_str().to_owned())),
            ("SSH_ASKPASS_REQUIRE", Some("force".into())),
            ("SSH_ASKPASS_PROMPT", None),
            (PIPE_VARIABLE, Some(name.into())),
            (TOKEN_VARIABLE, Some(secret.into())),
            (SERVER_VARIABLE, Some(std::process::id().to_string().into())),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect();
        let worker_stop = stop.clone();
        let worker_failure = failure.clone();
        let worker = thread::spawn(move || {
            let result = serve(pipe, &nonce, prompt, &worker_stop, timeout, &worker_failure);
            if !worker_stop.is_cancelled()
                && let Err(error) = result
            {
                let mut failure = worker_failure.lock().unwrap();
                if failure.is_none() {
                    *failure = Some(if error.kind() == io::ErrorKind::TimedOut {
                        Failure::TimedOut
                    } else {
                        Failure::Unavailable
                    });
                }
            }
            worker_stop.cancel();
        });
        Ok(Self {
            environment,
            stop,
            worker: Some(worker),
            failure,
        })
    }
    pub fn failure(&self) -> Option<Failure> {
        *self.failure.lock().unwrap()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn serve(
    pipe: OwnedHandle,
    nonce: &[u8; 32],
    prompt: PromptHandler,
    stop: &Cancellation,
    timeout: Duration,
    failure: &Mutex<Option<Failure>>,
) -> io::Result<()> {
    let handle = pipe.as_raw_handle();
    for count in 0..=16 {
        connect(handle, stop)?;
        if count == 16 {
            *failure.lock().unwrap() = Some(Failure::PromptLimit);
            return Err(unavailable());
        }
        let deadline = Instant::now() + timeout;
        let received = read_frame(handle, 32, stop, deadline)?;
        if received.len() != 32 || !bool::from(received.as_slice().ct_eq(nonce)) {
            return Err(unavailable());
        }
        let request = read_frame(handle, MAX_PROMPT + 1, stop, deadline)?;
        let kind = match request.first() {
            Some(0) => Kind::Secret,
            Some(1) => Kind::Confirmation,
            Some(2) => Kind::Information,
            _ => return Err(unavailable()),
        };
        let message = String::from_utf8(request[1..].to_vec()).map_err(|_| unavailable())?;
        let (tx, rx) = mpsc::channel();
        let closed = Cancellation::default();
        prompt(Prompt {
            kind,
            message,
            answer: tx,
            closed: closed.clone(),
        });
        let result = (|| {
            loop {
                if stop.is_cancelled() || Instant::now() >= deadline {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                let mut available = 0;
                if unsafe {
                    PeekNamedPipe(
                        handle,
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut available,
                        std::ptr::null_mut(),
                    )
                } == 0
                {
                    if kind == Kind::Information {
                        return Ok(());
                    }
                    return Err(unavailable());
                }
                if available != 0 {
                    return Err(unavailable());
                }
                match rx.try_recv() {
                    Ok(answer) => {
                        if let Some(answer) = answer {
                            if answer.len() > MAX_ANSWER
                                || answer.iter().any(|b| matches!(b, 0 | 10 | 13))
                            {
                                return Err(unavailable());
                            }
                            write_all(handle, &[1], stop, deadline)?;
                            write_frame(handle, &answer, stop, deadline)?;
                        } else {
                            write_all(handle, &[0], stop, deadline)?;
                        }
                        let mut ack = [0];
                        read_exact(handle, &mut ack, stop, deadline)?;
                        if ack != [2] {
                            return Err(unavailable());
                        }
                        return Ok(());
                    }
                    Err(TryRecvError::Disconnected) => return Err(unavailable()),
                    Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(25)),
                }
            }
        })();
        closed.cancel();
        result?;
        unsafe { DisconnectNamedPipe(handle) };
    }
    Ok(())
}
pub fn client_from_environment(message: &str) -> io::Result<Secret> {
    let name = std::env::var(PIPE_VARIABLE).map_err(|_| unavailable())?;
    let token = Zeroizing::new(std::env::var(TOKEN_VARIABLE).map_err(|_| unavailable())?);
    let server = std::env::var(SERVER_VARIABLE)
        .map_err(|_| unavailable())?
        .parse::<u32>()
        .map_err(|_| unavailable())?;
    let kind = match std::env::var("SSH_ASKPASS_PROMPT").as_deref() {
        Ok("confirm") => Kind::Confirmation,
        Ok("none") => Kind::Information,
        _ => Kind::Secret,
    };
    client(&name, &token, server, message, kind)
}
fn client(name: &str, token: &str, server: u32, message: &str, kind: Kind) -> io::Result<Secret> {
    let suffix = name.strip_prefix(PREFIX).ok_or_else(unavailable)?;
    if suffix.len() != 32
        || !suffix.bytes().all(|b| b.is_ascii_hexdigit())
        || token.len() != 64
        || message.len() > MAX_PROMPT
    {
        return Err(unavailable());
    }
    let mut nonce = Zeroizing::new([0u8; 32]);
    for (i, chunk) in token.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        nonce[i] = u8::from_str_radix(std::str::from_utf8(chunk).map_err(|_| unavailable())?, 16)
            .map_err(|_| unavailable())?;
    }
    let full = wide(&format!("\\\\.\\pipe\\{name}"));
    let connection_deadline = Instant::now() + Duration::from_secs(5);
    let pipe = loop {
        let handle = unsafe {
            CreateFileW(
                full.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            break owned(handle)?;
        }
        let error = unsafe { GetLastError() };
        if !matches!(error, ERROR_PIPE_BUSY | ERROR_FILE_NOT_FOUND)
            || Instant::now() >= connection_deadline
        {
            return Err(unavailable());
        }
        thread::sleep(Duration::from_millis(25));
    };
    let handle = pipe.as_raw_handle();
    let mut actual = 0;
    if unsafe { GetNamedPipeServerProcessId(handle, &mut actual) } == 0 || actual != server {
        return Err(unavailable());
    }
    let stop = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(125);
    write_frame(handle, &*nonce, &stop, deadline)?;
    let mut request = vec![match kind {
        Kind::Secret => 0,
        Kind::Confirmation => 1,
        Kind::Information => 2,
    }];
    request.extend(message.as_bytes());
    write_frame(handle, &request, &stop, deadline)?;
    let mut status = [0];
    read_exact(handle, &mut status, &stop, deadline)?;
    if status[0] != 1 {
        write_all(handle, &[2], &stop, deadline)?;
        return Err(unavailable());
    }
    let answer = read_frame(handle, MAX_ANSWER, &stop, deadline)?;
    if answer.iter().any(|b| matches!(b, 0 | 10 | 13)) {
        return Err(unavailable());
    }
    write_all(handle, &[2], &stop, deadline)?;
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(
        session: &Session,
        message: &str,
        kind: Kind,
        server: u32,
        token: Option<&str>,
    ) -> io::Result<Secret> {
        let value = |key: &str| {
            session
                .environment
                .get(std::ffi::OsStr::new(key))
                .unwrap()
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        };
        client(
            &value(PIPE_VARIABLE),
            token.unwrap_or(&value(TOKEN_VARIABLE)),
            server,
            message,
            kind,
        )
    }
    #[test]
    fn secret_and_host_prompts_roundtrip_without_csharp_or_console_input() {
        let observed = Arc::new(Mutex::new(vec![]));
        let seen = observed.clone();
        let session = Session::start(
            &std::env::current_exe().unwrap(),
            Arc::new(move |prompt| {
                seen.lock().unwrap().push(prompt.kind);
                let bytes = if prompt.kind == Kind::Confirmation {
                    b"yes".to_vec()
                } else {
                    "שלום secret".as_bytes().to_vec()
                };
                prompt.answer.send(Some(Zeroizing::new(bytes))).unwrap();
            }),
        )
        .unwrap();
        assert_eq!(
            &*call(
                &session,
                "Password:",
                Kind::Secret,
                std::process::id(),
                None
            )
            .unwrap(),
            "שלום secret".as_bytes()
        );
        assert_eq!(
            &*call(
                &session,
                "Host confirmation",
                Kind::Confirmation,
                std::process::id(),
                None
            )
            .unwrap(),
            b"yes"
        );
        assert_eq!(
            *observed.lock().unwrap(),
            vec![Kind::Secret, Kind::Confirmation]
        );
    }
    #[test]
    fn wrong_nonce_and_server_pid_never_reach_the_prompt() {
        for wrong_server in [false, true] {
            let session = Session::start(
                &std::env::current_exe().unwrap(),
                Arc::new(|_| panic!("unauthenticated prompt")),
            )
            .unwrap();
            let result = call(
                &session,
                "must not display",
                Kind::Secret,
                if wrong_server {
                    std::process::id() + 1
                } else {
                    std::process::id()
                },
                if wrong_server {
                    None
                } else {
                    Some("0000000000000000000000000000000000000000000000000000000000000000")
                },
            );
            assert!(result.is_err());
        }
    }
    #[test]
    fn cancelled_invalid_and_expired_answers_fail_without_hanging() {
        for mode in [0, 1, 2] {
            let held = Arc::new(Mutex::new(None));
            let hold = held.clone();
            let session = Session::start_with_timeout(
                &std::env::current_exe().unwrap(),
                Arc::new(move |prompt| match mode {
                    0 => {
                        prompt.answer.send(None).unwrap();
                    }
                    1 => {
                        prompt
                            .answer
                            .send(Some(Zeroizing::new(b"bad\nanswer".to_vec())))
                            .unwrap();
                    }
                    _ => {
                        *hold.lock().unwrap() = Some(prompt);
                    }
                }),
                Duration::from_millis(120),
            )
            .unwrap();
            let start = Instant::now();
            assert!(call(&session, "fixture", Kind::Secret, std::process::id(), None).is_err());
            assert!(start.elapsed() < Duration::from_secs(3));
            if let Some(prompt) = held.lock().unwrap().as_ref() {
                assert!(prompt.closed.is_cancelled());
            }
        }
    }
}

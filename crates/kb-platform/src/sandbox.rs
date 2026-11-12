//! Module host launcher (architecture §3.2, task 1.10).
//!
//! Windows: the child runs with a restricted primary token (all privileges removed, LOW
//! integrity level) inside a Job Object (kill-on-close, process count limit, optional memory
//! limit, die on unhandled exception). Low integrity means it cannot write to the user's
//! files, registry or medium-integrity objects.
//! Not yet: AppContainer (no network / no file access by capability) — follow-up item.
//! Linux / macOS: plain child process until task 1.10b (namespaces + seccomp + Landlock).

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct SpawnSpec {
    pub exe: PathBuf,
    pub args: Vec<OsString>,
    /// Extra environment variables (the rest is inherited).
    pub env: Vec<(String, String)>,
    /// Per-process committed memory limit in bytes.
    pub memory_limit: Option<usize>,
    /// Maximum number of processes in the job, including the host itself (≥ 1).
    pub max_processes: u32,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, HANDLE, LocalFree, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSidToSidW,
    };
    use windows_sys::Win32::Security::{
        CreateRestrictedToken, DISABLE_MAX_PRIVILEGE, GetLengthSid, GetTokenInformation,
        SID_AND_ATTRIBUTES, SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_ASSIGN_PRIMARY,
        TOKEN_DUPLICATE, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TOKEN_USER, TokenIntegrityLevel,
        TokenUser,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
        JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::SystemServices::SE_GROUP_INTEGRITY;
    use windows_sys::Win32::System::Threading::{
        CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW,
        GetCurrentProcess, GetExitCodeProcess, INFINITE, OpenProcessToken, PROCESS_INFORMATION,
        ResumeThread, STARTUPINFOW, WaitForSingleObject,
    };

    /// Owned Win32 handle.
    struct Handle(HANDLE);
    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    fn last_error(what: &str) -> io::Error {
        let code = unsafe { GetLastError() };
        io::Error::other(format!(
            "{what} failed: {}",
            io::Error::from_raw_os_error(code as i32)
        ))
    }

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Current user's SID as a string (`S-1-5-21-…`).
    pub fn current_user_sid() -> io::Result<String> {
        unsafe {
            let mut token: HANDLE = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(last_error("OpenProcessToken"));
            }
            let token = Handle(token);
            let mut len = 0u32;
            GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut len);
            let mut buf = vec![0u8; len as usize];
            if GetTokenInformation(token.0, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) == 0
            {
                return Err(last_error("GetTokenInformation"));
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s: *mut u16 = null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut s) == 0 {
                return Err(last_error("ConvertSidToStringSidW"));
            }
            let len = (0..).take_while(|&i| *s.add(i) != 0).count();
            let out = String::from_utf16_lossy(std::slice::from_raw_parts(s, len));
            LocalFree(s.cast());
            Ok(out)
        }
    }

    /// Restricted primary token: no privileges, LOW integrity.
    fn restricted_low_token() -> io::Result<Handle> {
        unsafe {
            let mut own: HANDLE = null_mut();
            let access =
                TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ASSIGN_PRIMARY | TOKEN_ADJUST_DEFAULT;
            if OpenProcessToken(GetCurrentProcess(), access, &mut own) == 0 {
                return Err(last_error("OpenProcessToken"));
            }
            let own = Handle(own);
            let mut restricted: HANDLE = null_mut();
            if CreateRestrictedToken(
                own.0,
                DISABLE_MAX_PRIVILEGE,
                0,
                null(),
                0,
                null(),
                0,
                null(),
                &mut restricted,
            ) == 0
            {
                return Err(last_error("CreateRestrictedToken"));
            }
            let restricted = Handle(restricted);

            let mut low_sid = null_mut();
            let low = wide(std::ffi::OsStr::new("S-1-16-4096"));
            if ConvertStringSidToSidW(low.as_ptr(), &mut low_sid) == 0 {
                return Err(last_error("ConvertStringSidToSidW"));
            }
            let label = TOKEN_MANDATORY_LABEL {
                Label: SID_AND_ATTRIBUTES {
                    Sid: low_sid,
                    Attributes: SE_GROUP_INTEGRITY as u32,
                },
            };
            let size = std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32 + GetLengthSid(low_sid);
            let ok = SetTokenInformation(
                restricted.0,
                TokenIntegrityLevel,
                (&label as *const TOKEN_MANDATORY_LABEL).cast(),
                size,
            );
            LocalFree(low_sid);
            if ok == 0 {
                return Err(last_error("SetTokenInformation(IntegrityLevel)"));
            }
            Ok(restricted)
        }
    }

    fn job(spec: &SpawnSpec) -> io::Result<Handle> {
        unsafe {
            let job = CreateJobObjectW(null(), null());
            if job.is_null() {
                return Err(last_error("CreateJobObjectW"));
            }
            let job = Handle(job);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
            info.BasicLimitInformation.ActiveProcessLimit = spec.max_processes.max(1);
            if let Some(limit) = spec.memory_limit {
                info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY;
                info.ProcessMemoryLimit = limit;
            }
            if SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                return Err(last_error("SetInformationJobObject"));
            }
            Ok(job)
        }
    }

    /// Windows command line quoting (CommandLineToArgvW rules).
    pub fn quote_arg(arg: &str) -> String {
        if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
            return arg.to_string();
        }
        let mut out = String::from("\"");
        let mut backslashes = 0;
        for c in arg.chars() {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    out.push_str(&"\\".repeat(backslashes * 2 + 1));
                    out.push('"');
                    backslashes = 0;
                }
                _ => {
                    out.push_str(&"\\".repeat(backslashes));
                    out.push(c);
                    backslashes = 0;
                }
            }
        }
        out.push_str(&"\\".repeat(backslashes * 2));
        out.push('"');
        out
    }

    fn env_block(extra: &[(String, String)]) -> Vec<u16> {
        let mut vars: Vec<(String, String)> = std::env::vars()
            .filter(|(k, _)| !extra.iter().any(|(e, _)| e.eq_ignore_ascii_case(k)))
            .collect();
        vars.extend(extra.iter().cloned());
        vars.sort_by_key(|(k, _)| k.to_uppercase());
        let mut block = Vec::new();
        for (k, v) in vars {
            block.extend(format!("{k}={v}").encode_utf16());
            block.push(0);
        }
        block.push(0);
        block
    }

    pub struct Child {
        process: Handle,
        job: Handle,
        pub pid: u32,
    }

    pub fn spawn(spec: &SpawnSpec) -> io::Result<Child> {
        let token = restricted_low_token()?;
        let job = job(spec)?;
        let mut cmdline = quote_arg(&spec.exe.to_string_lossy());
        for a in &spec.args {
            cmdline.push(' ');
            cmdline.push_str(&quote_arg(&a.to_string_lossy()));
        }
        let mut cmdline: Vec<u16> = cmdline.encode_utf16().chain(std::iter::once(0)).collect();
        let app = wide(spec.exe.as_os_str());
        let env = env_block(&spec.env);
        unsafe {
            let mut si: STARTUPINFOW = std::mem::zeroed();
            si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
            if CreateProcessAsUserW(
                token.0,
                app.as_ptr(),
                cmdline.as_mut_ptr(),
                null(),
                null(),
                0,
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
                env.as_ptr().cast(),
                null(),
                &si,
                &mut pi,
            ) == 0
            {
                return Err(last_error("CreateProcessAsUserW"));
            }
            let process = Handle(pi.hProcess);
            let thread = Handle(pi.hThread);
            if AssignProcessToJobObject(job.0, process.0) == 0 {
                let e = last_error("AssignProcessToJobObject");
                TerminateJobObject(job.0, 1);
                windows_sys::Win32::System::Threading::TerminateProcess(process.0, 1);
                return Err(e);
            }
            ResumeThread(thread.0);
            Ok(Child {
                process,
                job,
                pid: pi.dwProcessId,
            })
        }
    }

    impl Child {
        /// Blocks until the process exits; returns its exit code.
        pub fn wait(&self) -> io::Result<u32> {
            unsafe {
                if WaitForSingleObject(self.process.0, INFINITE) != WAIT_OBJECT_0 {
                    return Err(last_error("WaitForSingleObject"));
                }
                let mut code = 0u32;
                if GetExitCodeProcess(self.process.0, &mut code) == 0 {
                    return Err(last_error("GetExitCodeProcess"));
                }
                Ok(code)
            }
        }

        /// Kills every process in the job.
        pub fn kill(&self) -> io::Result<()> {
            if unsafe { TerminateJobObject(self.job.0, 1) } == 0 {
                return Err(last_error("TerminateJobObject"));
            }
            Ok(())
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    use std::process::Command;
    use std::sync::Mutex;

    pub struct Child {
        child: Mutex<std::process::Child>,
        pub pid: u32,
    }

    /// Unsandboxed until task 1.10b.
    pub fn spawn(spec: &SpawnSpec) -> io::Result<Child> {
        let child = Command::new(&spec.exe)
            .args(&spec.args)
            .envs(spec.env.iter().cloned())
            .spawn()?;
        let pid = child.id();
        Ok(Child {
            child: Mutex::new(child),
            pid,
        })
    }

    impl Child {
        pub fn wait(&self) -> io::Result<u32> {
            let status = self
                .child
                .lock()
                .map_err(|_| io::Error::other("poisoned"))?
                .wait()?;
            Ok(status.code().unwrap_or(-1) as u32)
        }

        pub fn kill(&self) -> io::Result<()> {
            self.child
                .lock()
                .map_err(|_| io::Error::other("poisoned"))?
                .kill()
        }
    }
}

pub use imp::{Child, spawn};
#[cfg(windows)]
pub use imp::{current_user_sid, quote_arg};

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(quote_arg("abc"), "abc");
        assert_eq!(quote_arg("a b"), "\"a b\"");
        assert_eq!(quote_arg(""), "\"\"");
        assert_eq!(quote_arg(r#"a"b"#), r#""a\"b""#);
        assert_eq!(quote_arg(r"C:\dir with space\"), r#""C:\dir with space\\""#);
    }

    #[test]
    fn user_sid_looks_like_a_sid() {
        // Local accounts are S-1-5-21-…, Microsoft / Entra ID accounts S-1-12-1-….
        assert!(current_user_sid().unwrap().starts_with("S-1-"));
    }
}

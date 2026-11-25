// SSH keys — Windows side (issue 054): owner-only key files and the
// OpenSSH for Windows agent named pipe.
//
// Windows OpenSSH refuses a private key file other accounts can read
// ("UNPROTECTED PRIVATE KEY FILE"), so an exported key gets a protected DACL
// granting the current user only. The file is created with no sharing and
// the DACL is set through that handle before any key byte is written: no
// other handle can be opened on it in between (access is checked at open).
//
// Each agent request opens the pipe, writes one frame and reads one answer
// on a worker thread; the caller waits at most AGENT_TIMEOUT so a stuck
// agent cannot freeze the UI (a timed-out worker is left to finish alone).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use zeroize::Zeroizing;

use super::{
    decode_identities, decode_status, encode_add_identity, encode_remove_identity,
    encode_request_identities, read_frame, Agent, SshKeyError,
};

/// Named pipe of the "OpenSSH Authentication Agent" Windows service.
pub const AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

/// Longest wait for one agent request.
const AGENT_TIMEOUT: Duration = Duration::from_secs(5);

fn io_err(context: &str, e: std::io::Error) -> SshKeyError {
    SshKeyError::Io(format!("{}: {}", context, e))
}

/// Send one frame to the agent and return the answer body.
fn transact(frame: Zeroizing<Vec<u8>>) -> Result<Vec<u8>, SshKeyError> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| {
            let mut pipe = OpenOptions::new()
                .read(true)
                .write(true)
                .open(AGENT_PIPE)
                .map_err(|e| match e.kind() {
                    std::io::ErrorKind::NotFound => SshKeyError::AgentUnavailable,
                    _ => io_err("ssh-agent pipe", e),
                })?;
            pipe.write_all(&frame).map_err(|e| io_err("ssh-agent write", e))?;
            read_frame(&mut pipe)
        })();
        let _ = tx.send(result);
    });
    match rx.recv_timeout(AGENT_TIMEOUT) {
        Ok(result) => result,
        Err(_) => Err(SshKeyError::AgentTimeout),
    }
}

/// The Windows OpenSSH agent, reached through its named pipe.
pub struct PipeAgent;

impl PipeAgent {
    /// Load a private key (decrypted with `passphrase` when needed).
    pub fn add(&mut self, private_openssh: &str, passphrase: &str) -> Result<(), SshKeyError> {
        let frame = encode_add_identity(private_openssh, passphrase)?;
        decode_status(&transact(frame)?)
    }
}

impl Agent for PipeAgent {
    fn identities(&mut self) -> Result<Vec<String>, SshKeyError> {
        decode_identities(&transact(Zeroizing::new(encode_request_identities()))?)
    }

    fn remove(&mut self, public_line: &str) -> Result<(), SshKeyError> {
        let frame = encode_remove_identity(public_line)?;
        decode_status(&transact(Zeroizing::new(frame))?)
    }
}

/// `<path>.pub`, next to the private key file.
pub fn public_path(private_path: &Path) -> PathBuf {
    let mut s = private_path.as_os_str().to_owned();
    s.push(".pub");
    PathBuf::from(s)
}

/// Write the key pair to `private_path` (owner-only DACL, set while the file
/// is still empty) and `<private_path>.pub`. Never overwrites: an existing
/// file is an error. On failure, files created by this call are removed.
pub fn export_key_pair(private_path: &Path, private_openssh: &str, public_line: &str) -> Result<(), SshKeyError> {
    let pub_path = public_path(private_path);
    for p in [private_path, pub_path.as_path()] {
        if p.exists() {
            return Err(SshKeyError::Io(format!("{} already exists", p.display())));
        }
    }

    let write_private = || -> Result<(), SshKeyError> {
        use windows::Win32::Storage::FileSystem::{FILE_GENERIC_WRITE, READ_CONTROL, WRITE_DAC};
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .access_mode((FILE_GENERIC_WRITE | WRITE_DAC | READ_CONTROL).0)
            .open(private_path)
            .map_err(|e| io_err("create private key file", e))?;
        restrict_to_current_user(&file)?;
        let trimmed = private_openssh.trim_end();
        // Sized up front: growing it would free an unwiped copy of the key.
        let mut content = Zeroizing::new(String::with_capacity(trimmed.len() + 1));
        content.push_str(trimmed);
        content.push('\n');
        file.write_all(content.as_bytes()).map_err(|e| io_err("write private key file", e))?;
        file.sync_all().map_err(|e| io_err("flush private key file", e))
    };
    if let Err(e) = write_private() {
        let _ = std::fs::remove_file(private_path);
        return Err(e);
    }

    let write_public = || -> Result<(), SshKeyError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pub_path)
            .map_err(|e| io_err("create public key file", e))?;
        file.write_all(format!("{}\n", public_line.trim_end()).as_bytes())
            .map_err(|e| io_err("write public key file", e))
    };
    if let Err(e) = write_public() {
        let _ = std::fs::remove_file(&pub_path);
        let _ = std::fs::remove_file(private_path);
        return Err(e);
    }
    Ok(())
}

/// Replace the file's DACL with a protected one (no inherited entries)
/// granting full access to the current user only. `file` must be open with
/// WRITE_DAC.
fn restrict_to_current_user(file: &File) -> Result<(), SshKeyError> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, LocalFree, ERROR_SUCCESS, HANDLE, HLOCAL};
    use windows::Win32::Security::Authorization::{
        SetEntriesInAclW, SetSecurityInfo, EXPLICIT_ACCESS_W, SET_ACCESS, SE_FILE_OBJECT,
        TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
    };
    use windows::Win32::Security::{
        GetTokenInformation, TokenUser, ACL, DACL_SECURITY_INFORMATION, NO_INHERITANCE,
        PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
    };
    use windows::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let win_err = |what: &str, code: u32| SshKeyError::Io(format!("{} failed (error {})", what, code));

    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|e| SshKeyError::Io(format!("OpenProcessToken: {}", e)))?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
        // u64 buffer: TOKEN_USER holds a pointer and needs its alignment.
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        let info = GetTokenInformation(token, TokenUser, Some(buf.as_mut_ptr().cast()), len, &mut len);
        let _ = CloseHandle(token);
        info.map_err(|e| SshKeyError::Io(format!("GetTokenInformation: {}", e)))?;
        let user = &*(buf.as_ptr() as *const TOKEN_USER);

        let access = EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS.0,
            grfAccessMode: SET_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: TRUSTEE_W {
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_USER,
                ptstrName: PWSTR(user.User.Sid.0.cast()),
                ..Default::default()
            },
        };
        let mut acl: *mut ACL = std::ptr::null_mut();
        let rc = SetEntriesInAclW(Some(&[access]), None, &mut acl);
        if rc != ERROR_SUCCESS {
            return Err(win_err("SetEntriesInAclW", rc.0));
        }
        let rc = SetSecurityInfo(
            HANDLE(file.as_raw_handle()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(acl),
            None,
        );
        let _ = LocalFree(Some(HLOCAL(acl.cast())));
        if rc != ERROR_SUCCESS {
            return Err(win_err("SetSecurityInfo", rc.0));
        }
    }
    Ok(())
}

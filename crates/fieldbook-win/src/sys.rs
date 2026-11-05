#![expect(unsafe_code, reason = "Windows system calls")]

use std::ffi::{OsStr, OsString, c_void};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf, Prefix};
use std::ptr;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_CLOUD_FILE_NOT_UNDER_SYNC_ROOT, ERROR_INSUFFICIENT_BUFFER,
    ERROR_NOT_A_CLOUD_FILE, ERROR_NOT_FOUND, GENERIC_ALL, GENERIC_WRITE, GetLastError, HANDLE,
    LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSidToSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, CheckTokenMembership, DACL_SECURITY_INFORMATION, GetAce,
    GetTokenInformation, IsValidSid, IsWellKnownSid, LookupAccountNameW, LookupAccountSidW,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, TOKEN_QUERY, TOKEN_USER, TokenUser,
    WinBuiltinAdministratorsSid, WinLocalSystemSid,
};
use windows_sys::Win32::Storage::CloudFilters::{
    CF_SYNC_ROOT_BASIC_INFO, CF_SYNC_ROOT_INFO_BASIC, CfGetSyncRootInfoByPath,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_APPEND_DATA, FILE_ATTRIBUTE_NOT_CONTENT_INDEXED, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_WRITE_DATA, GetDriveTypeW, GetFileAttributesW, GetVolumePathNameW,
    INVALID_FILE_ATTRIBUTES, SetFileAttributesW, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::RemoteDesktop::{
    WTS_CURRENT_SERVER_HANDLE, WTS_SESSION_INFOW, WTSActive, WTSDisconnected,
    WTSEnumerateSessionsW, WTSFreeMemory, WTSQuerySessionInformationW, WTSUserName,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_ProgramData, SHGetKnownFolderPath,
};

use crate::{Folder, Place, WinError, WriterKind};

const DRIVE_REMOTE_CODE: u32 = 4;
const ACCESS_ALLOWED_ACE_TYPE_CODE: u8 = 0;
const ACCESS_ALLOWED_CALLBACK_ACE_TYPE_CODE: u8 = 9;
const ACCESS_DENIED_ACE_TYPE_CODE: u8 = 1;
const ACCESS_DENIED_OBJECT_ACE_TYPE_CODE: u8 = 6;
const ACCESS_DENIED_CALLBACK_ACE_TYPE_CODE: u8 = 10;
const ACCESS_DENIED_CALLBACK_OBJECT_ACE_TYPE_CODE: u8 = 12;
const WRITE_RIGHTS_MASK: u32 = FILE_WRITE_DATA
    | FILE_APPEND_DATA
    | DELETE
    | WRITE_DAC
    | WRITE_OWNER
    | GENERIC_WRITE
    | GENERIC_ALL;

fn to_wide_null(s: &OsStr) -> Vec<u16> {
    let mut wide: Vec<u16> = s.encode_wide().collect();
    wide.push(0);
    wide
}

fn last_error(call: &'static str) -> WinError {
    // SAFETY: `GetLastError` reads the calling thread's last-error slot and takes no pointers.
    let code = unsafe { GetLastError() };
    WinError { call, code }
}

pub(crate) fn known_folder(folder: Folder) -> Result<PathBuf, WinError> {
    let rfid = match folder {
        Folder::ProgramData => FOLDERID_ProgramData,
        Folder::LocalAppData => FOLDERID_LocalAppData,
    };
    let mut raw_path: *mut u16 = ptr::null_mut();
    // SAFETY: `rfid` is a valid `GUID`, `hToken` is null (current user), and `raw_path`
    // receives the CoTaskMem-allocated NUL-terminated wide string on `S_OK`.
    let hr =
        unsafe { SHGetKnownFolderPath(&raw const rfid, 0, ptr::null_mut(), &raw mut raw_path) };
    if hr < 0 || raw_path.is_null() {
        if !raw_path.is_null() {
            // SAFETY: `raw_path` was allocated by `SHGetKnownFolderPath` using the COM task allocator.
            unsafe { CoTaskMemFree(raw_path.cast::<c_void>()) };
        }
        return Err(WinError {
            call: "SHGetKnownFolderPath",
            code: hr.cast_unsigned(),
        });
    }
    let mut len = 0usize;
    // SAFETY: `raw_path` is a non-null NUL-terminated UTF-16 buffer allocated by `SHGetKnownFolderPath`.
    while unsafe { *raw_path.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `raw_path` is valid for `len` `u16` elements up to the NUL terminator.
    let slice = unsafe { std::slice::from_raw_parts(raw_path, len) };
    let os = OsString::from_wide(slice);
    // SAFETY: `raw_path` was allocated by `SHGetKnownFolderPath` and is not used after freeing.
    unsafe { CoTaskMemFree(raw_path.cast::<c_void>()) };
    Ok(PathBuf::from(os))
}

pub(crate) fn principal() -> Result<String, WinError> {
    let mut token: HANDLE = ptr::null_mut();
    // SAFETY: `GetCurrentProcess` returns the pseudo-handle for the current process, and `token`
    // is a valid out-pointer for the opened process token handle.
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) };
    if opened == 0 {
        return Err(last_error("OpenProcessToken"));
    }

    let mut needed: u32 = 0;
    // SAFETY: Passing a null buffer and 0 length queries the required byte size in `needed`.
    let _ = unsafe { GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &raw mut needed) };
    if needed == 0 {
        let err = last_error("GetTokenInformation");
        // SAFETY: `token` is a valid open handle from `OpenProcessToken`.
        unsafe {
            let _ = CloseHandle(token);
        }
        return Err(err);
    }

    let mut buf = vec![0u8; needed as usize];
    // SAFETY: `buf` is allocated with `needed` bytes and writable for `GetTokenInformation`.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buf.as_mut_ptr().cast::<c_void>(),
            needed,
            &raw mut needed,
        )
    };
    // SAFETY: `token` is a valid open handle from `OpenProcessToken` and is no longer needed.
    unsafe {
        let _ = CloseHandle(token);
    }
    if ok == 0 {
        return Err(last_error("GetTokenInformation"));
    }

    // SAFETY: `GetTokenInformation` succeeded for `TokenUser`, so `buf` starts with a valid `TOKEN_USER`.
    let user_sid: PSID = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut name_len: u32 = 0;
    let mut domain_len: u32 = 0;
    let mut use_kind: i32 = 0;
    // SAFETY: Passing null buffers with 0 lengths queries the required character counts (including NUL).
    let _ = unsafe {
        LookupAccountSidW(
            ptr::null(),
            user_sid,
            ptr::null_mut(),
            &raw mut name_len,
            ptr::null_mut(),
            &raw mut domain_len,
            &raw mut use_kind,
        )
    };
    if name_len == 0 {
        return Err(last_error("LookupAccountSidW"));
    }

    let mut name_buf = vec![0u16; name_len as usize];
    let mut domain_buf = vec![0u16; domain_len.max(1) as usize];
    // SAFETY: `name_buf` and `domain_buf` have capacities `name_len` and `domain_len` wide chars.
    let looked_up = unsafe {
        LookupAccountSidW(
            ptr::null(),
            user_sid,
            name_buf.as_mut_ptr(),
            &raw mut name_len,
            domain_buf.as_mut_ptr(),
            &raw mut domain_len,
            &raw mut use_kind,
        )
    };
    if looked_up == 0 {
        return Err(last_error("LookupAccountSidW"));
    }

    let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
    let domain = String::from_utf16_lossy(&domain_buf[..domain_len as usize]);
    if domain.is_empty() {
        Ok(name)
    } else {
        Ok(format!("{domain}\\{name}"))
    }
}

pub(crate) fn in_group(group: &str) -> Result<bool, WinError> {
    if group.is_empty() {
        return Err(WinError {
            call: "LookupAccountNameW",
            code: ERROR_NOT_FOUND,
        });
    }
    let wide = to_wide_null(OsStr::new(group));
    let looks_like_sid = group.starts_with("S-") || group.starts_with("s-");
    if looks_like_sid {
        let mut sid: PSID = ptr::null_mut();
        // SAFETY: `wide` is a valid NUL-terminated UTF-16 string and `sid` receives the LocalAlloc'd SID.
        let converted = unsafe { ConvertStringSidToSidW(wide.as_ptr(), &raw mut sid) };
        if converted == 0 || sid.is_null() {
            return Err(last_error("ConvertStringSidToSidW"));
        }
        let mut is_member: i32 = 0;
        // SAFETY: `sid` is a valid SID from `ConvertStringSidToSidW`, and `TokenHandle` is null.
        let checked = unsafe { CheckTokenMembership(ptr::null_mut(), sid, &raw mut is_member) };
        let err = if checked == 0 {
            Some(last_error("CheckTokenMembership"))
        } else {
            None
        };
        // SAFETY: `sid` was allocated by `ConvertStringSidToSidW` and must be freed with `LocalFree`.
        unsafe {
            let _ = LocalFree(sid.cast::<c_void>());
        }
        if let Some(e) = err {
            return Err(e);
        }
        return Ok(is_member != 0);
    }

    let mut sid_size: u32 = 0;
    let mut domain_len: u32 = 0;
    let mut use_kind: i32 = 0;
    // SAFETY: Passing null buffers with 0 lengths queries required byte and character sizes.
    let _ = unsafe {
        LookupAccountNameW(
            ptr::null(),
            wide.as_ptr(),
            ptr::null_mut(),
            &raw mut sid_size,
            ptr::null_mut(),
            &raw mut domain_len,
            &raw mut use_kind,
        )
    };
    if sid_size == 0 {
        return Err(last_error("LookupAccountNameW"));
    }

    let mut sid_buf = vec![0u8; sid_size as usize];
    let mut domain_buf = vec![0u16; domain_len.max(1) as usize];
    // SAFETY: `sid_buf` has `sid_size` bytes and `domain_buf` has `domain_len` wide chars.
    let resolved = unsafe {
        LookupAccountNameW(
            ptr::null(),
            wide.as_ptr(),
            sid_buf.as_mut_ptr().cast::<c_void>(),
            &raw mut sid_size,
            domain_buf.as_mut_ptr(),
            &raw mut domain_len,
            &raw mut use_kind,
        )
    };
    if resolved == 0 {
        return Err(last_error("LookupAccountNameW"));
    }

    let mut is_member: i32 = 0;
    // SAFETY: `sid_buf` holds a valid SID populated by `LookupAccountNameW`.
    let checked = unsafe {
        CheckTokenMembership(
            ptr::null_mut(),
            sid_buf.as_mut_ptr().cast::<c_void>(),
            &raw mut is_member,
        )
    };
    if checked == 0 {
        return Err(last_error("CheckTokenMembership"));
    }
    Ok(is_member != 0)
}

pub(crate) fn interactive_logons() -> Result<u32, WinError> {
    let mut sessions_ptr: *mut WTS_SESSION_INFOW = ptr::null_mut();
    let mut count: u32 = 0;
    // SAFETY: `WTS_CURRENT_SERVER_HANDLE` queries the local machine, and `sessions_ptr` / `count`
    // are valid out-pointers for the session array allocated by WTS.
    let ok = unsafe {
        WTSEnumerateSessionsW(
            WTS_CURRENT_SERVER_HANDLE,
            0,
            1,
            &raw mut sessions_ptr,
            &raw mut count,
        )
    };
    if ok == 0 {
        return Err(last_error("WTSEnumerateSessionsW"));
    }

    let mut interactive: u32 = 0;
    if !sessions_ptr.is_null() && count > 0 {
        // SAFETY: `WTSEnumerateSessionsW` succeeded and returned `count` `WTS_SESSION_INFOW` items.
        let sessions = unsafe { std::slice::from_raw_parts(sessions_ptr, count as usize) };
        for sess in sessions {
            if sess.State != WTSActive && sess.State != WTSDisconnected {
                continue;
            }
            let mut buffer: *mut u16 = ptr::null_mut();
            let mut bytes_returned: u32 = 0;
            // SAFETY: Queries `WTSUserName` for `sess.SessionId`; `buffer` is allocated by WTS on success.
            let queried = unsafe {
                WTSQuerySessionInformationW(
                    WTS_CURRENT_SERVER_HANDLE,
                    sess.SessionId,
                    WTSUserName,
                    &raw mut buffer,
                    &raw mut bytes_returned,
                )
            };
            if queried != 0 && !buffer.is_null() {
                // SAFETY: When `WTSQuerySessionInformationW` succeeds for `WTSUserName`, `buffer`
                // points to a NUL-terminated UTF-16 string of at least 1 `u16`.
                let has_user = bytes_returned >= 2 && unsafe { *buffer } != 0;
                if has_user {
                    interactive = interactive.saturating_add(1);
                }
                // SAFETY: `buffer` was allocated by `WTSQuerySessionInformationW`.
                unsafe { WTSFreeMemory(buffer.cast::<c_void>()) };
            }
        }
    }
    if !sessions_ptr.is_null() {
        // SAFETY: `sessions_ptr` was allocated by `WTSEnumerateSessionsW`.
        unsafe { WTSFreeMemory(sessions_ptr.cast::<c_void>()) };
    }
    Ok(interactive)
}

fn nearest_existing_ancestor(path: &Path) -> Result<PathBuf, WinError> {
    let mut cur = Some(path);
    while let Some(candidate) = cur {
        let wide = to_wide_null(candidate.as_os_str());
        // SAFETY: `wide` is a valid NUL-terminated UTF-16 path string.
        let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
        if attrs != INVALID_FILE_ATTRIBUTES {
            return Ok(candidate.to_path_buf());
        }
        cur = candidate.parent().filter(|p| !p.as_os_str().is_empty());
    }
    Err(last_error("GetFileAttributesW"))
}

fn check_reparse_chain(existing: &Path) -> Result<bool, WinError> {
    let mut cur = Some(existing);
    while let Some(folder) = cur {
        let wide = to_wide_null(folder.as_os_str());
        // SAFETY: `wide` is a valid NUL-terminated UTF-16 path string.
        let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
        if attrs == INVALID_FILE_ATTRIBUTES {
            return Err(last_error("GetFileAttributesW"));
        }
        if (attrs & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
            return Ok(true);
        }
        cur = folder.parent().filter(|p| !p.as_os_str().is_empty());
    }
    Ok(false)
}

fn check_sync_root(existing: &Path) -> Result<bool, WinError> {
    let wide = to_wide_null(existing.as_os_str());
    let mut info = CF_SYNC_ROOT_BASIC_INFO { SyncRootFileId: 0 };
    let mut returned: u32 = 0;
    let info_size = u32::try_from(std::mem::size_of::<CF_SYNC_ROOT_BASIC_INFO>()).unwrap_or(8);
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 path and `info` is writable for `info_size` bytes.
    let hr = unsafe {
        CfGetSyncRootInfoByPath(
            wide.as_ptr(),
            CF_SYNC_ROOT_INFO_BASIC,
            (&raw mut info).cast::<c_void>(),
            info_size,
            &raw mut returned,
        )
    };
    if hr >= 0 {
        return Ok(true);
    }
    let code = hr.cast_unsigned();
    let low_win32 = code & 0xFFFF;
    if low_win32 == ERROR_CLOUD_FILE_NOT_UNDER_SYNC_ROOT || low_win32 == ERROR_NOT_A_CLOUD_FILE {
        Ok(false)
    } else {
        Err(WinError {
            call: "CfGetSyncRootInfoByPath",
            code,
        })
    }
}

fn check_network(path: &Path, existing: Option<&Path>) -> Result<bool, WinError> {
    if let Some(Component::Prefix(prefix_component)) = path.components().next() {
        match prefix_component.kind() {
            Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _) => return Ok(true),
            Prefix::Disk(drive_byte) | Prefix::VerbatimDisk(drive_byte) => {
                let root = format!("{}:\\", char::from(drive_byte));
                let wide = to_wide_null(OsStr::new(&root));
                // SAFETY: `wide` is a NUL-terminated drive root (`X:\`).
                let drive_type = unsafe { GetDriveTypeW(wide.as_ptr()) };
                return Ok(drive_type == DRIVE_REMOTE_CODE);
            }
            Prefix::Verbatim(_) | Prefix::DeviceNS(_) => {}
        }
    }

    let query_path = existing.unwrap_or(path);
    let wide_path = to_wide_null(query_path.as_os_str());
    let mut vol_buf = vec![0u16; 264];
    // SAFETY: `wide_path` is a NUL-terminated UTF-16 string and `vol_buf` has capacity for 264 `u16`s.
    let ok = unsafe {
        GetVolumePathNameW(
            wide_path.as_ptr(),
            vol_buf.as_mut_ptr(),
            u32::try_from(vol_buf.len()).unwrap_or(264),
        )
    };
    if ok == 0 {
        return Err(last_error("GetVolumePathNameW"));
    }
    // SAFETY: `vol_buf` was populated with a NUL-terminated volume root path by `GetVolumePathNameW`.
    let drive_type = unsafe { GetDriveTypeW(vol_buf.as_ptr()) };
    Ok(drive_type == DRIVE_REMOTE_CODE)
}

pub(crate) fn place(path: &Path) -> Result<Place, WinError> {
    if let Some(Component::Prefix(prefix_component)) = path.components().next()
        && matches!(
            prefix_component.kind(),
            Prefix::UNC(_, _) | Prefix::VerbatimUNC(_, _)
        )
    {
        return Ok(Place {
            reparse: false,
            sync_root: false,
            network: true,
        });
    }

    let existing = nearest_existing_ancestor(path)?;
    let reparse = check_reparse_chain(&existing)?;
    let sync_root = check_sync_root(&existing)?;
    let network = check_network(path, Some(&existing))?;
    Ok(Place {
        reparse,
        sync_root,
        network,
    })
}

fn classify_sid(sid: PSID) -> WriterKind {
    if sid.is_null() {
        return WriterKind::Unknown;
    }
    // SAFETY: `sid` is non-null; `IsValidSid` validates the SID header and subauthority count.
    if unsafe { IsValidSid(sid) } == 0 {
        return WriterKind::Unknown;
    }
    // SAFETY: `sid` is a valid SID.
    if unsafe { IsWellKnownSid(sid, WinBuiltinAdministratorsSid) } != 0 {
        return WriterKind::Administrators;
    }
    // SAFETY: `sid` is a valid SID.
    if unsafe { IsWellKnownSid(sid, WinLocalSystemSid) } != 0 {
        return WriterKind::System;
    }

    let mut name_len: u32 = 0;
    let mut domain_len: u32 = 0;
    let mut use_kind: i32 = 0;
    // SAFETY: Queries buffer lengths for `sid` with null output buffers.
    let ok = unsafe {
        LookupAccountSidW(
            ptr::null(),
            sid,
            ptr::null_mut(),
            &raw mut name_len,
            ptr::null_mut(),
            &raw mut domain_len,
            &raw mut use_kind,
        )
    };
    // SAFETY: `GetLastError` reads the calling thread's last-error slot and takes no pointers.
    if ok != 0 || unsafe { GetLastError() } == ERROR_INSUFFICIENT_BUFFER {
        WriterKind::Other
    } else {
        WriterKind::Unknown
    }
}

pub(crate) fn writers(path: &Path) -> Result<Vec<WriterKind>, WinError> {
    let wide = to_wide_null(path.as_os_str());
    let mut owner_sid: PSID = ptr::null_mut();
    let mut dacl: *mut ACL = ptr::null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = ptr::null_mut();

    // SAFETY: `wide` is a NUL-terminated path; `owner_sid`, `dacl` and `sd` receive pointers into
    // the allocated security descriptor `sd`, which is freed with `LocalFree`.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &raw mut owner_sid,
            ptr::null_mut(),
            &raw mut dacl,
            ptr::null_mut(),
            &raw mut sd,
        )
    };
    if status != 0 {
        return Err(WinError {
            call: "GetNamedSecurityInfoW",
            code: status,
        });
    }

    let mut out = Vec::new();
    let owner_kind = classify_sid(owner_sid);
    if owner_kind != WriterKind::Administrators && owner_kind != WriterKind::System {
        out.push(owner_kind);
    }

    if dacl.is_null() {
        out.push(WriterKind::Other);
    } else {
        // SAFETY: `dacl` is a non-null `ACL` pointer inside `sd`.
        let ace_count = unsafe { (*dacl).AceCount };
        for idx in 0..u32::from(ace_count) {
            let mut ace_ptr: *mut c_void = ptr::null_mut();
            // SAFETY: `dacl` is valid and `idx < ace_count`; `ace_ptr` receives the ACE address.
            let got = unsafe { GetAce(dacl, idx, &raw mut ace_ptr) };
            if got == 0 || ace_ptr.is_null() {
                out.push(WriterKind::Unknown);
                continue;
            }
            // SAFETY: Every ACE begins with an `ACE_HEADER`.
            let header = unsafe { *ace_ptr.cast::<ACE_HEADER>() };
            match header.AceType {
                ACCESS_ALLOWED_ACE_TYPE_CODE | ACCESS_ALLOWED_CALLBACK_ACE_TYPE_CODE => {
                    // SAFETY: Standard and callback access-allowed ACEs share the `ACCESS_ALLOWED_ACE` prefix.
                    let allowed = unsafe { &*ace_ptr.cast::<ACCESS_ALLOWED_ACE>() };
                    if (allowed.Mask & WRITE_RIGHTS_MASK) != 0 {
                        let sid_ptr = (&raw const allowed.SidStart).cast::<c_void>() as PSID;
                        out.push(classify_sid(sid_ptr));
                    }
                }
                ACCESS_DENIED_ACE_TYPE_CODE
                | ACCESS_DENIED_OBJECT_ACE_TYPE_CODE
                | ACCESS_DENIED_CALLBACK_ACE_TYPE_CODE
                | ACCESS_DENIED_CALLBACK_OBJECT_ACE_TYPE_CODE => {}
                _ => {
                    out.push(WriterKind::Unknown);
                }
            }
        }
    }

    if !sd.is_null() {
        // SAFETY: `sd` was allocated by `GetNamedSecurityInfoW` and must be freed with `LocalFree`.
        unsafe {
            let _ = LocalFree(sd);
        }
    }
    Ok(out)
}

pub(crate) fn mark_not_indexed(path: &Path) -> Result<(), WinError> {
    let wide = to_wide_null(path.as_os_str());
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 path string.
    let attrs = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return Err(last_error("GetFileAttributesW"));
    }
    let updated = attrs | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED;
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 path string.
    let set_ok = unsafe { SetFileAttributesW(wide.as_ptr(), updated) };
    if set_ok == 0 {
        return Err(last_error("SetFileAttributesW"));
    }
    Ok(())
}

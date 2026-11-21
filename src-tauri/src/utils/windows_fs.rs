use std::fs;

pub(crate) fn windows_ntstatus_error(
    status: windows_sys::Win32::Foundation::NTSTATUS,
) -> std::io::Error {
    let code = unsafe { windows_sys::Win32::Foundation::RtlNtStatusToDosError(status) };
    std::io::Error::from_raw_os_error(code as i32)
}

/// Open or create one name relative to a directory handle. The caller maps
/// NTSTATUS before Win32 collapses distinct conditions into the same error.
pub(crate) fn open_relative_handle(
    path: (&fs::File, &std::ffi::OsStr),
    desired_access: u32,
    share_access: u32,
    create_disposition: u32,
    create_options: u32,
    file_attributes: u32,
    map_status: impl FnOnce(windows_sys::Win32::Foundation::NTSTATUS) -> std::io::Error,
) -> std::io::Result<fs::File> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows_sys::Wdk::Storage::FileSystem::NtCreateFile;
    use windows_sys::Win32::Foundation::{HANDLE, UNICODE_STRING};
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    let (parent, name) = path;
    let mut wide = name.encode_wide().collect::<Vec<_>>();
    let byte_len = wide
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .filter(|length| *length <= u16::MAX as usize)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Windows path component is too long",
            )
        })?;
    if wide.is_empty() || wide.contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Windows path component is empty or contains NUL",
        ));
    }
    let unicode = UNICODE_STRING {
        Length: byte_len as u16,
        MaximumLength: byte_len as u16,
        Buffer: wide.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle() as HANDLE,
        ObjectName: &unicode,
        // OBJ_CASE_INSENSITIVE. Windows file opens are ordinarily
        // case-insensitive; preserving that contract avoids a surprising
        // divergence from CreateFileW while still anchoring resolution to the
        // verified parent handle.
        Attributes: 0x40,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut io_status: IO_STATUS_BLOCK = unsafe { std::mem::zeroed() };
    let mut handle: HANDLE = std::ptr::null_mut();
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            desired_access,
            &attributes,
            &mut io_status,
            std::ptr::null(),
            file_attributes,
            share_access,
            create_disposition,
            create_options,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(map_status(status));
    }
    if handle.is_null() {
        return Err(std::io::Error::other(
            "NtCreateFile succeeded without returning a handle",
        ));
    }
    Ok(unsafe { fs::File::from_raw_handle(handle as _) })
}

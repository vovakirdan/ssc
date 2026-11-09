use crate::errors::AppError;
use ring::rand::{SecureRandom, SystemRandom};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use zeroize::Zeroizing;

fn error() -> AppError {
    super::error(
        "access_storage",
        "Не удалось безопасно открыть данные доступа",
        false,
    )
}

pub fn prepare(directory: &Path) -> Result<(), AppError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(directory).map_err(|_| error())?;
        let meta = fs::symlink_metadata(directory).map_err(|_| error())?;
        if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } {
            return Err(error());
        }
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(|_| error())?;
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(directory).map_err(|_| error())?;
    }
    Ok(())
}

pub fn read(path: &Path, encrypted: bool) -> Result<Option<Zeroizing<Vec<u8>>>, AppError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(error()),
    };
    let meta = file.metadata().map_err(|_| error())?;
    if !meta.is_file() || meta.len() > 16 * 1024 {
        return Err(error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
            return Err(error());
        }
    }
    let mut bytes = Zeroizing::new(Vec::new());
    Read::by_ref(&mut file)
        .take(16 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error())?;
    if bytes.len() > 16 * 1024 {
        return Err(error());
    }
    if encrypted {
        Ok(Some(unprotect(&bytes)?))
    } else {
        Ok(Some(bytes))
    }
}

pub fn write(path: &Path, bytes: &[u8], encrypted: bool) -> Result<(), AppError> {
    let directory = path.parent().ok_or_else(error)?;
    prepare(directory)?;
    let bytes = if encrypted {
        protect(bytes)?
    } else {
        Zeroizing::new(bytes.to_vec())
    };
    let mut suffix = [0; 12];
    SystemRandom::new().fill(&mut suffix).map_err(|_| error())?;
    let temporary = directory.join(format!(".access-{}", hex::encode(suffix)));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|_| error())?;
        file.write_all(&bytes).map_err(|_| error())?;
        file.sync_all().map_err(|_| error())?;
        replace(&temporary, path)?;
        #[cfg(unix)]
        {
            File::open(directory)
                .and_then(|f| f.sync_all())
                .map_err(|_| error())?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> Result<(), AppError> {
    fs::rename(from, to).map_err(|_| error())
}
#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> Result<(), AppError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let from: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(error())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn protect(bytes: &[u8]) -> Result<Zeroizing<Vec<u8>>, AppError> {
    Ok(Zeroizing::new(bytes.to_vec()))
}
#[cfg(not(windows))]
fn unprotect(bytes: &[u8]) -> Result<Zeroizing<Vec<u8>>, AppError> {
    Ok(Zeroizing::new(bytes.to_vec()))
}
#[cfg(windows)]
fn protect(bytes: &[u8]) -> Result<Zeroizing<Vec<u8>>, AppError> {
    dpapi(bytes, true)
}
#[cfg(windows)]
fn unprotect(bytes: &[u8]) -> Result<Zeroizing<Vec<u8>>, AppError> {
    dpapi(bytes, false)
}
#[cfg(windows)]
fn dpapi(bytes: &[u8], encrypt: bool) -> Result<Zeroizing<Vec<u8>>, AppError> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().map_err(|_| error())?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(error());
    }
    let result = unsafe {
        Zeroizing::new(std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec())
    };
    unsafe {
        std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        LocalFree(output.pbData as *mut std::ffi::c_void);
    }
    Ok(result)
}

/// The OS releases this lock even if a client crashes during activation.
/// Separate application processes must never generate competing keys for one profile.
pub fn lock(directory: &Path) -> Result<File, AppError> {
    prepare(directory)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options
        .open(directory.join(".access.lock"))
        .map_err(|_| error())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = file.metadata().map_err(|_| error())?;
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
            return Err(error());
        }
    }
    file.try_lock().map_err(|_| {
        super::error(
            "access_busy",
            "Активация выполняется в другом окне. Повторите попытку",
            true,
        )
    })?;
    Ok(file)
}

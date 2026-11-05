//! Descriptor-relative, bounded repository storage. No symlink component is followed.
use rustix::fs::{self, AtFlags, Mode, OFlags};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    os::fd::OwnedFd,
    path::{Component, Path},
};
pub const LIMIT: u64 = 8 * 1024 * 1024;
pub fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
pub fn json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|e| e.to_string())
}
pub fn allocate() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub struct Store {
    root: OwnedFd,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        // Opening every component also refuses symlinks in the supplied repository root.
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };
        let mut fd = fs::open(
            "/",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?;
        for c in absolute.components() {
            match c {
                Component::RootDir => (),
                Component::Normal(n) => {
                    fd = fs::openat(
                        &fd,
                        n,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|e| format!("root component: {e}"))?;
                }
                _ => return Err("unsafe root".into()),
            }
        }
        Ok(Self { root: fd })
    }
    fn parent(&self, path: &str, create: bool) -> Result<(OwnedFd, String), String> {
        let parts: Vec<_> = Path::new(path).components().collect();
        if parts.is_empty() || parts.iter().any(|p| !matches!(p, Component::Normal(_))) {
            return Err(format!("unsafe path: {path}"));
        }
        let mut fd = fs::openat(
            &self.root,
            ".",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?;
        for part in &parts[..parts.len() - 1] {
            let name = part.as_os_str();
            if create {
                match fs::mkdirat(&fd, name, Mode::from_bits_truncate(0o700)) {
                    Ok(()) => {
                        fs::fsync(&fd).map_err(|e| e.to_string())?;
                    }
                    Err(rustix::io::Errno::EXIST) => (),
                    Err(e) => return Err(e.to_string()),
                }
            }
            fd = fs::openat(
                &fd,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|e| e.to_string())?;
        }
        Ok((
            fd,
            parts
                .last()
                .unwrap()
                .as_os_str()
                .to_str()
                .ok_or("non UTF-8 path")?
                .into(),
        ))
    }
    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>, String> {
        let (fd, name) = match self.parent(path, false) {
            Ok(x) => x,
            Err(e) => {
                if e.contains("No such file") {
                    return Ok(None);
                }
                return Err(e);
            }
        };
        let f = match fs::openat(
            &fd,
            name.as_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(f) => f,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let f = File::from(f);
        let m = f.metadata().map_err(|e| e.to_string())?;
        if !m.is_file() || m.len() > LIMIT {
            return Err(format!("not a bounded regular file: {path}"));
        }
        let mut bytes = Vec::new();
        f.take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > LIMIT {
            return Err("file exceeds limit".into());
        }
        Ok(Some(bytes))
    }
    pub fn load<T: DeserializeOwned>(&self, path: &str) -> Result<Option<T>, String> {
        self.read(path)?
            .map(|b| serde_json::from_slice(&b).map_err(|e| format!("{path}: {e}")))
            .transpose()
    }
    pub fn write<T: Serialize>(&self, path: &str, v: &T) -> Result<(), String> {
        self.write_bytes(path, &json(v)?)
    }
    pub fn write_bytes(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() as u64 > LIMIT {
            return Err("write exceeds file limit".into());
        }
        let (fd, name) = self.parent(path, true)?;
        // Refuse replacing symlinks, including dangling symlinks.
        match fs::statat(&fd, name.as_str(), AtFlags::SYMLINK_NOFOLLOW) {
            Ok(st) => {
                if fs::FileType::from_raw_mode(st.st_mode) != fs::FileType::RegularFile {
                    return Err("destination is not regular".into());
                }
            }
            Err(rustix::io::Errno::NOENT) => (),
            Err(e) => return Err(e.to_string()),
        }
        let tmp = format!(".write-{}", allocate()?);
        let f = fs::openat(
            &fd,
            tmp.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(|e| e.to_string())?;
        let result = (|| {
            let mut f = File::from(f);
            f.write_all(bytes)
                .and_then(|()| f.sync_all())
                .map_err(|e| e.to_string())?;
            fs::renameat(&fd, tmp.as_str(), &fd, name.as_str()).map_err(|e| e.to_string())?;
            fs::fsync(&fd).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = fs::unlinkat(&fd, tmp.as_str(), AtFlags::empty());
        }
        result
    }
    pub fn remove(&self, path: &str) -> Result<(), String> {
        let (fd, name) = self.parent(path, false)?;
        match fs::unlinkat(&fd, name.as_str(), AtFlags::empty()) {
            Ok(()) => fs::fsync(&fd).map_err(|e| e.to_string()),
            Err(rustix::io::Errno::NOENT) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn list(&self, path: &str) -> Result<Vec<String>, String> {
        let (fd, name) = match self.parent(path, false) {
            Ok(x) => x,
            Err(e) => {
                if e.contains("No such file") {
                    return Ok(vec![]);
                }
                return Err(e);
            }
        };
        let dir = match fs::openat(
            &fd,
            name.as_str(),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(f) => f,
            Err(rustix::io::Errno::NOENT) => return Ok(vec![]),
            Err(e) => return Err(e.to_string()),
        };
        let mut out = Vec::new();
        let mut entries = fs::Dir::read_from(&dir).map_err(|e| e.to_string())?;
        for e in &mut entries {
            let e = e.map_err(|e| e.to_string())?;
            let n = e.file_name().to_str().map_err(|e| e.to_string())?;
            if n == "." || n == ".." {
                continue;
            }
            if out.len() >= 10000 {
                return Err("directory entry limit".into());
            }
            out.push(n.into());
        }
        out.sort();
        Ok(out)
    }
    /// Lock the existing repository descriptor without creating state (nightly is read-only).
    pub fn read_lock(&self) -> Result<(), String> {
        fs::flock(&self.root, fs::FlockOperation::NonBlockingLockShared)
            .map_err(|e| format!("reader lock: {e}"))
    }
    pub fn lock(&self) -> Result<OwnedFd, String> {
        fs::flock(&self.root, fs::FlockOperation::NonBlockingLockExclusive)
            .map_err(|e| format!("repository writer lock: {e}"))?;
        let (fd, name) = self.parent(".devrec/lock", true)?;
        let lock = fs::openat(
            &fd,
            name.as_str(),
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o600),
        )
        .map_err(|e| e.to_string())?;
        if fs::FileType::from_raw_mode(fs::fstat(&lock).map_err(|e| e.to_string())?.st_mode)
            != fs::FileType::RegularFile
        {
            return Err("writer lock is not a regular file".into());
        }
        fs::flock(&lock, fs::FlockOperation::NonBlockingLockExclusive)
            .map_err(|e| format!("writer lock: {e}"))?;
        Ok(lock)
    }
}

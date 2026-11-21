//! Linux host plumbing in plain syscalls: loop devices, ext4 and overlay mounts, binderfs and its device nodes.

use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use nix::mount::{MntFlags, MsFlags, mount, umount2};

const LOOP_CTL_GET_FREE: libc::c_ulong = 0x4C82;
const LOOP_CONFIGURE: libc::c_ulong = 0x4C0A;
const LO_FLAGS_READ_ONLY: u32 = 1;
const LO_FLAGS_AUTOCLEAR: u32 = 4;
const BINDER_CTL_ADD: libc::c_ulong = (3 << 30) | (264 << 16) | ((b'b' as libc::c_ulong) << 8) | 1;

#[repr(C)]
struct LoopInfo64 {
    device: u64,
    inode: u64,
    rdevice: u64,
    offset: u64,
    sizelimit: u64,
    number: u32,
    encrypt_type: u32,
    encrypt_key_size: u32,
    flags: u32,
    file_name: [u8; 64],
    crypt_name: [u8; 64],
    encrypt_key: [u8; 32],
    init: [u64; 2],
}

#[repr(C)]
struct LoopConfig {
    fd: u32,
    block_size: u32,
    info: LoopInfo64,
    reserved: [u64; 8],
}

#[repr(C)]
struct BinderDevice {
    name: [u8; 256],
    major: u32,
    minor: u32,
}

fn io_error(context: &str, error: io::Error) -> String {
    format!("{context}: {error}")
}

/// Attaches `image` read-only to a free loop device; the device detaches itself once its last user (the mount) is gone.
/// The returned handle must stay open until the mount exists: with autoclear, closing the last descriptor first detaches it.
pub fn attach_loop(image: &Path) -> Result<(PathBuf, File), String> {
    let control = OpenOptions::new().read(true).write(true).open("/dev/loop-control").map_err(|error| io_error("/dev/loop-control", error))?;
    // SAFETY: LOOP_CTL_GET_FREE takes no argument and returns the number of a free loop device.
    let number = unsafe { libc::ioctl(control.as_raw_fd(), LOOP_CTL_GET_FREE) };
    if number < 0 {
        return Err(io_error("LOOP_CTL_GET_FREE", io::Error::last_os_error()));
    }
    let device = PathBuf::from(format!("/dev/loop{number}"));
    let loop_file = OpenOptions::new().read(true).write(true).open(&device).map_err(|error| io_error(&device.display().to_string(), error))?;
    let backing = File::open(image).map_err(|error| io_error(&image.display().to_string(), error))?;
    let mut config = LoopConfig {
        fd: backing.as_raw_fd() as u32,
        block_size: 0,
        info: LoopInfo64 {
            device: 0,
            inode: 0,
            rdevice: 0,
            offset: 0,
            sizelimit: 0,
            number: 0,
            encrypt_type: 0,
            encrypt_key_size: 0,
            flags: LO_FLAGS_READ_ONLY | LO_FLAGS_AUTOCLEAR,
            file_name: [0; 64],
            crypt_name: [0; 64],
            encrypt_key: [0; 32],
            init: [0; 2],
        },
        reserved: [0; 8],
    };
    // SAFETY: `config` is a fully initialised loop_config and outlives the call.
    let result = unsafe { libc::ioctl(loop_file.as_raw_fd(), LOOP_CONFIGURE, &mut config as *mut LoopConfig) };
    if result < 0 {
        return Err(io_error("LOOP_CONFIGURE", io::Error::last_os_error()));
    }
    Ok((device, loop_file))
}

/// Filesystem type of an image from its superblock magic (ext4 at 1080, erofs at 1024).
pub fn probe_filesystem(image: &Path) -> Result<&'static str, String> {
    let mut file = File::open(image).map_err(|error| io_error(&image.display().to_string(), error))?;
    let mut magic = [0u8; 4];
    file.seek(SeekFrom::Start(1024)).and_then(|_| file.read_exact(&mut magic)).map_err(|error| error.to_string())?;
    if u32::from_le_bytes(magic) == 0xE0F5_E1E2 {
        return Ok("erofs");
    }
    let mut ext = [0u8; 2];
    file.seek(SeekFrom::Start(1080)).and_then(|_| file.read_exact(&mut ext)).map_err(|error| error.to_string())?;
    if ext == [0x53, 0xEF] {
        return Ok("ext4");
    }
    Err(format!("{}: neither ext4 nor erofs", image.display()))
}

pub fn is_mount_point(path: &Path) -> bool {
    let Ok(real) = fs::canonicalize(path) else { return false };
    fs::read_to_string("/proc/self/mountinfo")
        .map(|table| table.lines().any(|line| line.split(' ').nth(4).map(Path::new) == Some(real.as_path())))
        .unwrap_or(false)
}

pub fn mount_image(image: &Path, target: &Path) -> Result<(), String> {
    if is_mount_point(target) {
        return Ok(());
    }
    fs::create_dir_all(target).map_err(|error| io_error(&target.display().to_string(), error))?;
    let filesystem = probe_filesystem(image)?;
    let (device, _hold) = attach_loop(image)?;
    mount(Some(&device), target, Some(filesystem), MsFlags::MS_RDONLY, None::<&str>)
        .map_err(|error| format!("mount {} on {}: {error}", device.display(), target.display()))
}

/// Writable view of a read-only `lower` tree: every change lands in `upper`.
pub fn mount_overlay(lower: &Path, upper: &Path, work: &Path, target: &Path) -> Result<(), String> {
    if is_mount_point(target) {
        return Ok(());
    }
    for directory in [upper, work, target] {
        fs::create_dir_all(directory).map_err(|error| io_error(&directory.display().to_string(), error))?;
    }
    let options = format!("lowerdir={},upperdir={},workdir={}", lower.display(), upper.display(), work.display());
    mount(Some("overlay"), target, Some("overlay"), MsFlags::empty(), Some(options.as_str()))
        .map_err(|error| format!("overlay on {}: {error}", target.display()))
}

pub fn unmount(target: &Path) -> Result<(), String> {
    while is_mount_point(target) {
        umount2(target, MntFlags::MNT_DETACH).map_err(|error| format!("umount {}: {error}", target.display()))?;
    }
    Ok(())
}

/// Mounts a private binderfs instance and creates the three Android binder devices in it. A private instance keeps
/// the guest's binder contexts apart from every other binder user on the host.
pub fn setup_binderfs(target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|error| io_error(&target.display().to_string(), error))?;
    if !is_mount_point(target) {
        mount(Some("binder"), target, Some("binder"), MsFlags::empty(), None::<&str>)
            .map_err(|error| format!("mount binderfs on {}: {error} (the kernel needs CONFIG_ANDROID_BINDERFS)", target.display()))?;
    }
    let control = OpenOptions::new().read(true).write(true).open(target.join("binder-control")).map_err(|error| io_error("binder-control", error))?;
    for name in ["binder", "vndbinder", "hwbinder"] {
        if target.join(name).exists() {
            continue;
        }
        let mut device = BinderDevice { name: [0; 256], major: 0, minor: 0 };
        device.name[..name.len()].copy_from_slice(name.as_bytes());
        // SAFETY: BINDER_CTL_ADD reads and writes one binderfs_device struct of 264 bytes.
        let result = unsafe { libc::ioctl(control.as_raw_fd(), BINDER_CTL_ADD, &mut device as *mut BinderDevice) };
        if result < 0 {
            return Err(io_error(&format!("BINDER_CTL_ADD {name}"), io::Error::last_os_error()));
        }
    }
    for name in ["binder", "vndbinder", "hwbinder"] {
        let node = CString::new(target.join(name).as_os_str().as_encoded_bytes()).map_err(|error| error.to_string())?;
        // SAFETY: `node` is a valid NUL-terminated path.
        if unsafe { libc::chmod(node.as_ptr(), 0o666) } != 0 {
            return Err(io_error(&format!("chmod {name}"), io::Error::last_os_error()));
        }
    }
    Ok(())
}

/// Host facts `fw android status` reports.
pub fn prerequisites() -> Vec<(String, bool, String)> {
    let filesystems = fs::read_to_string("/proc/filesystems").unwrap_or_default();
    let has = |name: &str| filesystems.lines().any(|line| line.split_whitespace().last() == Some(name));
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    vec![
        ("kernel".into(), true, kernel.trim().to_owned()),
        ("binderfs".into(), has("binder"), "binder in /proc/filesystems".into()),
        ("overlayfs".into(), has("overlay"), "overlay in /proc/filesystems".into()),
        ("cgroup2".into(), has("cgroup2"), "cgroup2 in /proc/filesystems".into()),
        ("loop".into(), Path::new("/dev/loop-control").exists(), "/dev/loop-control".into()),
        ("memfd".into(), true, "memfd_create(2) is in every kernel since 3.17; the guest runs with sys.use_memfd=true".into()),
        ("kvm".into(), Path::new("/dev/kvm").exists(), "/dev/kvm (only the VM path needs it)".into()),
        ("render-node".into(), Path::new("/dev/dri/renderD128").exists(), "/dev/dri/renderD128".into()),
        ("root".into(), effective_uid() == 0, "mounts, binderfs and the container need euid 0".into()),
    ]
}

fn effective_uid() -> u32 {
    nix::unistd::geteuid().as_raw()
}

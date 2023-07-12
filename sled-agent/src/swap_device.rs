// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#[derive(Debug, thiserror::Error)]
pub enum SwapDeviceError {
    #[error("Could not find boot disk")]
    BootDiskNotFound,

    #[error("Error running ZFS command: {0}")]
    Zfs(illumos_utils::ExecutionError),

    #[error("{msg}: {error}")]
    Keyfile {
        error: String,
        msg: &'static str,
    },

    #[error("Error listing swap devices: {0}")]
    ListDevices(String),

    #[error("Error adding swap device: {msg} (path=\"{path}\", start={start}, length={length})")]
    AddDevice { msg: String, path: String, start: u64, length: u64 },
}

/// Ensure the system has a swap device, creating the underlying block
/// device if necessary.
///
/// The swap device is an encrypted zvol that lives on the M.2 disk that the
/// system booted from.  Because it booted from the disk, we know for certain
/// the system can access it. We encrypt the zvol because arbitrary system
/// memory could exist in swap, including sensitive data. The zvol is encrypted
/// with an ephemeral key; we throw it away immediately after creation and
/// create a new zvol if we find one on startup (that isn't backing a current
/// swap device). An ephemeral key is prudent because the kernel has the key
/// once the device is created, and there is no need for anything else to ever
/// decrypt swap.
///
/// To achieve idempotency in the case of crash and restart, we do the following:
///   1. On startup, check if there is a swap device. If one exists, we are done.
///      Swap devices do not persist across reboot by default, so if a device
///      already exists, this isn't our first time starting after boot. The
///      device may be in use. Changes to how the swap device is setup, should we
///      decide to do that, will be across reboots (as this is how sled-agent is
///      upgraded), so we will get a shot to make changes across upgrade.
///   2. If there is no swap device, check for a zvol at the known path on the
///      M.2 that we booted from. If we find such a zvol, delete it.
///   3. Create an encrypted zvol with a randomly generated key that is
///      immediately discarded.
///   4. Add the zvol as a swap device with swapctl(2).
///
/// Note that this introduces a sled-agent upgrade consideration if we ever
/// choose to change how we set up the device. A configured swap device does not
/// persist across reboot by default, but a zvol does. Thus, if the well known
/// path for the zvol ever changes, we will need to at least support a window
/// where we check for both the previously well-known path and the new
/// configuration.
pub(crate) async fn ensure_swap_device(
    log: &slog::Logger,
    boot_zpool_name: &illumos_utils::zpool::ZpoolName,
    size_gb: u8,
) -> Result<(), SwapDeviceError> {
    assert!(size_gb > 0);

    let devs = swapctl::list_swap_devices()?;
    if devs.len() > 0 {
        if devs.len() > 1 {
            // This should really never happen unless we've made a mistake, but it's
            // probably fine to have more than one swap device. Thus, don't panic
            // over it, but do log a warning so there is evidence that we found
            // extra devices.
            warn!(
                log,
                "Found multiple existing swap devices on startup: {:?}", devs
            );
        } else {
            info!(log, "Swap device already exists: {:?}", devs);
        }

        return Ok(());
    }

    let swap_zvol = format!("{}/{}", boot_zpool_name, "swap");
    if zvol_exists(&swap_zvol)? {
        zvol_destroy(&swap_zvol)?;
    }

    // The process of paging out using block I/O, so use the "dsk" version of
    // the zvol path (as opposed to "rdsk", which is for character/raw access.)
    let swapname = format!("/dev/zvol/dsk/{}", swap_zvol);
    create_encrypted_swap_zvol(log, &swapname, size_gb).await?;

    // Specifying 0 length tells the kernel to use the size of the device.
    swapctl::add_swap_device(swapname, 0, 0)?;

    Ok(())
}

// Check whether the given zvol exists.
fn zvol_exists(name: &str) -> Result<bool, SwapDeviceError> {
    let mut command = std::process::Command::new(illumos_utils::zfs::ZFS);
    let cmd = command.args(&["list", "-Hpo", "name,type"]);

    let output =
        illumos_utils::execute(cmd).map_err(|e| SwapDeviceError::Zfs(e))?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut found = false;
    for line in stdout.lines() {
        let v: Vec<_> = line.split('\t').collect();

        if v[0] != name {
            continue;
        }
        if v[1] != "volume" {
            panic!(
                "found dataset \"{}\" for swap device, but it is not a volume",
                name
            );
        } else {
            found = true;
        }
    }

    Ok(found)
}

// Destroys a zvol at the given path, double checking that it's gone after
// issuing the destroy command.
fn zvol_destroy(name: &str) -> Result<(), SwapDeviceError> {
    let mut command = std::process::Command::new(illumos_utils::zfs::ZFS);
    let cmd = command.args(&["destroy", name]);
    illumos_utils::execute(cmd).map_err(|e| SwapDeviceError::Zfs(e))?;

    // TODO: remove after testing
    if zvol_exists(name)? {
        panic!("zvol not cleaned up");
    }

    Ok(())
}

// Creates an encrypted zvol at the input path with the given size.
//
// The keyfile is created in a location and tmpfs and unlinked after the zvol is
// created.
async fn create_encrypted_swap_zvol(
    log: &slog::Logger,
    name: &str,
    size_gb: u8,
) -> Result<(), SwapDeviceError> {
    // Create an ephemeral key from random bytes.
    let mut urandom = std::fs::OpenOptions::new().create(false).read(true).open("/dev/urandom").map_err(|e| Keyfile {
        msg: "could not open /dev/urandom",
        error: e.to_string(),
    })?;
    let mut bytes = vec![0u8; 64];
    urandom.read_exact(&mut bytes).map_err(|e| Keyfile {
        msg: "could not read from /dev/urandom",
        error: e.to_string(),
    })?;


    // TODO: path, generate random bytes
    let kp = illumos_utils::zfs::Keypath(camino::Utf8PathBuf::from(format!(
        "{}/swap",
        sled_hardware::disk::KEYPATH_ROOT
    )));
    let keypath = format!("{}", kp);
    let key = [0; 32];
    let mut keyfile = sled_hardware::KeyFile::create(kp, &key, log)
        .await
        .map_err(|e| SwapDeviceError::Keyfile {
            msg: "could not create keyfile",
            error: e.to_string(),
        })?;

    // Create the zvol
    let size_arg = format!("{}G", size_gb);
    let mut command = std::process::Command::new(illumos_utils::zfs::ZFS);
    let cmd = command.args(&[
        "create",
        "-s",
        "-V",
        &size_arg,
        "-b",
        // TODO: correct thing here for pageconf
        "4096",
        "-o",
        "logbias=throughput",
        "-o",
        "primarycache=metadata",
        "-o",
        "secondarycache=none",
        "-o",
        "encryption=aes-256-gcm",
        "-o",
        "keyformat=raw",
        "-o",
        &keypath,
        name,
    ]);

    illumos_utils::execute(cmd).map_err(|e| SwapDeviceError::Zfs(e))?;

    // Unlink the key.
    keyfile.zero_and_unlink().await.map_err(|e| {
        SwapDeviceError::Keyfile {
            msg: "could not zero and unlink keyfile",
            error: e.to_string(),
        }
    })?;

    // TODO: remove after testing
    if !zvol_exists(name)? {
        panic!("zvol not created successfully");
    }

    Ok(())
}

/// Wrapper functions around swapctl(2) operations
mod swapctl {
    use crate::swap_device::SwapDeviceError;

    #[derive(Debug)]
    #[allow(dead_code)]
    pub(crate) struct SwapDevice {
        /// path to the resource
        path: String,

        /// starting block on device used for swap
        start: u64,

        /// length of swap area
        length: u64,

        /// total number of pages used for swapping
        total_pages: u64,

        /// free npages for swapping
        free_pages: u64,

        flags: i64,
    }

    // swapctl(2)
    extern "C" {
        fn swapctl(cmd: i32, arg: *mut libc::c_void) -> i32;
    }

    // swapctl(2) commands
    const SC_ADD: i32 = 0x1;
    const SC_LIST: i32 = 0x2;
    #[allow(dead_code)]
    const SC_REMOVE: i32 = 0x3;
    const SC_GETNSWP: i32 = 0x4;

    // SC_ADD / SC_REMOVE arg
    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    struct swapres {
        sr_name: *const libc::c_char,
        sr_start: libc::off_t,
        sr_length: libc::off_t,
    }

    // SC_LIST arg: swaptbl with an embedded array of swt_n swapents
    #[repr(C)]
    #[derive(Debug, Clone)]
    struct swaptbl {
        swt_n: i32,
        swt_ent: [swapent; N_SWAPENTS],
    }
    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    struct swapent {
        ste_path: *const libc::c_char,
        ste_start: libc::off_t,
        ste_length: libc::off_t,
        ste_pages: libc::c_long,
        ste_free: libc::c_long,
        ste_flags: libc::c_long,
    }
    impl Default for swapent {
        fn default() -> Self {
            Self {
                ste_path: std::ptr::null_mut(),
                ste_start: 0,
                ste_length: 0,
                ste_pages: 0,
                ste_free: 0,
                ste_flags: 0,
            }
        }
    }

    // The argument for SC_LIST (struct swaptbl) requires an embedded array in the struct,
    // with swt_n entries, each of which requires a pointer to store the path to the
    // device.
    //
    // Ideally, we would want to query the number of swap devices on the system via
    // SC_GETNSWP, allocate enough memory for each device entry, then pass in
    // this memory to the list command. Unfortunately, creating a generically
    // large array embedded in a struct that can be passed to C is a bit of a
    // challenge in safe Rust. So instead, we just pick a reasonable max number
    // of devices to list.
    //
    // We pick a max of 3 devices, somewhat arbitrarily, but log the number of
    // swap devices we see regardless. We only ever expect to see 0 or 1 swap
    // device(s); if there are more, that is a bug. In this case we log a warning,
    // and eventually, we should send an ereport.
    const N_SWAPENTS: usize = 3;

    // Wrapper around swapctl(2) call. All commands except SC_GETNSWP require an
    // argument, hence `data` being an optional parameter.
    unsafe fn swapctl_cmd<T>(
        cmd: i32,
        data: Option<std::ptr::NonNull<T>>,
    ) -> std::io::Result<u32> {
        assert!(
            cmd >= SC_ADD && cmd <= SC_GETNSWP,
            "invalid swapctl cmd: {cmd}"
        );

        let ptr = match data {
            Some(v) => v.as_ptr() as *mut libc::c_void,
            None => std::ptr::null_mut(),
        };

        let res = swapctl(cmd, ptr);
        if res == -1 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(res as u32)
    }

    #[allow(dead_code)]
    fn swapctl_get_num_devices() -> std::io::Result<u32> {
        unsafe { swapctl_cmd::<i32>(SC_GETNSWP, None) }
    }

    /// List swap devices on the system.
    pub(crate) fn list_swap_devices() -> Result<Vec<SwapDevice>, SwapDeviceError>
    {
        // Statically create the array of swapents for SC_LIST: see comment on
        // `N_SWAPENTS` for details as to why we do this statically.
        //
        // Each swapent requires a char * pointer in our control for the
        // `ste_path` field,, which the kernel will fill in with a path if there
        // is a swap device for that entry. Because these pointers are mutated
        // by the kernel, we mark them as mutable. (Note that the compiler will
        // happily accept these definitions as non-mutable, since it can't know
        // what happens to the pointers on the C side, but not marking them as
        // mutable is undefined behavior, since they can be mutated).
        //
        // Per limits.h(3HEAD), PATH_MAX is the max number of bytes in a path
        // name, including the null terminating character, so these buffers
        // have sufficient space.
        const MAXPATHLEN: usize = libc::PATH_MAX as usize;
        assert_eq!(N_SWAPENTS, 3);
        let mut p1 = [0i8; MAXPATHLEN];
        let mut p2 = [0i8; MAXPATHLEN];
        let mut p3 = [0i8; MAXPATHLEN];
        let entries: [swapent; N_SWAPENTS] = [
            swapent {
                ste_path: &mut p1 as *mut libc::c_char,
                ..Default::default()
            },
            swapent {
                ste_path: &mut p2 as *mut libc::c_char,
                ..Default::default()
            },
            swapent {
                ste_path: &mut p3 as *mut libc::c_char,
                ..Default::default()
            },
        ];

        let mut list_req =
            swaptbl { swt_n: N_SWAPENTS as i32, swt_ent: entries };
        // Unwrap safety: We know this isn't null because we just created it
        let ptr = std::ptr::NonNull::new(&mut list_req).unwrap();
        let n_devices = unsafe {
            swapctl_cmd(SC_LIST, Some(ptr))
                .map_err(|e| SwapDeviceError::ListDevices(e.to_string()))?
        };

        let mut devices = Vec::with_capacity(n_devices as usize);
        for i in 0..n_devices as usize {
            let e = list_req.swt_ent[i];

            // Safety: CStr::from_ptr is documeted as safe if:
            //   1. The pointer contains a valid nul terminator at the end of the
            // string
            //   2. The pointer is valid for reads of bytes up to and including the
            // null terminator
            //   3. The memory referenced by the return CStr is not mutated for the
            // duration of lifetime 'a
            //
            // (1) is true because we initialize the buffers for ste_path as all
            // 0s, and their length is long enough to include the null
            // terminator for paths on the system.
            // (2) should be guaranteed by the syscall itself, and we can know
            // how many entries are valid via its return value.
            // (3) we aren't currently mutating the memory referenced by the
            // CStr, though there's nothing here enforcing that.
            let p = unsafe { std::ffi::CStr::from_ptr(e.ste_path) };
            let path = String::from_utf8_lossy(p.to_bytes()).to_string();

            devices.push(SwapDevice {
                path: path,
                start: e.ste_start as u64,
                length: e.ste_length as u64,
                total_pages: e.ste_pages as u64,
                free_pages: e.ste_free as u64,
                flags: e.ste_flags,
            });
        }

        Ok(devices)
    }

    /// Add a swap device at the given path.
    pub fn add_swap_device(
        path: String,
        start: u64,
        length: u64,
    ) -> Result<(), SwapDeviceError> {
        let path_cp = path.clone();
        let name = std::ffi::CString::new(path).map_err(|e| {
            SwapDeviceError::AddDevice {
                msg: format!(
                    "could not convert path to CString: {}",
                    e.to_string()
                ),
                path: path_cp.clone(),
                start: start,
                length: length,
            }
        })?;

        let mut add_req = swapres {
            sr_name: name.as_ptr(),
            sr_start: start as i64,
            sr_length: length as i64,
        };
        // Unwrap safety: We know this isn't null because we just created it
        let ptr = std::ptr::NonNull::new(&mut add_req).unwrap();

        let res = unsafe {
            swapctl_cmd(SC_ADD, Some(ptr)).map_err(|e| {
                SwapDeviceError::AddDevice {
                    msg: e.to_string(),
                    path: path_cp,
                    start: start,
                    length: length,
                }
            })?
        };
        assert_eq!(res, 0);

        Ok(())
    }
}

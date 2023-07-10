// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use omicron_common::api::external::ByteCount;

// TODO:
// - comment about why swap device is necessary at all
// - create and unlink key
// - pull in the rest of swapctl code

#[derive(Debug, thiserror::Error)]
pub enum SwapDeviceError {
    // TODO: error struct type?
    #[error("IO error: {0}")]
    Io(String),

    #[error("Boot device not found")]
    NoBootDeviceFound,
}

/// Ensure the system has a swap device setup, creating the underlying block
/// device if necessary.
///
/// The swap device is backed by an encrypted zvol that lives on the M.2 disk
/// that we booted from. Because we booted from the disk, we know for certain the
/// system can access it. We encrypt the zvol because arbitrary system memory could
/// exist in swap, including sensitive data. The zvol is encrypted with an
/// ephemeral key; we throw it away immediately after creation and create a new
/// zvol if we find one on startup (that isn't backing a current swap device). An
/// ephemeral key is prudent because the kernel has the key once the device is
/// created, and there is no need for anyone else to ever decrypt swap.
///
/// To achieve idempotency in the case of crash and restart, we do the following:
///   1. On startup, check if there is a swap device. If one exists, we are done.
///      Swap devices do not persist across reboot by default, so if a device
///      already exists, this isn't our first time starting after boot. The
///      device may be in use. Changes to how the swap device is setup, should we
///      decide to do that, will be across reboots, as this is how sled-agent is
///      upgraded, so we will get a shot to make changes across upgrade.
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
    storage: &crate::storage_manager::StorageManager,
    size_gb: u8,
) -> Result<(), SwapDeviceError> {
    assert!(size_gb > 0);

    // TODO error translation of io error
    let devs = swapctl::list_swap_devices()
        .map_err(|e| SwapDeviceError::Io(e.to_string()))?;
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

    let boot_disk = storage
        .resources()
        .boot_disk()
        .await
        .ok_or_else(|| SwapDeviceError::NoBootDeviceFound)?;
    // TODO
    let swap_zvol_path = format!("{}/{}", boot_disk.1, "swap");

    if zvol_exists(&swap_zvol_path)? {
        zvol_destroy(&swap_zvol_path)?;
    }

    create_encrypted_swap_zvol(&swap_zvol_path, size_gb)?;

    // Add the zvol as a swap device
    // TODO: right parameters here
    swapctl::add_swap_device(
        swap_zvol_path,
        ByteCount::from_kibibytes_u32(0),
        ByteCount::from_kibibytes_u32(0),
    )
    .map_err(|e| SwapDeviceError::Io(e.to_string()))?;

    Ok(())
}

fn zvol_exists(name: &str) -> Result<bool, SwapDeviceError> {
    let output = std::process::Command::new(illumos_utils::zfs::ZFS)
        .args(&["list", "-Hpo", "name,type"])
        .output()
        .map_err(|e| SwapDeviceError::Io(e.to_string()))?;
    if !output.status.success() {
        //return Err(SwapDeviceError::Io("zfs list failure".to_string()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut found = false;
    for line in stdout.lines() {
        let v: Vec<_> = line.split('\t').collect();

        if v[0] != name {
            continue;
        }
        if v[1] != "volume" {
            //return SwapDeviceError::Io(format!(
            //"{} found but not a volume",
            //zvol_path
            //));
        } else {
            found = true;
        }
    }

    Ok(found)
}

fn zvol_destroy(name: &str) -> Result<(), SwapDeviceError> {
    let output = std::process::Command::new(illumos_utils::zfs::ZFS)
        .args(&["destroy", name])
        .output()
        .map_err(|e| SwapDeviceError::Io(e.to_string()))?;
    if !output.status.success() {
        //return Err(SwapDeviceError::Io("zfs destroy failure".to_string()));
    }

    if zvol_exists(name)? {
        // TODO: error here
        panic!("zvol not cleaned up");
    }

    Ok(())
}

fn create_encrypted_swap_zvol(
    name: &str,
    size_gb: u8,
) -> Result<(), SwapDeviceError> {
    // Create the zvol
    let size_arg = format!("{}G", size_gb);
    let output = std::process::Command::new(illumos_utils::zfs::ZFS)
        .args(&[
            "create",
            "-s",
            "-V",
            &size_arg,
            "-b",
            // TODO: correct thing here
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
            // TODO: keypath
            "TODO keypath",
            name,
        ])
        .output()
        .map_err(|e| SwapDeviceError::Io(e.to_string()))?;
    if !output.status.success() {
        //return Err(SwapDeviceError::Io("zfs create failure".to_string()));
    }

    if !zvol_exists(name)? {
        // TODO: error here
        panic!("zvol not created successfully");
    }

    Ok(())
}

mod swapctl {
    use omicron_common::api::external::ByteCount;

    #[derive(Debug)]
    pub(crate) struct SwapDevice {}

    /// List all swap devices on the system.
    pub(crate) fn list_swap_devices() -> std::io::Result<Vec<SwapDevice>> {
        // TODO
        let devs = vec![];
        Ok(devs)
    }

    // TODO: could make this a swap device object as an arg
    /// Add a swap device at the given path
    pub fn add_swap_device(
        path: String,
        offset: ByteCount,
        length: ByteCount,
    ) -> std::io::Result<()> {
        // TODO
        Ok(())
    }

    // swapctl(2)
    extern "C" {
        fn swapctl(cmd: i32, arg: *mut libc::c_void) -> i32;
    }

    // swapctl(2) commands
    const SC_ADD: i32 = 0x1;
    const SC_LIST: i32 = 0x2;
    const SC_REMOVE: i32 = 0x3;
    const SC_GETNSWP: i32 = 0x4;

    // SC_ADD / SC_REMOVE arg
    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct swapres {
        sr_name: *const libc::c_char,
        sr_start: libc::off_t,
        sr_length: libc::off_t,
    }

    // SC_LIST arg: swaptbl with an embedded array of swt_n swapents
    #[repr(C)]
    #[derive(Debug, Clone)]
    pub struct swaptbl {
        swt_n: i32,
        swt_ent: [swapent; N_SWAPENTS],
    }
    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct swapent {
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
                ste_path: std::ptr::null(),
                ste_start: 0,
                ste_length: 0,
                ste_pages: 0,
                ste_free: 0,
                ste_flags: 0,
            }
        }
    }

    // The argument for SC_LIST (swaptbl) requires an embedded array in the struct,
    // with swt_n entries, each of which requires a pointer to store the path to the
    // device.
    //
    // Ideally, we would want to query the number of swap devices on the system via
    // SC_GETNSWP, allocate enough memory for the number of devices, then list the
    // swap devices. Creating a generically large array embedded in a struct that
    // can be passed to C is a bit of a challenge in safe Rust. So instead, we just
    // pick a reasonable max number of devices to list.
    //
    // We pick a max of 3 devices, somewhat arbitrarily, but log the number of
    // swap devices we see regardless. We only ever expect to see 0 or 1 swap
    // device(s); if there are more, that is a bug. In this case we log a warning,
    // and eventually, we should send an ereport.
    const N_SWAPENTS: usize = 3;

    unsafe fn swapctl_cmd<T>(
        cmd: i32,
        data: Option<*mut T>,
    ) -> std::io::Result<u32> {
        assert!(cmd >= 0 && cmd <= SC_GETNSWP, "invalid swapctl cmd: {cmd}");

        let ptr = match data {
            Some(v) => v as *mut libc::c_void,
            None => std::ptr::null_mut(),
        };

        let res = swapctl(cmd, ptr);
        if res == -1 {
            // TODO: log message
            // TODO: custom error
            return Err(std::io::Error::last_os_error());
        }

        Ok(res as u32)
    }

    fn swapctl_get_num_devices() -> std::io::Result<u32> {
        unsafe { swapctl_cmd::<i32>(SC_GETNSWP, None) }
    }

    fn swapctl_list_devices(ndev: u32) -> std::io::Result<Vec<SwapDevice>> {
        assert!(ndev > 0);

        let devs: Vec<SwapDevice> = Vec::with_capacity(ndev as usize);

        // statically allocate the array of swapents for SC_LIST
        //
        // see comment on `N_SWAPENTS` for details
        const MAXPATHLEN: usize = libc::PATH_MAX as usize;
        assert_eq!(N_SWAPENTS, 3);
        let p1 = [0i8; MAXPATHLEN];
        let p2 = [0i8; MAXPATHLEN];
        let p3 = [0i8; MAXPATHLEN];

        let entries: [swapent; N_SWAPENTS] = [
            swapent {
                ste_path: &p1 as *const libc::c_char,
                ..Default::default()
            },
            swapent {
                ste_path: &p2 as *const libc::c_char,
                ..Default::default()
            },
            swapent {
                ste_path: &p3 as *const libc::c_char,
                ..Default::default()
            },
        ];

        let mut list_req =
            swaptbl { swt_n: N_SWAPENTS as i32, swt_ent: entries };

        let n_devices = unsafe { swapctl_cmd(SC_LIST, Some(&mut list_req))? };

        // extract out the device information
        // TODO

        Ok(devs)
    }
}

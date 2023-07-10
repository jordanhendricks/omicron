// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use omicron_common::api::external::ByteCount;

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

    pub(crate) fn list_swap_devices() -> std::io::Result<Vec<SwapDevice>> {
        // TODO
        let devs = vec![];
        Ok(devs)
    }

    // TODO: could make this a swap device object as an arg
    pub fn add_swap_device(
        path: String,
        offset: ByteCount,
        length: ByteCount,
    ) -> std::io::Result<()> {
        // TODO
        Ok(())
    }
}

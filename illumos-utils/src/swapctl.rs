// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wrappers around swapctl(2) operations

use omicron_common::api::external::ByteCount;

#[derive(Debug)]
pub struct SwapDevice {
}

pub fn list_swap_devices() -> std::io::Result<Vec<SwapDevice>> {
    // TODO
    let devs = vec![];
    Ok(devs)
}

// TODO: could make this a swap device object as an arg
pub fn add_swap_device(path: String, offset: ByteCount, length: ByteCount) -> std::io::Result<()> {
    // TODO
    Ok(())
}

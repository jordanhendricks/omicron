// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Mock / dummy versions of the OPTE module, for non-illumos platforms

use slog::Logger;

use crate::addrobj::AddrObject;
use omicron_common::api::internal::shared::NetworkInterfaceKind;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid IP configuration for port")]
    InvalidPortIpConfig,

    #[error("Tried to release non-existent port ({0}, {1:?})")]
    ReleaseMissingPort(uuid::Uuid, NetworkInterfaceKind),
}

pub fn initialize_xde_driver(
    log: &Logger,
    _underlay_nics: &[AddrObject],
) -> Result<(), Error> {
    slog::warn!(log, "`xde` driver is a fiction on non-illumos systems");
    Ok(())
}

pub fn delete_all_xde_devices(log: &Logger) -> Result<(), Error> {
    slog::warn!(log, "`xde` driver is a fiction on non-illumos systems");
    Ok(())
}

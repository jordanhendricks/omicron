// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Interface for making API requests to the Oxide control plane at large
//! from within the control plane

use std::collections::HashMap;

progenitor::generate_api!(
    spec = "../openapi/nexus-internal.json",
    derives = [schemars::JsonSchema, PartialEq],
    inner_type = slog::Logger,
    pre_hook = (|log: &slog::Logger, request: &reqwest::Request| {
        slog::debug!(log, "client request";
            "method" => %request.method(),
            "uri" => %request.url(),
            "body" => ?&request.body(),
        );
    }),
    post_hook = (|log: &slog::Logger, result: &Result<_, _>| {
        slog::debug!(log, "client response"; "result" => ?result);
    }),
    replace = {
        Ipv4Network = ipnetwork::Ipv4Network,
        MacAddr = omicron_common::api::external::MacAddr,
        Name = omicron_common::api::external::Name,
        NewPasswordHash = omicron_passwords::NewPasswordHash,
    }
);

impl omicron_common::api::external::ClientError for types::Error {
    fn message(&self) -> String {
        self.message.clone()
    }
}

impl From<omicron_common::api::external::ByteCount> for types::ByteCount {
    fn from(s: omicron_common::api::external::ByteCount) -> Self {
        Self(s.to_bytes())
    }
}

impl From<types::DiskState> for omicron_common::api::external::DiskState {
    fn from(s: types::DiskState) -> Self {
        match s {
            types::DiskState::Creating => Self::Creating,
            types::DiskState::Detached => Self::Detached,
            types::DiskState::ImportReady => Self::ImportReady,
            types::DiskState::ImportingFromUrl => Self::ImportingFromUrl,
            types::DiskState::ImportingFromBulkWrites => {
                Self::ImportingFromBulkWrites
            }
            types::DiskState::Finalizing => Self::Finalizing,
            types::DiskState::Maintenance => Self::Maintenance,
            types::DiskState::Attaching(u) => Self::Attaching(u),
            types::DiskState::Attached(u) => Self::Attached(u),
            types::DiskState::Detaching(u) => Self::Detaching(u),
            types::DiskState::Destroyed => Self::Destroyed,
            types::DiskState::Faulted => Self::Faulted,
        }
    }
}

impl From<types::InstanceState>
    for omicron_common::api::external::InstanceState
{
    fn from(s: types::InstanceState) -> Self {
        match s {
            types::InstanceState::Creating => Self::Creating,
            types::InstanceState::Starting => Self::Starting,
            types::InstanceState::Running => Self::Running,
            types::InstanceState::Stopping => Self::Stopping,
            types::InstanceState::Stopped => Self::Stopped,
            types::InstanceState::Rebooting => Self::Rebooting,
            types::InstanceState::Migrating => Self::Migrating,
            types::InstanceState::Repairing => Self::Repairing,
            types::InstanceState::Failed => Self::Failed,
            types::InstanceState::Destroyed => Self::Destroyed,
        }
    }
}

impl From<omicron_common::api::internal::nexus::InstanceRuntimeState>
    for types::InstanceRuntimeState
{
    fn from(
        s: omicron_common::api::internal::nexus::InstanceRuntimeState,
    ) -> Self {
        Self {
            run_state: s.run_state.into(),
            sled_id: s.sled_id,
            propolis_id: s.propolis_id,
            dst_propolis_id: s.dst_propolis_id,
            propolis_addr: s.propolis_addr.map(|addr| addr.to_string()),
            migration_id: s.migration_id,
            propolis_gen: s.propolis_gen.into(),
            ncpus: s.ncpus.into(),
            memory: s.memory.into(),
            hostname: s.hostname,
            gen: s.gen.into(),
            time_updated: s.time_updated,
        }
    }
}

impl From<omicron_common::api::external::InstanceState>
    for types::InstanceState
{
    fn from(s: omicron_common::api::external::InstanceState) -> Self {
        use omicron_common::api::external::InstanceState;
        match s {
            InstanceState::Creating => Self::Creating,
            InstanceState::Starting => Self::Starting,
            InstanceState::Running => Self::Running,
            InstanceState::Stopping => Self::Stopping,
            InstanceState::Stopped => Self::Stopped,
            InstanceState::Rebooting => Self::Rebooting,
            InstanceState::Migrating => Self::Migrating,
            InstanceState::Repairing => Self::Repairing,
            InstanceState::Failed => Self::Failed,
            InstanceState::Destroyed => Self::Destroyed,
        }
    }
}

impl From<omicron_common::api::external::InstanceCpuCount>
    for types::InstanceCpuCount
{
    fn from(s: omicron_common::api::external::InstanceCpuCount) -> Self {
        Self(s.0)
    }
}

impl From<omicron_common::api::external::Generation> for types::Generation {
    fn from(s: omicron_common::api::external::Generation) -> Self {
        Self(i64::from(&s) as u64)
    }
}

impl From<omicron_common::api::internal::nexus::DiskRuntimeState>
    for types::DiskRuntimeState
{
    fn from(s: omicron_common::api::internal::nexus::DiskRuntimeState) -> Self {
        Self {
            disk_state: s.disk_state.into(),
            gen: s.gen.into(),
            time_updated: s.time_updated,
        }
    }
}

impl From<omicron_common::api::external::DiskState> for types::DiskState {
    fn from(s: omicron_common::api::external::DiskState) -> Self {
        use omicron_common::api::external::DiskState;
        match s {
            DiskState::Creating => Self::Creating,
            DiskState::Detached => Self::Detached,
            DiskState::ImportReady => Self::ImportReady,
            DiskState::ImportingFromUrl => Self::ImportingFromUrl,
            DiskState::ImportingFromBulkWrites => Self::ImportingFromBulkWrites,
            DiskState::Finalizing => Self::Finalizing,
            DiskState::Maintenance => Self::Maintenance,
            DiskState::Attaching(u) => Self::Attaching(u),
            DiskState::Attached(u) => Self::Attached(u),
            DiskState::Detaching(u) => Self::Detaching(u),
            DiskState::Destroyed => Self::Destroyed,
            DiskState::Faulted => Self::Faulted,
        }
    }
}

impl From<&types::InstanceState>
    for omicron_common::api::external::InstanceState
{
    fn from(state: &types::InstanceState) -> Self {
        match state {
            types::InstanceState::Creating => Self::Creating,
            types::InstanceState::Starting => Self::Starting,
            types::InstanceState::Running => Self::Running,
            types::InstanceState::Stopping => Self::Stopping,
            types::InstanceState::Stopped => Self::Stopped,
            types::InstanceState::Rebooting => Self::Rebooting,
            types::InstanceState::Migrating => Self::Migrating,
            types::InstanceState::Repairing => Self::Repairing,
            types::InstanceState::Failed => Self::Failed,
            types::InstanceState::Destroyed => Self::Destroyed,
        }
    }
}

impl From<&omicron_common::api::internal::nexus::ProducerEndpoint>
    for types::ProducerEndpoint
{
    fn from(
        s: &omicron_common::api::internal::nexus::ProducerEndpoint,
    ) -> Self {
        Self {
            address: s.address.to_string(),
            base_route: s.base_route.clone(),
            id: s.id,
            interval: s.interval.into(),
        }
    }
}

impl From<omicron_common::api::external::SemverVersion>
    for types::SemverVersion
{
    fn from(s: omicron_common::api::external::SemverVersion) -> Self {
        s.to_string().parse().expect(
            "semver should generate output that matches validation regex",
        )
    }
}

impl From<omicron_common::api::internal::nexus::KnownArtifactKind>
    for types::KnownArtifactKind
{
    fn from(
        s: omicron_common::api::internal::nexus::KnownArtifactKind,
    ) -> Self {
        use omicron_common::api::internal::nexus::KnownArtifactKind;

        match s {
            KnownArtifactKind::GimletSp => types::KnownArtifactKind::GimletSp,
            KnownArtifactKind::GimletRot => types::KnownArtifactKind::GimletRot,
            KnownArtifactKind::Host => types::KnownArtifactKind::Host,
            KnownArtifactKind::Trampoline => {
                types::KnownArtifactKind::Trampoline
            }
            KnownArtifactKind::ControlPlane => {
                types::KnownArtifactKind::ControlPlane
            }
            KnownArtifactKind::PscSp => types::KnownArtifactKind::PscSp,
            KnownArtifactKind::PscRot => types::KnownArtifactKind::PscRot,
            KnownArtifactKind::SwitchSp => types::KnownArtifactKind::SwitchSp,
            KnownArtifactKind::SwitchRot => types::KnownArtifactKind::SwitchRot,
        }
    }
}

impl From<std::time::Duration> for types::Duration {
    fn from(s: std::time::Duration) -> Self {
        Self { secs: s.as_secs(), nanos: s.subsec_nanos() }
    }
}

impl From<types::Duration> for std::time::Duration {
    fn from(s: types::Duration) -> Self {
        std::time::Duration::from_nanos(s.secs * 1000000000 + s.nanos as u64)
    }
}

impl From<omicron_common::address::IpRange> for types::IpRange {
    fn from(r: omicron_common::address::IpRange) -> Self {
        use omicron_common::address::IpRange;
        match r {
            IpRange::V4(r) => types::IpRange::V4(r.into()),
            IpRange::V6(r) => types::IpRange::V6(r.into()),
        }
    }
}

impl From<omicron_common::address::Ipv4Range> for types::Ipv4Range {
    fn from(r: omicron_common::address::Ipv4Range) -> Self {
        Self { first: r.first, last: r.last }
    }
}

impl From<omicron_common::address::Ipv6Range> for types::Ipv6Range {
    fn from(r: omicron_common::address::Ipv6Range) -> Self {
        Self { first: r.first, last: r.last }
    }
}

impl From<&omicron_common::api::internal::shared::SourceNatConfig>
    for types::SourceNatConfig
{
    fn from(
        r: &omicron_common::api::internal::shared::SourceNatConfig,
    ) -> Self {
        Self { ip: r.ip, first_port: r.first_port, last_port: r.last_port }
    }
}

impl From<omicron_common::api::internal::shared::PortSpeed>
    for types::PortSpeed
{
    fn from(value: omicron_common::api::internal::shared::PortSpeed) -> Self {
        match value {
            omicron_common::api::internal::shared::PortSpeed::Speed0G => {
                types::PortSpeed::Speed0G
            }
            omicron_common::api::internal::shared::PortSpeed::Speed1G => {
                types::PortSpeed::Speed1G
            }
            omicron_common::api::internal::shared::PortSpeed::Speed10G => {
                types::PortSpeed::Speed10G
            }
            omicron_common::api::internal::shared::PortSpeed::Speed25G => {
                types::PortSpeed::Speed25G
            }
            omicron_common::api::internal::shared::PortSpeed::Speed40G => {
                types::PortSpeed::Speed40G
            }
            omicron_common::api::internal::shared::PortSpeed::Speed50G => {
                types::PortSpeed::Speed50G
            }
            omicron_common::api::internal::shared::PortSpeed::Speed100G => {
                types::PortSpeed::Speed100G
            }
            omicron_common::api::internal::shared::PortSpeed::Speed200G => {
                types::PortSpeed::Speed200G
            }
            omicron_common::api::internal::shared::PortSpeed::Speed400G => {
                types::PortSpeed::Speed400G
            }
        }
    }
}

impl From<omicron_common::api::internal::shared::PortFec> for types::PortFec {
    fn from(value: omicron_common::api::internal::shared::PortFec) -> Self {
        match value {
            omicron_common::api::internal::shared::PortFec::Firecode => {
                types::PortFec::Firecode
            }
            omicron_common::api::internal::shared::PortFec::None => {
                types::PortFec::None
            }
            omicron_common::api::internal::shared::PortFec::Rs => {
                types::PortFec::Rs
            }
        }
    }
}

impl From<omicron_common::api::internal::shared::SwitchLocation>
    for types::SwitchLocation
{
    fn from(
        value: omicron_common::api::internal::shared::SwitchLocation,
    ) -> Self {
        match value {
            omicron_common::api::internal::shared::SwitchLocation::Switch0 => {
                types::SwitchLocation::Switch0
            }
            omicron_common::api::internal::shared::SwitchLocation::Switch1 => {
                types::SwitchLocation::Switch1
            }
        }
    }
}

impl From<omicron_common::api::internal::shared::ExternalPortDiscovery>
    for types::ExternalPortDiscovery
{
    fn from(
        value: omicron_common::api::internal::shared::ExternalPortDiscovery,
    ) -> Self {
        match value {
            omicron_common::api::internal::shared::ExternalPortDiscovery::Auto(val) => {
                let new: HashMap<_, _> = val.iter().map(|(slot, addr)| {
                    (slot.to_string(), *addr)
                }).collect();
                types::ExternalPortDiscovery::Auto(new)
            },
            omicron_common::api::internal::shared::ExternalPortDiscovery::Static(val) => {
                let new: HashMap<_, _> = val.iter().map(|(slot, ports)| {
                    (slot.to_string(), ports.clone())
                }).collect();
                types::ExternalPortDiscovery::Static(new)
            },
        }
    }
}

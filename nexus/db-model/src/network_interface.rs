// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::{MacAddr, VpcSubnet};
use crate::impl_enum_type;
use crate::schema::instance_network_interface;
use crate::schema::network_interface;
use crate::schema::service_network_interface;
use crate::Name;
use chrono::DateTime;
use chrono::Utc;
use db_macros::Resource;
use diesel::AsChangeset;
use nexus_types::external_api::params;
use nexus_types::identity::Resource;
use omicron_common::api::external;
use uuid::Uuid;

impl_enum_type! {
    #[derive(SqlType, QueryId, Debug, Clone, Copy)]
    #[diesel(postgres_type(name = "network_interface_kind"))]
    pub struct NetworkInterfaceKindEnum;

    #[derive(Clone, Copy, Debug, AsExpression, FromSqlRow, PartialEq)]
    #[diesel(sql_type = NetworkInterfaceKindEnum)]
    pub enum NetworkInterfaceKind;

    Instance => b"instance"
    Service => b"service"
}

/// Generic Network Interface DB model.
#[derive(Selectable, Queryable, Clone, Debug, Resource)]
#[diesel(table_name = network_interface)]
pub struct NetworkInterface {
    #[diesel(embed)]
    pub identity: NetworkInterfaceIdentity,

    pub kind: NetworkInterfaceKind,
    pub parent_id: Uuid,

    pub vpc_id: Uuid,
    pub subnet_id: Uuid,

    pub mac: MacAddr,
    // TODO-correctness: We need to split this into an optional V4 and optional V6 address, at
    // least one of which will always be specified.
    //
    // If user requests an address of either kind, give exactly that and not the other.
    // If neither is specified, auto-assign one of each?
    pub ip: ipnetwork::IpNetwork,

    pub slot: i16,
    #[diesel(column_name = is_primary)]
    pub primary: bool,
}

/// Instance Network Interface DB model.
///
/// The underlying "table" (`instance_network_interface`) is actually a view
/// over the `network_interface` table, that contains only rows with
/// `kind = 'instance'`.
#[derive(Selectable, Queryable, Clone, Debug, Resource)]
#[diesel(table_name = instance_network_interface)]
pub struct InstanceNetworkInterface {
    #[diesel(embed)]
    pub identity: InstanceNetworkInterfaceIdentity,

    pub instance_id: Uuid,
    pub vpc_id: Uuid,
    pub subnet_id: Uuid,

    pub mac: MacAddr,
    pub ip: ipnetwork::IpNetwork,

    pub slot: i16,
    #[diesel(column_name = is_primary)]
    pub primary: bool,
}

/// Service Network Interface DB model.
///
/// The underlying "table" (`service_network_interface`) is actually a view
/// over the `network_interface` table, that contains only rows with
/// `kind = 'service'`.
#[derive(Selectable, Queryable, Clone, Debug, Resource)]
#[diesel(table_name = service_network_interface)]
pub struct ServiceNetworkInterface {
    #[diesel(embed)]
    pub identity: ServiceNetworkInterfaceIdentity,

    pub service_id: Uuid,
    pub vpc_id: Uuid,
    pub subnet_id: Uuid,

    pub mac: MacAddr,
    pub ip: ipnetwork::IpNetwork,

    pub slot: i16,
    #[diesel(column_name = is_primary)]
    pub primary: bool,
}

impl NetworkInterface {
    /// Treat this `NetworkInterface` as an `InstanceNetworkInterface`.
    ///
    /// # Panics
    /// Panics if this isn't an 'instance' kind network interface.
    pub fn as_instance(self) -> InstanceNetworkInterface {
        assert_eq!(self.kind, NetworkInterfaceKind::Instance);
        InstanceNetworkInterface {
            identity: InstanceNetworkInterfaceIdentity {
                id: self.identity.id,
                name: self.identity.name,
                description: self.identity.description,
                time_created: self.identity.time_created,
                time_modified: self.identity.time_modified,
                time_deleted: self.identity.time_deleted,
            },
            instance_id: self.parent_id,
            vpc_id: self.vpc_id,
            subnet_id: self.subnet_id,
            mac: self.mac,
            ip: self.ip,
            slot: self.slot,
            primary: self.primary,
        }
    }

    /// Treat this `NetworkInterface` as a `ServiceNetworkInterface`.
    ///
    /// # Panics
    /// Panics if this isn't a 'service' kind network interface.
    pub fn as_service(self) -> ServiceNetworkInterface {
        assert_eq!(self.kind, NetworkInterfaceKind::Service);
        ServiceNetworkInterface {
            identity: ServiceNetworkInterfaceIdentity {
                id: self.identity.id,
                name: self.identity.name,
                description: self.identity.description,
                time_created: self.identity.time_created,
                time_modified: self.identity.time_modified,
                time_deleted: self.identity.time_deleted,
            },
            service_id: self.parent_id,
            vpc_id: self.vpc_id,
            subnet_id: self.subnet_id,
            mac: self.mac,
            ip: self.ip,
            slot: self.slot,
            primary: self.primary,
        }
    }
}

impl From<InstanceNetworkInterface> for NetworkInterface {
    fn from(iface: InstanceNetworkInterface) -> Self {
        NetworkInterface {
            identity: NetworkInterfaceIdentity {
                id: iface.identity.id,
                name: iface.identity.name,
                description: iface.identity.description,
                time_created: iface.identity.time_created,
                time_modified: iface.identity.time_modified,
                time_deleted: iface.identity.time_deleted,
            },
            kind: NetworkInterfaceKind::Instance,
            parent_id: iface.instance_id,
            vpc_id: iface.vpc_id,
            subnet_id: iface.subnet_id,
            mac: iface.mac,
            ip: iface.ip,
            slot: iface.slot,
            primary: iface.primary,
        }
    }
}

impl From<ServiceNetworkInterface> for NetworkInterface {
    fn from(iface: ServiceNetworkInterface) -> Self {
        NetworkInterface {
            identity: NetworkInterfaceIdentity {
                id: iface.identity.id,
                name: iface.identity.name,
                description: iface.identity.description,
                time_created: iface.identity.time_created,
                time_modified: iface.identity.time_modified,
                time_deleted: iface.identity.time_deleted,
            },
            kind: NetworkInterfaceKind::Service,
            parent_id: iface.service_id,
            vpc_id: iface.vpc_id,
            subnet_id: iface.subnet_id,
            mac: iface.mac,
            ip: iface.ip,
            slot: iface.slot,
            primary: iface.primary,
        }
    }
}

/// A not fully constructed NetworkInterface. It may not yet have an IP
/// address allocated.
#[derive(Clone, Debug)]
pub struct IncompleteNetworkInterface {
    pub identity: NetworkInterfaceIdentity,
    pub kind: NetworkInterfaceKind,
    pub parent_id: Uuid,
    pub subnet: VpcSubnet,
    pub ip: Option<std::net::IpAddr>,
    pub mac: Option<external::MacAddr>,
}

impl IncompleteNetworkInterface {
    fn new(
        interface_id: Uuid,
        kind: NetworkInterfaceKind,
        parent_id: Uuid,
        subnet: VpcSubnet,
        identity: external::IdentityMetadataCreateParams,
        ip: Option<std::net::IpAddr>,
        mac: Option<external::MacAddr>,
    ) -> Result<Self, external::Error> {
        if let Some(ip) = ip {
            subnet.check_requestable_addr(ip)?;
        };
        match (mac, kind) {
            (Some(mac), NetworkInterfaceKind::Instance) if !mac.is_guest() => {
                return Err(external::Error::invalid_request(&format!(
                    "invalid MAC address {} for guest NIC",
                    mac
                )));
            }
            (Some(mac), NetworkInterfaceKind::Service) if !mac.is_system() => {
                return Err(external::Error::invalid_request(&format!(
                    "invalid MAC address {} for service NIC",
                    mac
                )));
            }
            _ => {}
        }
        let identity = NetworkInterfaceIdentity::new(interface_id, identity);
        Ok(IncompleteNetworkInterface {
            identity,
            kind,
            parent_id,
            subnet,
            ip,
            mac,
        })
    }

    pub fn new_instance(
        interface_id: Uuid,
        instance_id: Uuid,
        subnet: VpcSubnet,
        identity: external::IdentityMetadataCreateParams,
        ip: Option<std::net::IpAddr>,
    ) -> Result<Self, external::Error> {
        Self::new(
            interface_id,
            NetworkInterfaceKind::Instance,
            instance_id,
            subnet,
            identity,
            ip,
            None,
        )
    }

    pub fn new_service(
        interface_id: Uuid,
        service_id: Uuid,
        subnet: VpcSubnet,
        identity: external::IdentityMetadataCreateParams,
        ip: Option<std::net::IpAddr>,
        mac: Option<external::MacAddr>,
    ) -> Result<Self, external::Error> {
        Self::new(
            interface_id,
            NetworkInterfaceKind::Service,
            service_id,
            subnet,
            identity,
            ip,
            mac,
        )
    }
}

/// Describes a set of updates for the [`NetworkInterface`] model.
#[derive(AsChangeset, Debug, Clone)]
#[diesel(table_name = network_interface)]
pub struct NetworkInterfaceUpdate {
    pub name: Option<Name>,
    pub description: Option<String>,
    pub time_modified: DateTime<Utc>,
    #[diesel(column_name = is_primary)]
    pub primary: Option<bool>,
}

impl From<InstanceNetworkInterface> for external::InstanceNetworkInterface {
    fn from(iface: InstanceNetworkInterface) -> Self {
        Self {
            identity: iface.identity(),
            instance_id: iface.instance_id,
            vpc_id: iface.vpc_id,
            subnet_id: iface.subnet_id,
            ip: iface.ip.ip(),
            mac: *iface.mac,
            primary: iface.primary,
        }
    }
}

impl From<params::InstanceNetworkInterfaceUpdate> for NetworkInterfaceUpdate {
    fn from(params: params::InstanceNetworkInterfaceUpdate) -> Self {
        let primary = if params.primary { Some(true) } else { None };
        Self {
            name: params.identity.name.map(|n| n.into()),
            description: params.identity.description,
            time_modified: Utc::now(),
            primary,
        }
    }
}

use futures_util::TryStreamExt;
use rtnetlink::{LinkVeth, new_connection};
use std::{io, net::Ipv4Addr};
use tokio::runtime::Runtime;

const CONTAINER_INTERFACE: &str = "eth0";
const HOST_IP: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 1);
const CONTAINER_IP: Ipv4Addr = Ipv4Addr::new(10, 0, 0, 2);

#[allow(dead_code)]
pub struct NetworkConfig {
    pub host_interface: String,
    pub container_interface: String,
    pub container_ip: std::net::Ipv4Addr,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum NetworkError {
    Io(io::Error),
    Netlink(rtnetlink::Error),
    InterfaceNotFound(String),
    Namespace(String),
}

impl From<io::Error> for NetworkError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rtnetlink::Error> for NetworkError {
    fn from(error: rtnetlink::Error) -> Self {
        Self::Netlink(error)
    }
}

pub fn setup_network(container_pid: libc::pid_t) -> Result<NetworkConfig, NetworkError> {
    let host_interface = format!("veth{}", container_pid);
    let peer_interface = format!("veth_c{}", container_pid);
    let runtime = Runtime::new().map_err(NetworkError::Io)?;

    runtime.block_on(async {
        let (connection, handle, _) = new_connection()?;
        tokio::spawn(connection);

        // 1. Create veth pair using temporary peer name in host namespace
        handle
            .link()
            .add(LinkVeth::new(&host_interface, &peer_interface).build())
            .execute()
            .await?;

        // 2. Locate the peer interface index
        let mut links = handle.link().get().match_name(&peer_interface).execute();

        let container_link = links
            .try_next()
            .await?
            .ok_or_else(|| NetworkError::InterfaceNotFound(peer_interface.clone()))?;

        let container_index = container_link.header.index;

        // 3. Move peer interface into container's network namespace
        handle
            .link()
            .change(
                rtnetlink::LinkMessageBuilder::<rtnetlink::LinkUnspec>::new()
                    .index(container_index)
                    .setns_by_pid(container_pid as u32)
                    .build(),
            )
            .execute()
            .await?;

        // 4. Find host-side interface index
        let mut links = handle.link().get().match_name(&host_interface).execute();

        let host_link = links
            .try_next()
            .await?
            .ok_or_else(|| NetworkError::InterfaceNotFound(host_interface.clone()))?;

        let host_index = host_link.header.index;

        // 5. Assign IP to host side interface
        handle
            .address()
            .add(host_index, HOST_IP.into(), 24)
            .execute()
            .await?;

        // 6. Bring host side interface UP
        handle
            .link()
            .change(
                rtnetlink::LinkMessageBuilder::<rtnetlink::LinkUnspec>::new()
                    .index(host_index)
                    .up()
                    .build(),
            )
            .execute()
            .await?;

        Ok(NetworkConfig {
            host_interface,
            container_interface: CONTAINER_INTERFACE.to_string(),
            container_ip: CONTAINER_IP,
        })
    })
}

pub fn configure_container_network() -> Result<(), NetworkError> {
    let runtime = Runtime::new().map_err(NetworkError::Io)?;

    runtime.block_on(async {
        let (connection, handle, _) = new_connection()?;
        tokio::spawn(connection);

        // 1. Get all interfaces in child namespace to locate the moved veth interface
        let mut links = handle.link().get().execute();
        let mut peer_index = None;

        while let Some(link) = links.try_next().await? {
            // Find the non-loopback interface moved from host
            if link.header.index != 1 {
                peer_index = Some(link.header.index);
                break;
            }
        }

        let index = peer_index.ok_or_else(|| {
            NetworkError::InterfaceNotFound("Container peer interface missing".to_string())
        })?;

        // 2. Rename moved veth interface to "eth0"
        handle
            .link()
            .change(
                rtnetlink::LinkMessageBuilder::<rtnetlink::LinkUnspec>::new()
                    .index(index)
                    .name(CONTAINER_INTERFACE.to_string())
                    .build(),
            )
            .execute()
            .await?;

        // 3. Assign IP address (10.0.0.2/24)
        handle
            .address()
            .add(index, CONTAINER_IP.into(), 24)
            .execute()
            .await?;

        // 4. Bring eth0 UP
        handle
            .link()
            .change(
                rtnetlink::LinkMessageBuilder::<rtnetlink::LinkUnspec>::new()
                    .index(index)
                    .up()
                    .build(),
            )
            .execute()
            .await?;

        Ok(())
    })
}

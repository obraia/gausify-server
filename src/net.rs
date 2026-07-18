//! Local network address discovery, for the startup banner and cert SANs.

use std::net::{IpAddr, Ipv4Addr};

/// Non-loopback, non-link-local IPv4 addresses of this host, sorted & deduped.
pub fn local_ipv4s() -> Vec<Ipv4Addr> {
    let mut out: Vec<Ipv4Addr> = Vec::new();
    if let Ok(list) = local_ip_address::list_afinet_netifas() {
        for (_name, ip) in list {
            if let IpAddr::V4(v4) = ip {
                if !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified() {
                    out.push(v4);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

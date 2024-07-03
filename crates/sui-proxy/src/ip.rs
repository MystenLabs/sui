// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use ipnetwork::IpNetwork;
use multiaddr::Multiaddr;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn to_multiaddr(addr: IpAddr) -> Multiaddr {
    match addr {
        IpAddr::V4(a) => Multiaddr::from(a),
        IpAddr::V6(a) => Multiaddr::from(a),
    }
}

/// is_private makes a decent guess at determining of an addr is publicly routable.
pub fn is_private(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(a) => is_private_v4(a),
        IpAddr::V6(a) => is_private_v6(a),
    }
}

/// is_private_v4 will say just that, is it private? we ignore 169.254.0.0/16 in this consideration
fn is_private_v4(addr: Ipv4Addr) -> bool {
    // special case we will allow
    let allowed_private: IpNetwork = "169.254.0.0/16".parse().unwrap();
    if allowed_private.contains(IpAddr::V4(addr)) {
        // intentional
        return false;
    }
    addr.is_private()
}

/// is_private_v6 and the funcs below are based on an unstable const fn in core. yoinked it.
/// taken from https://github.com/rust-lang/rust/blob/340bb19fea20fd5f9357bbfac542fad84fc7ea2b/library/core/src/net/ip_addr.rs#L691-L783
#[allow(clippy::manual_range_contains)]
fn is_private_v6(addr: Ipv6Addr) -> bool {
    addr.is_unspecified()
        || addr.is_loopback()
        // IPv4-mapped Address (`::ffff:0:0/96`)
        || matches!(addr.segments(), [0, 0, 0, 0, 0, 0xffff, _, _])
        // IPv4-IPv6 Translat. (`64:ff9b:1::/48`)
        || matches!(addr.segments(), [0x64, 0xff9b, 1, _, _, _, _, _])
        // Discard-Only Address Block (`100::/64`)
        || matches!(addr.segments(), [0x100, 0, 0, 0, _, _, _, _])
        // IETF Protocol Assignments (`2001::/23`)
        || (matches!(addr.segments(), [0x2001, b, _, _, _, _, _, _] if b < 0x200)
            && !(
                // Port Control Protocol Anycast (`2001:1::1`)
                u128::from_be_bytes(addr.octets()) == 0x2001_0001_0000_0000_0000_0000_0000_0001
                // Traversal Using Relays around NAT Anycast (`2001:1::2`)
                || u128::from_be_bytes(addr.octets()) == 0x2001_0001_0000_0000_0000_0000_0000_0002
                // AMT (`2001:3::/32`)
                || matches!(addr.segments(), [0x2001, 3, _, _, _, _, _, _])
                // AS112-v6 (`2001:4:112::/48`)
                || matches!(addr.segments(), [0x2001, 4, 0x112, _, _, _, _, _])
                // ORCHIDv2 (`2001:20::/28`)
                // Drone Remote ID Protocol Entity Tags (DETs) Prefix (`2001:30::/28`)`
                || matches!(addr.segments(), [0x2001, b, _, _, _, _, _, _] if b >= 0x20 && b <= 0x3F)
            ))
        // 6to4 (`2002::/16`) – it's not explicitly documented as globally reachable,
        // IANA says N/A.
        || matches!(addr.segments(), [0x2002, _, _, _, _, _, _, _])
        || is_documentation(&addr)
        || is_unique_local(&addr)
        || is_unicast_link_local(&addr)
}

fn is_documentation(addr: &Ipv6Addr) -> bool {
    (addr.segments()[0] == 0x2001) && (addr.segments()[1] == 0xdb8)
}
fn is_unique_local(addr: &Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xfe00) == 0xfc00
}
fn is_unicast_link_local(addr: &Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xffc0) == 0xfe80
}

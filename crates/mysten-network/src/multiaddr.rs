// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use eyre::{eyre, Result};
use std::{
    borrow::Cow,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};
use tracing::error;

pub use ::multiaddr::Error;
pub use ::multiaddr::Protocol;

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Multiaddr(::multiaddr::Multiaddr);

impl Multiaddr {
    pub fn empty() -> Self {
        Self(::multiaddr::Multiaddr::empty())
    }

    #[cfg(test)]
    pub(crate) fn new_internal(inner: ::multiaddr::Multiaddr) -> Self {
        Self(inner)
    }

    pub fn iter(&self) -> ::multiaddr::Iter<'_> {
        self.0.iter()
    }

    pub fn pop<'a>(&mut self) -> Option<Protocol<'a>> {
        self.0.pop()
    }

    pub fn push(&mut self, p: Protocol<'_>) {
        self.0.push(p)
    }

    pub fn replace<'a, F>(&self, at: usize, by: F) -> Option<Multiaddr>
    where
        F: FnOnce(&Protocol<'_>) -> Option<Protocol<'a>>,
    {
        self.0.replace(at, by).map(Self)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Attempts to convert a multiaddr of the form `/[ip4,ip6,dns]/{}/udp/{port}` into an anemo
    /// address
    pub fn to_anemo_address(&self) -> Result<anemo::types::Address, &'static str> {
        let mut iter = self.iter();

        match (iter.next(), iter.next()) {
            (Some(Protocol::Ip4(ipaddr)), Some(Protocol::Udp(port))) => Ok((ipaddr, port).into()),
            (Some(Protocol::Ip6(ipaddr)), Some(Protocol::Udp(port))) => Ok((ipaddr, port).into()),
            (Some(Protocol::Dns(hostname)), Some(Protocol::Udp(port))) => {
                Ok((hostname.as_ref(), port).into())
            }

            _ => {
                tracing::warn!("unsupported p2p multiaddr: '{self}'");
                Err("invalid address")
            }
        }
    }

    pub fn udp_multiaddr_to_listen_address(&self) -> Option<std::net::SocketAddr> {
        let mut iter = self.iter();

        match (iter.next(), iter.next()) {
            (Some(Protocol::Ip4(ipaddr)), Some(Protocol::Udp(port))) => Some((ipaddr, port).into()),
            (Some(Protocol::Ip6(ipaddr)), Some(Protocol::Udp(port))) => Some((ipaddr, port).into()),

            (Some(Protocol::Dns(_)), Some(Protocol::Udp(port))) => {
                Some((std::net::Ipv4Addr::UNSPECIFIED, port).into())
            }

            _ => None,
        }
    }

    // Converts a /ip{4,6}/-/tcp/-[/-] Multiaddr to SocketAddr.
    // Useful when an external library only accepts SocketAddr, e.g. to start a local server.
    // See `client::endpoint_from_multiaddr()` for converting to Endpoint for clients.
    pub fn to_socket_addr(&self) -> Result<SocketAddr> {
        let mut iter = self.iter();
        let ip = match iter.next().ok_or_else(|| {
            eyre!("failed to convert to SocketAddr: Multiaddr does not contain IP")
        })? {
            Protocol::Ip4(ip4_addr) => IpAddr::V4(ip4_addr),
            Protocol::Ip6(ip6_addr) => IpAddr::V6(ip6_addr),
            unsupported => return Err(eyre!("unsupported protocol {unsupported}")),
        };
        let tcp_port = parse_tcp(&mut iter)?;
        Ok(SocketAddr::new(ip, tcp_port))
    }

    // Returns true if the third component in the multiaddr is `Protocol::Tcp`
    pub fn is_loosely_valid_tcp_addr(&self) -> bool {
        let mut iter = self.iter();
        iter.next(); // Skip the ip/dns part
        match iter.next() {
            Some(Protocol::Tcp(_)) => true,
            _ => false, // including `None` and `Some(other)`
        }
    }

    /// Set the ip address to `0.0.0.0`. For instance, it converts the following address
    /// `/ip4/155.138.174.208/tcp/1500/http` into `/ip4/0.0.0.0/tcp/1500/http`.
    /// This is useful when starting a server and you want to listen on all interfaces.
    pub fn with_zero_ip(&self) -> Self {
        let mut new_address = self.0.clone();
        let Some(protocol) = new_address.iter().next() else {
            error!("Multiaddr is empty");
            return Self(new_address);
        };
        match protocol {
            multiaddr::Protocol::Ip4(_)
            | multiaddr::Protocol::Dns(_)
            | multiaddr::Protocol::Dns4(_) => {
                new_address = new_address
                    .replace(0, |_| Some(multiaddr::Protocol::Ip4(Ipv4Addr::UNSPECIFIED)))
                    .unwrap();
            }
            multiaddr::Protocol::Ip6(_) | multiaddr::Protocol::Dns6(_) => {
                new_address = new_address
                    .replace(0, |_| Some(multiaddr::Protocol::Ip6(Ipv6Addr::UNSPECIFIED)))
                    .unwrap();
            }
            p => {
                error!("Unsupported protocol {} in Multiaddr {}!", p, new_address);
            }
        }
        Self(new_address)
    }

    /// Set the ip address to `127.0.0.1`. For instance, it converts the following address
    /// `/ip4/155.138.174.208/tcp/1500/http` into `/ip4/127.0.0.1/tcp/1500/http`.
    pub fn with_localhost_ip(&self) -> Self {
        let mut new_address = self.0.clone();
        let Some(protocol) = new_address.iter().next() else {
            error!("Multiaddr is empty");
            return Self(new_address);
        };
        match protocol {
            multiaddr::Protocol::Ip4(_)
            | multiaddr::Protocol::Dns(_)
            | multiaddr::Protocol::Dns4(_) => {
                new_address = new_address
                    .replace(0, |_| Some(multiaddr::Protocol::Ip4(Ipv4Addr::LOCALHOST)))
                    .unwrap();
            }
            multiaddr::Protocol::Ip6(_) | multiaddr::Protocol::Dns6(_) => {
                new_address = new_address
                    .replace(0, |_| Some(multiaddr::Protocol::Ip6(Ipv6Addr::LOCALHOST)))
                    .unwrap();
            }
            p => {
                error!("Unsupported protocol {} in Multiaddr {}!", p, new_address);
            }
        }
        Self(new_address)
    }

    pub fn is_localhost_ip(&self) -> bool {
        let Some(protocol) = self.0.iter().next() else {
            error!("Multiaddr is empty");
            return false;
        };
        match protocol {
            multiaddr::Protocol::Ip4(addr) => addr == Ipv4Addr::LOCALHOST,
            multiaddr::Protocol::Ip6(addr) => addr == Ipv6Addr::LOCALHOST,
            _ => false,
        }
    }

    pub fn hostname(&self) -> Option<String> {
        for component in self.iter() {
            match component {
                Protocol::Ip4(ip) => return Some(ip.to_string()),
                Protocol::Ip6(ip) => return Some(ip.to_string()),
                Protocol::Dns(dns) => return Some(dns.to_string()),
                _ => (),
            }
        }
        None
    }

    pub fn port(&self) -> Option<u16> {
        for component in self.iter() {
            match component {
                Protocol::Udp(port) | Protocol::Tcp(port) => return Some(port),
                _ => (),
            }
        }
        None
    }

    pub fn rewrite_udp_to_tcp(&self) -> Self {
        let mut new = Self::empty();

        for component in self.iter() {
            if let Protocol::Udp(port) = component {
                new.push(Protocol::Tcp(port));
            } else {
                new.push(component);
            }
        }

        new
    }
}

impl std::fmt::Display for Multiaddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::str::FromStr for Multiaddr {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ::multiaddr::Multiaddr::from_str(s).map(Self)
    }
}

impl<'a> TryFrom<&'a str> for Multiaddr {
    type Error = Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for Multiaddr {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl serde::Serialize for Multiaddr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Multiaddr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse()
            .map(Self)
            .map_err(|e| serde::de::Error::custom(e.to_string()))
    }
}

impl std::net::ToSocketAddrs for Multiaddr {
    type Iter = Box<dyn Iterator<Item = SocketAddr>>;

    fn to_socket_addrs(&self) -> std::io::Result<Self::Iter> {
        let mut iter = self.iter();

        match (iter.next(), iter.next()) {
            (Some(Protocol::Ip4(ip4)), Some(Protocol::Tcp(port) | Protocol::Udp(port))) => {
                (ip4, port)
                    .to_socket_addrs()
                    .map(|iter| Box::new(iter) as _)
            }
            (Some(Protocol::Ip6(ip6)), Some(Protocol::Tcp(port) | Protocol::Udp(port))) => {
                (ip6, port)
                    .to_socket_addrs()
                    .map(|iter| Box::new(iter) as _)
            }
            (Some(Protocol::Dns(hostname)), Some(Protocol::Tcp(port) | Protocol::Udp(port))) => {
                (hostname.as_ref(), port)
                    .to_socket_addrs()
                    .map(|iter| Box::new(iter) as _)
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "unable to convert Multiaddr to SocketAddr",
            )),
        }
    }
}

pub(crate) fn parse_tcp<'a, T: Iterator<Item = Protocol<'a>>>(protocols: &mut T) -> Result<u16> {
    if let Protocol::Tcp(port) = protocols
        .next()
        .ok_or_else(|| eyre!("unexpected end of multiaddr"))?
    {
        Ok(port)
    } else {
        Err(eyre!("expected tcp protocol"))
    }
}

pub(crate) fn parse_http_https<'a, T: Iterator<Item = Protocol<'a>>>(
    protocols: &mut T,
) -> Result<&'static str> {
    match protocols.next() {
        Some(Protocol::Http) => Ok("http"),
        Some(Protocol::Https) => Ok("https"),
        _ => Ok("http"),
    }
}

pub(crate) fn parse_end<'a, T: Iterator<Item = Protocol<'a>>>(protocols: &mut T) -> Result<()> {
    if protocols.next().is_none() {
        Ok(())
    } else {
        Err(eyre!("expected end of multiaddr"))
    }
}

// Parse a full /dns/-/tcp/-/{http,https} address
pub(crate) fn parse_dns(address: &Multiaddr) -> Result<(Cow<'_, str>, u16, &'static str)> {
    let mut iter = address.iter();

    let dns_name = match iter
        .next()
        .ok_or_else(|| eyre!("unexpected end of multiaddr"))?
    {
        Protocol::Dns(dns_name) => dns_name,
        other => return Err(eyre!("expected dns found {other}")),
    };
    let tcp_port = parse_tcp(&mut iter)?;
    let http_or_https = parse_http_https(&mut iter)?;
    parse_end(&mut iter)?;
    Ok((dns_name, tcp_port, http_or_https))
}

// Parse a full /ip4/-/tcp/-/{http,https} address
pub(crate) fn parse_ip4(address: &Multiaddr) -> Result<(SocketAddr, &'static str)> {
    let mut iter = address.iter();

    let ip_addr = match iter
        .next()
        .ok_or_else(|| eyre!("unexpected end of multiaddr"))?
    {
        Protocol::Ip4(ip4_addr) => IpAddr::V4(ip4_addr),
        other => return Err(eyre!("expected ip4 found {other}")),
    };
    let tcp_port = parse_tcp(&mut iter)?;
    let http_or_https = parse_http_https(&mut iter)?;
    parse_end(&mut iter)?;
    let socket_addr = SocketAddr::new(ip_addr, tcp_port);

    Ok((socket_addr, http_or_https))
}

// Parse a full /ip6/-/tcp/-/{http,https} address
pub(crate) fn parse_ip6(address: &Multiaddr) -> Result<(SocketAddr, &'static str)> {
    let mut iter = address.iter();

    let ip_addr = match iter
        .next()
        .ok_or_else(|| eyre!("unexpected end of multiaddr"))?
    {
        Protocol::Ip6(ip6_addr) => IpAddr::V6(ip6_addr),
        other => return Err(eyre!("expected ip6 found {other}")),
    };
    let tcp_port = parse_tcp(&mut iter)?;
    let http_or_https = parse_http_https(&mut iter)?;
    parse_end(&mut iter)?;
    let socket_addr = SocketAddr::new(ip_addr, tcp_port);

    Ok((socket_addr, http_or_https))
}

#[cfg(test)]
mod test {
    use super::Multiaddr;
    use multiaddr::multiaddr;

    #[test]
    fn test_to_socket_addr_basic() {
        let multi_addr_ipv4 = Multiaddr(multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10500u16)));
        let socket_addr_ipv4 = multi_addr_ipv4
            .to_socket_addr()
            .expect("Couldn't convert to socket addr");
        assert_eq!(socket_addr_ipv4.to_string(), "127.0.0.1:10500");

        let multi_addr_ipv6 = Multiaddr(multiaddr!(Ip6([172, 0, 0, 1, 1, 1, 1, 1]), Tcp(10500u16)));
        let socket_addr_ipv6 = multi_addr_ipv6
            .to_socket_addr()
            .expect("Couldn't convert to socket addr");
        assert_eq!(socket_addr_ipv6.to_string(), "[ac::1:1:1:1:1]:10500");
    }

    #[test]
    fn test_to_socket_addr_unsupported_protocol() {
        let multi_addr_dns = Multiaddr(multiaddr!(Dnsaddr("mysten.sui"), Tcp(10500u16)));
        let _ = multi_addr_dns
            .to_socket_addr()
            .expect_err("DNS is unsupported");
    }

    #[test]
    fn test_is_loosely_valid_tcp_addr() {
        let multi_addr_ipv4 = Multiaddr(multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10500u16)));
        assert!(multi_addr_ipv4.is_loosely_valid_tcp_addr());
        let multi_addr_ipv6 = Multiaddr(multiaddr!(Ip6([172, 0, 0, 1, 1, 1, 1, 1]), Tcp(10500u16)));
        assert!(multi_addr_ipv6.is_loosely_valid_tcp_addr());
        let multi_addr_dns = Multiaddr(multiaddr!(Dnsaddr("mysten.sui"), Tcp(10500u16)));
        assert!(multi_addr_dns.is_loosely_valid_tcp_addr());

        let multi_addr_ipv4 = Multiaddr(multiaddr!(Ip4([127, 0, 0, 1]), Udp(10500u16)));
        assert!(!multi_addr_ipv4.is_loosely_valid_tcp_addr());
        let multi_addr_ipv6 = Multiaddr(multiaddr!(Ip6([172, 0, 0, 1, 1, 1, 1, 1]), Udp(10500u16)));
        assert!(!multi_addr_ipv6.is_loosely_valid_tcp_addr());
        let multi_addr_dns = Multiaddr(multiaddr!(Dnsaddr("mysten.sui"), Udp(10500u16)));
        assert!(!multi_addr_dns.is_loosely_valid_tcp_addr());

        let invalid_multi_addr_ipv4 = Multiaddr(multiaddr!(Ip4([127, 0, 0, 1])));
        assert!(!invalid_multi_addr_ipv4.is_loosely_valid_tcp_addr());
    }

    #[test]
    fn test_get_hostname_port() {
        let multi_addr_ip4 = Multiaddr(multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10500u16)));
        assert_eq!(Some("127.0.0.1".to_string()), multi_addr_ip4.hostname());
        assert_eq!(Some(10500u16), multi_addr_ip4.port());

        let multi_addr_dns = Multiaddr(multiaddr!(Dns("mysten.sui"), Tcp(10501u16)));
        assert_eq!(Some("mysten.sui".to_string()), multi_addr_dns.hostname());
        assert_eq!(Some(10501u16), multi_addr_dns.port());
    }

    #[test]
    fn test_to_anemo_address() {
        let addr_ip4 = Multiaddr(multiaddr!(Ip4([15, 15, 15, 1]), Udp(10500u16)))
            .to_anemo_address()
            .unwrap();
        assert_eq!("15.15.15.1:10500".to_string(), addr_ip4.to_string());

        let addr_ip6 = Multiaddr(multiaddr!(
            Ip6([15, 15, 15, 15, 15, 15, 15, 1]),
            Udp(10500u16)
        ))
        .to_anemo_address()
        .unwrap();
        assert_eq!("[f:f:f:f:f:f:f:1]:10500".to_string(), addr_ip6.to_string());

        let addr_dns = Multiaddr(multiaddr!(Dns("mysten.sui"), Udp(10501u16)))
            .to_anemo_address()
            .unwrap();
        assert_eq!("mysten.sui:10501".to_string(), addr_dns.to_string());

        let addr_invalid =
            Multiaddr(multiaddr!(Dns("mysten.sui"), Tcp(10501u16))).to_anemo_address();
        assert!(addr_invalid.is_err());
    }

    #[test]
    fn test_with_zero_ip() {
        let multi_addr_ip4 =
            Multiaddr(multiaddr!(Ip4([15, 15, 15, 1]), Tcp(10500u16))).with_zero_ip();
        assert_eq!(Some("0.0.0.0".to_string()), multi_addr_ip4.hostname());
        assert_eq!(Some(10500u16), multi_addr_ip4.port());

        let multi_addr_ip6 = Multiaddr(multiaddr!(
            Ip6([15, 15, 15, 15, 15, 15, 15, 1]),
            Tcp(10500u16)
        ))
        .with_zero_ip();
        assert_eq!(Some("::".to_string()), multi_addr_ip6.hostname());
        assert_eq!(Some(10500u16), multi_addr_ip4.port());

        let multi_addr_dns = Multiaddr(multiaddr!(Dns("mysten.sui"), Tcp(10501u16))).with_zero_ip();
        assert_eq!(Some("0.0.0.0".to_string()), multi_addr_dns.hostname());
        assert_eq!(Some(10501u16), multi_addr_dns.port());
    }

    #[test]
    fn test_with_localhost_ip() {
        let multi_addr_ip4 =
            Multiaddr(multiaddr!(Ip4([15, 15, 15, 1]), Tcp(10500u16))).with_localhost_ip();
        assert_eq!(Some("127.0.0.1".to_string()), multi_addr_ip4.hostname());
        assert_eq!(Some(10500u16), multi_addr_ip4.port());

        let multi_addr_ip6 = Multiaddr(multiaddr!(
            Ip6([15, 15, 15, 15, 15, 15, 15, 1]),
            Tcp(10500u16)
        ))
        .with_localhost_ip();
        assert_eq!(Some("::1".to_string()), multi_addr_ip6.hostname());
        assert_eq!(Some(10500u16), multi_addr_ip4.port());

        let multi_addr_dns =
            Multiaddr(multiaddr!(Dns("mysten.sui"), Tcp(10501u16))).with_localhost_ip();
        assert_eq!(Some("127.0.0.1".to_string()), multi_addr_dns.hostname());
        assert_eq!(Some(10501u16), multi_addr_dns.port());
    }
}

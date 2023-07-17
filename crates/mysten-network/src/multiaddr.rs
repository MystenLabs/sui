// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use eyre::{eyre, Result};
use std::{
    borrow::Cow,
    net::{IpAddr, SocketAddr},
};

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

    pub(crate) fn iter(&self) -> ::multiaddr::Iter<'_> {
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

    /// Set the ip address to `0.0.0.0`. For instance, it converts the following address
    /// `/ip4/155.138.174.208/tcp/1500/http` into `/ip4/0.0.0.0/tcp/1500/http`.
    pub fn zero_ip_multi_address(&self) -> Self {
        let mut new_address = ::multiaddr::Multiaddr::empty();
        for component in &self.0 {
            match component {
                multiaddr::Protocol::Ip4(_) => new_address.push(multiaddr::Protocol::Ip4(
                    std::net::Ipv4Addr::new(0, 0, 0, 0),
                )),
                c => new_address.push(c),
            }
        }
        Self(new_address)
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
    match protocols
        .next()
        .ok_or_else(|| eyre!("unexpected end of multiaddr"))?
    {
        Protocol::Http => Ok("http"),
        Protocol::Https => Ok("https"),
        _ => Err(eyre!("expected http/https protocol")),
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

// Parse a full /unix/-/{http,https} address
#[cfg(unix)]
pub(crate) fn parse_unix(address: &Multiaddr) -> Result<(Cow<'_, str>, &'static str)> {
    let mut iter = address.iter();

    let path = match iter
        .next()
        .ok_or_else(|| eyre!("unexpected end of multiaddr"))?
    {
        Protocol::Unix(path) => path,
        other => return Err(eyre!("expected unix found {other}")),
    };
    let http_or_https = parse_http_https(&mut iter)?;
    parse_end(&mut iter)?;

    Ok((path, http_or_https))
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
    fn test_get_hostname_port() {
        let multi_addr_ip4 = Multiaddr(multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10500u16)));
        assert_eq!(Some("127.0.0.1".to_string()), multi_addr_ip4.hostname());
        assert_eq!(Some(10500u16), multi_addr_ip4.port());

        let multi_addr_dns = Multiaddr(multiaddr!(Dns("mysten.sui"), Tcp(10501u16)));
        assert_eq!(Some("mysten.sui".to_string()), multi_addr_dns.hostname());
        assert_eq!(Some(10501u16), multi_addr_dns.port());
    }
}

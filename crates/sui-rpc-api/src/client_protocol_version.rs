// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_protocol_config::ProtocolVersion;
use tonic::metadata::MetadataMap;

/// Request header carrying the highest protocol version whose types the client can decode.
pub const X_SUI_CLIENT_PROTOCOL_VERSION: &str = "x-sui-client-protocol-version";

/// The highest protocol version the client that sent a request understands, if it said.
///
/// Self-reported, so it may be used to pick a response shape the client can decode but never for
/// access control. `None` for clients that predate the header or send a malformed value.
pub fn client_protocol_version(metadata: &MetadataMap) -> Option<ProtocolVersion> {
    metadata
        .get(X_SUI_CLIENT_PROTOCOL_VERSION)?
        .to_str()
        .ok()?
        .parse()
        .ok()
        .map(ProtocolVersion::new)
}

/// How far past this binary's max protocol version a reported version is still recorded as
/// itself. A client may know a few versions this node doesn't, but not many.
const METRIC_VERSIONS_ABOVE_MAX: u64 = 20;

/// The client's reported protocol version for metrics, or 0 (never a real protocol version) if
/// the header is missing or invalid. The header is client-controlled, so versions above
/// `ProtocolVersion::MAX_ALLOWED + METRIC_VERSIONS_ABOVE_MAX` count as invalid, which bounds the
/// number of distinct values.
pub(crate) fn client_protocol_version_for_metrics(metadata: &MetadataMap) -> u64 {
    client_protocol_version(metadata)
        .map(|v| v.as_u64())
        .filter(|&v| v <= ProtocolVersion::MAX_ALLOWED.as_u64() + METRIC_VERSIONS_ABOVE_MAX)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: Option<&'static str>) -> Option<ProtocolVersion> {
        let mut metadata = MetadataMap::new();
        if let Some(value) = value {
            metadata.insert(X_SUI_CLIENT_PROTOCOL_VERSION, value.parse().unwrap());
        }
        client_protocol_version(&metadata)
    }

    #[test]
    fn parses_header() {
        assert_eq!(parse(Some("138")), Some(ProtocolVersion::new(138)));
        assert_eq!(parse(None), None);
        assert_eq!(parse(Some("")), None);
        assert_eq!(parse(Some("-1")), None);
        assert_eq!(parse(Some("1.2")), None);
        assert_eq!(parse(Some("18446744073709551616")), None);
    }

    #[test]
    fn metric_version_is_bounded() {
        let version = |value: Option<String>| {
            let mut metadata = MetadataMap::new();
            if let Some(value) = value {
                metadata.insert(X_SUI_CLIENT_PROTOCOL_VERSION, value.parse().unwrap());
            }
            client_protocol_version_for_metrics(&metadata)
        };
        let cap = ProtocolVersion::MAX_ALLOWED.as_u64() + METRIC_VERSIONS_ABOVE_MAX;

        assert_eq!(version(None), 0);
        assert_eq!(version(Some("abc".into())), 0);
        assert_eq!(version(Some("137".into())), 137);
        assert_eq!(version(Some(cap.to_string())), cap);
        assert_eq!(version(Some((cap + 1).to_string())), 0);
        assert_eq!(version(Some(u64::MAX.to_string())), 0);
    }
}

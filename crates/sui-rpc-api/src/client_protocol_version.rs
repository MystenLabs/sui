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
}

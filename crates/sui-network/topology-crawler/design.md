# Topology Crawler Design (Planning)

## Overview
The topology crawler is a Rust binary that discovers the public Sui network
graph by walking the discovery protocol starting from hardcoded seed peers. It
outputs a JSON snapshot to stdout, which a cron job can capture and publish for
the web UI.

The crawler lives alongside the discovery protocol in the `sui-network` crate,
under `crates/sui-network/topology-crawler/`, and reuses the discovery RPC
client to query peers.

## Goals
- Traverse the reachable public peer graph starting from seed peers.
- Emit a compact JSON snapshot of nodes and edges with public metadata.
- Provide a CLI for `--network (mainnet|testnet)`.
- Run as a cron job every 60 minutes (or on-demand) and overwrite the snapshot.

## Non-goals
- Persisting historical graph data in a database.
- Enumerating private or trusted peers that are not shared publicly.
- Validating network correctness beyond signature checks and sanity filters.

## Inputs and Outputs
### Inputs
- `--network (mainnet|testnet)` CLI argument.
- Hardcoded seed peers from `docs/content/guides/operator/sui-full-node.mdx`.
- Discovery RPC `get_known_peers_v2` responses.

### Output (JSON snapshot to stdout)
```json
{
  "generated_at_ms": 1730000000000,
  "nodes": [
    {
      "peer_id": "...",
      "addresses": ["/dns/ewr-00.mainnet.sui.io/udp/8084"],
      "access_type": "Public",
      "timestamp_ms": 1730000000000,
      "label": "ewr-00.mainnet.sui.io"
    }
  ],
  "edges": [
    {"from": "peer_id_a", "to": "peer_id_b"}
  ]
}
```

## Crawl Algorithm
1. Initialize a BFS queue with the configured seed peers.
2. For each peer:
   - Dial discovery RPC and call `get_known_peers_v2`.
   - Validate `SignedNodeInfo` signatures.
   - Record the peer's `NodeInfo` and edges to returned peers.
   - Enqueue newly discovered peers (dedupe by `peer_id`).
3. Stop when the queue is empty or after a max budget (time or peer count).
4. Write the JSON snapshot to stdout.

## Public Metadata
`NodeInfo` provides:
- `peer_id`
- `addresses` (multiaddr list)
- `timestamp_ms`
- `access_type` (Public/Private/Trusted)

No human-readable name is included by the protocol. The crawler derives a label
from DNS multiaddrs when present (e.g., `/dns/ewr-00.mainnet.sui.io/...` becomes
`ewr-00.mainnet.sui.io`). If no DNS address exists, the label falls back to the
`peer_id` prefix.

## Access Control and Filtering
- Only persist `AccessType::Public` peers.
- Skip peers whose `SignedNodeInfo` signature fails verification.
- Deduplicate by `peer_id`; keep the freshest `timestamp_ms`.

## Scheduling and Operation
- Run via cron every 60 minutes (or a systemd timer).
- Ensure only one instance runs at a time (lockfile or PID check).
- Emit metrics: peers scanned, peers discovered, crawl duration, failures.

## Failure Handling
- Per-peer timeouts and exponential backoff on retry.
- Partial snapshots are allowed but flagged in logs.
- If seed peers are unreachable, emit an empty snapshot with an error field.

## UI Integration
The web UI consumes the JSON snapshot and renders the graph. For filtering and
search, the UI can index on `label`, `peer_id`, and `access_type`.

## Future Extensions
- Add optional incremental diffs between snapshots.
- Store history in a separate service if long-term analytics are needed.

# Move Package Registry (MVR) API Notes

## Background

The Move Package Registry (MVR) is a naming service for Sui Move packages maintained by Mysten Labs.
Source: https://github.com/MystenLabs/mvr

The frontend at https://www.moveregistry.com is a React/Next.js SPA — pages are client-rendered
and cannot be scraped with a simple HTTP fetch. Use the underlying REST API directly instead.

## Endpoints

Base URL (mainnet): `https://mainnet.mvr.mystenlabs.com`
Base URL (testnet): `https://testnet.mvr.mystenlabs.com`

The API server is implemented in `crates/mvr-api` in the MVR repo and exposes:

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/v1/resolution/{name}` | Resolve a name → package address |
| POST | `/v1/resolution/bulk` | Bulk name → address resolution |
| GET | `/v1/reverse-resolution/{package_id}` | Reverse: address → name |
| POST | `/v1/reverse-resolution/bulk` | Bulk reverse resolution |
| GET | `/v1/names/{name}` | Full name metadata (package_address, git_info, version, etc.) |
| GET | `/v1/names` | Paginated list of all registered names |
| GET | `/v1/type-resolution/{type_name}` | Resolve a Move type name |
| GET | `/v1/struct-definition/{type_name}` | Get struct definition |
| GET | `/v1/package-address/{address}/dependencies` | Dependency graph |
| GET | `/v1/package-address/{address}/dependents` | Reverse dependencies |
| GET | `/v1/sitemap` | Sitemap |

## Usage Examples

### Resolve a package name to its on-chain address

```bash
curl https://mainnet.mvr.mystenlabs.com/v1/resolution/@aftermath-fi/amm
# → {"package_id":"0xf948935b..."}

curl https://mainnet.mvr.mystenlabs.com/v1/resolution/@bluefinprotocol/bluefin-spot
# → {"package_id":"0xd075338d..."}
```

### Get full metadata for a package

```bash
curl https://mainnet.mvr.mystenlabs.com/v1/names/@aftermath-fi/amm
# → {
#     "name": "@aftermath-fi/amm",
#     "package_address": "0xf948935b...",
#     "version": 3,
#     "git_info": { "repository_url": "...", "tag": "mainnet/amm/v3" },
#     "metadata": { "description": "...", "homepage_url": "..." }
#   }
```

### Reverse resolve: address → name

```bash
curl https://mainnet.mvr.mystenlabs.com/v1/reverse-resolution/0xf948935b...
# → {"name":"@aftermath-fi/amm"}
```

### Get package dependencies

```bash
curl https://mainnet.mvr.mystenlabs.com/v1/package-address/0xf948935b.../dependencies
# → {"dependencies":["0x1","0x2",...]}
```

## Notes on Package Addresses

In Sui Move, packages can be upgraded. The address system works as follows:

- **`published-at`** (canonical / original): The address of the first-published version.
  This is what MVR's `package_address` field tracks.
  Example: Bluefin spot = `0xd075338d...` (v1, the `published-at` value)

- **Named address** (latest upgrade): The current version's address used in Move.toml
  `[addresses]` section. Needed to call the latest functions.
  Example: Bluefin spot `bluefin_spot = "0x3492c874..."` (v17/latest)

- **Pool object types** may reference either the original or upgraded package address
  depending on when the pool was created.

For `sui_getNormalizedMoveModule`, use the **latest** package address for the most
up-to-date ABI. Both addresses are valid for introspection.

## Future Extension: sui fork mrv-resolve

The `sui fork` tool could expose an `mvr-resolve` subcommand backed by this API:

```rust
// In commands.rs
SuiForkCommand::MvrResolve { name } => {
    let url = format!("https://mainnet.mvr.mystenlabs.com/v1/names/{name}");
    let resp: serde_json::Value = reqwest::get(&url).await?.json().await?;
    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}
```

This would let researchers quickly look up protocol package addresses without leaving
the `sui fork` workflow.

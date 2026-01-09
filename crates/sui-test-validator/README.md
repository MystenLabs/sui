The sui-test-validator starts a local network that includes a Sui Full node, a Sui validator, a Sui faucet and (optionally)
an indexer.

## Guide

Refer to [sui-local-network.md](../../docs/content/guides/developer/getting-started/local-network.mdx)

## Run with a persisted state
You can combine this with indexer runs as well to save a persisted state on local development.

1. Generate a config to store db and genesis configs `sui genesis -f --with-faucet --working-dir=[some-directory]`
2. `sui-test-validator --config-dir [some-directory]`

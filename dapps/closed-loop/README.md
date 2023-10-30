# Closed Loop Token (CLT) CLI

This application implements a CLI for creating new Closed Loop Tokens and managing them.

## Prepare & Publish

To publish and store the config for the CL Token, please, use this command:

```bash
sui client publish \
    --gas-budget 1000000000 \
    --skip-dependency-verification \
    --with-unpublished-dependencies \
    --json > published.json
```

Notes:

- *it is important that you store the result in the json file for the CLI to be able to fetch the config.*
- *the environment for publishing must be "devnet" so the CLI can work with it*

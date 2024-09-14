---
title: Sui Bridge Validator Runbook
---

## Prerequisite

Install `sui`, `sui-bridge-cli` binaries:
```bash
# install from tip of `main`
cargo install --locked --git "https://github.com/MystenLabs/sui.git" sui sui-bridge-cli
# install with a commit sha
cargo install --locked --git "https://github.com/MystenLabs/sui.git" --rev {SHA} sui sui-bridge-cli
```

## Committee Registeration

### Prepare for Metadata

The required metadata includes two things:
* `BridgeAuthorityKey`, a ECDSA key to sign messages. Since this is a hot key that is kept in memory, it’s fine to use the following tool to generate one and write to file.
* a REST API URL where the bridge node listens to and serves requests. Example: `https://bridge.example-sui-validator.io:443`. Make sure the port is correct and the url does not contain any invalid characters, for exmaple quotes.

To create a `BridgeAuthorityKey`, run
```bash
sui-bridge-cli create-bridge-validator-key {PATH_TO_WRITE}
```
This creates the keypair and writes it to `{PATH_TO_WRITE}`.

*Note: it's highly recommended you create a new key pair in a secure environment (e.g. in the same machine where your node will run) to avoid key compromise.*

### Registration
Once you have both authority key file and REST API URL ready, you can register them by using sui cli:
```bash
sui validator register-bridge-committee --bridge-authority-key-path <BRIDGE_AUTHORITY_KEY_PATH> --bridge-authority-url <BRIDGE_AUTHORITY_URL>
```

#### Offline Signing
If your validator account key is kept in cold storage or you want to do offline signing, use flag `--print-only` and provide validator address with `--validator-address`. This prints serialized unsigned transaction bytes, then you can use your preferred signing process to produce signed bytes. Run the following command to execute it:
```bash
sui client execute-signed-tx
```

#### Update Metadata
Both key and URL are changeable **before the committee is finalized**. If you wish to update metadata, simply rerun `sui validator register-bridge-committee`.

#### View Registered Metadata
To double check your registered the correct metadata onchain, run
```
sui-bridge-cli view-bridge-registration --sui-rpc-url {SUI_FULLNODE_URL}
```

## Bridge Node

### Bridge Node Hardware Requirements

Suggested hardware requirements:
* CPU: 6 physical cores
* Memory: 16GB
* Storage: 200GB
* Network: 100Mbps

### WAF Protection for Bridge Node

In order to protect against DDOS and attacks intended to expend validator resources, rate limit protection of the bridge server is required. 
In addition to protection, this will give node operators fine-grained control over the rate of requests the receive, and observability into those requests.

The currently recommended rate-limit is `50 requests/second per unique IP`.

#### WAF Options

You can use a managed cloud service, for example:
* [Cloudflare WAF](https://www.cloudflare.com/en-ca/application-services/products/waf/)
* [AWS WAF](https://aws.amazon.com/waf/)
* [GCP Cloud Armor](https://cloud.google.com/security/products/armor)

It's also possible to use an open source load balancer such as haproxy for a simple, ip-based rate limit.
An example, shortened HAProxy config for this looks like:
```
frontend http-in
    bind *:80
    # Define an ACL to count requests per IP and block if over limit
    acl too_many_requests src_http_req_rate() gt 50
    # Track the request rate per IP
    stick-table type ip size 1m expire 1m store http_req_rate(1s)
    # Check request rate and deny if the limit is exceeded
    http-request track-sc0 src
    http-request deny if too_many_requests

    default_backend bridgevalidator

backend bridgevalidator
    server bridgevalidator 0.0.0.0:9191
```

If choosing to use an open source load-balancing option, make sure to set up metrics collection and alerting on the service.

### Bridge Node Config
Use `sui-bridge-cli` command to create a template. If you want to run `BridgeClient` (see the following section), pass `--run-client` as a parameter.

```
sui-bridge-cli create-bridge-node-config-template {PATH}
sui-bridge-cli create-bridge-node-config-template --run-client {PATH}
```

In the generated config:
* `server-listen-port` : the port that Bridge Node listens to handle requests
* `metrics-port`: port to export prometheus metrics
* `bridge-authority-key-path` is the path to the Bridge Validator key, generated from `sui-bridge-cli create-bridge-validator-key` from above command.
* `run-client`: if Bridge Client should be enabled in Bridge Node (more instructions for this below)
* `approved-governance-actions` : a list of governance actions that you want to support.
* `sui:sui-rpc-url`: Sui RPC URL
* `sui:sui-bridge-chain-id`: 0 for Sui Mainnet, 1 for Sui Testnet
* `eth:eth-rpc-url`: Ethereum RPC URL
* `eth:eth-bridge-proxy-address`: The proxy address for Bridge Solidity contracts on Ethereum.
* `eth:eth-bridge-chain-id`: 10 for Ethereum Mainnet, 11 for Sepolia Testnet
* `eth:eth-contracts-start-block-fallback`: The starting block BridgeNodes queries for from Ethereum FullNode. This number should be the block where Solidity contracts are deployed or slightly before.
* `metrics:push-url`: The url of the remote Sui metrics pipeline: `https://metrics-proxy.[testnet|mainnet].sui.io:8443/publish/metrics`

With `run-client: true`, these additional fields can be found in the generated config:
* `db-path`: path of BridgeClient DB, for BridgeClient
* `sui:bridge-client-key-path`: the file path of Bridge Client key. This key can be generated with `sui-bridge-cli create-bridge-client-key` as shown above. When `run-client` is true but `sui:bridge-client-key-path` not provided, it defaults to use Bridge Validator key to submit transactions on Sui. However this is not recommended for the sake of key separation.

### Bridge Client
`BridgeClient` orchestrates bridge transfer requests.
* It is **optional** to run for a `BridgeNode`.
* `BridgeClient` submits transaction on Sui Network. Thus when it's enabled, a Sui Account Key with enough SUI balance is needed.

To enable `bridge_client` feature on a `BridgeNode`, set the following parameters in `BridgeNodeConfig`:
```yaml
run-client: true
db-path: <PATH_TO_DB>
sui:
    bridge-client-key-path: <PATH_TO_BRIDGE_CLIENT_KEY>  # optional, when absent, use bridge-authority-key-path as the keypair for BridgeClient
```


To create a `BridgeClient` keypair, run
```
sui-bridge-cli create-bridge-client-key <PATH_TO_BRIDGE_CLIENT_KEY>
```
This prints the newly created Sui Address. Then we need to fund this address with some SUI for operations.

# Sui indexer-alt-jsonrpc proxy

This proxy is adapted from `sui-edge-proxy` to filter and transform requests sent to `indexer-alt-jsonrpc` server.
Unsupported methods can be configured via `unsupported-methods` configuration and are dropped by the proxy.
Since the cursor format used by alt rpc server is different, queries using cursors are transformed to a previously cached cursor before being proxied.

## Run the proxy
`config.yaml` provides an example config file. Provide the config file and run like this
```
cargo run -p sui-indexer-alt-jsonrpc-proxy -- --config <config-file-path>
```


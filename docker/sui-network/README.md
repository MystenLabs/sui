# Sui Network Docker Compose

This was tested using MacOS 14.3.1, Docker Compose: v2.13.0.

This compose brings up 3 validators, 1 fullnode, and 1 stress (load gen) client

Steps for running:

`cd docker/sui-network`
`./new-genesis.sh`
(optional) `rm -r /tmp/sui`
`docker compose up`

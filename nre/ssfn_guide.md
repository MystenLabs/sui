
State sync fullnodes are in essence just regular fullnodes, with a few tweaks.


I won't detail setting up a Sui fullnode here, just the ways in which state sync fullnodes differ:

1. State sync fullnodes should be peered directly to a validator, these are the only nodes in the network that explictly set validators as their peer

The way to allow your state sync fullnode to connect to your validator is as follows:

```
# create a network key for your ssfn
$ sui keytool generate ed25519
# record the peerId of the key
# modify the ssfn's config to use the newly created key as a network key, eg:
# ---
# network-key-pair:
#   path: /opt/sui/key-pairs/network.key
# p2p-config:
#   seed-peers:
#     - address: /dns/myssfn1/udp/8084
#       peer-id: abcdefg1 # you can grab this value via `sui keytool show [path_to_validator_keys]/network.key`
# ...

# allow your ssfn to talk to your validator by setting validator config's seed peers to point at your ssfns
$ vim /opt/sui/config/sui-node.yaml #on validator host

# p2p-config:
#   seed-peers:
#     - address: /dns/myssfn1/udp/8084
#       peer-id: abcdefg1
#     - address: /dns/myssfn2/udp/8084
#       peer-id: abcdefg2
```

2. State sync fullnodes should have indexing disabled, run with pruning, and push metrics to mystens metric proxy

This is a simple change, just add the following configs to your fullnode:
```
enable-index-processing: false

authority-store-pruning-config:
  num-latest-epoch-dbs-to-retain: 3
  epoch-db-pruning-period-secs: 3600
  max-checkpoints-in-batch: 10
  max-transactions-in-batch: 1000
  num-epochs-to-retain: 0
  num-epochs-to-retain-for-checkpoints: 2
  periodic-compaction-threshold-days: 1

metrics:
  push-interval-seconds: 60
  push-url: https://metrics-proxy.mainnet.sui.io:8443/publish/metrics
```

This coupled with starting your node from a formal snapshot should mean a very small database footprint for ssfns


![ssfn diagram](nre/ssfn-diagram.png)



#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Make the cluster's genesis and one config per host, natively, once.

After sui-operations' docker/sui-antithesis/genesis/generate.py:
`sui genesis --from-config`, the validator overlay merged into
each validator's config, the fullnode config, and an observer fullnode
that follows validator1's consensus. Writes

    <out>/genesis.blob
    <out>/<host>/node.yaml     for validator1-4, fullnode1, observer-fullnode1

Usage: genesis.py SUI OUT
"""

import base64
import os
import shutil
import subprocess
import sys
import tempfile

import yaml
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

HERE = os.path.dirname(os.path.abspath(__file__))


def load(name):
    with open(os.path.join(HERE, name)) as f:
        return yaml.safe_load(f)


def dump(path, config):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(yaml.safe_dump(config, sort_keys=False))


def merge(base, over):
    """Maps merge key by key; anything else in `over` replaces."""
    for key, value in over.items():
        if isinstance(value, dict) and isinstance(base.get(key), dict):
            merge(base[key], value)
        else:
            base[key] = value
    return base


def network_public_key(config):
    """The hex ed25519 public key of a node config's network key pair."""
    seed = base64.b64decode(config["network-key-pair"]["value"])[1:]
    key = Ed25519PrivateKey.from_private_bytes(seed)
    return key.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw).hex()


def main(sui, out):
    os.makedirs(out)
    template = load("genesis.yaml")
    work = tempfile.mkdtemp()
    os.makedirs(os.path.join(work, "z"))
    try:
        dump(os.path.join(work, "genesis.yaml"), template)
        subprocess.run(
            [sui, "genesis", "--from-config", os.path.join(work, "genesis.yaml"),
             "--working-dir", os.path.join(work, "z"), "-f"],
            check=True,
        )
        shutil.move(os.path.join(work, "z", "genesis.blob"), out)

        overlay = load("validator.yaml")
        for validator in template["validator_config_info"]:
            name = validator["name"]
            with open(os.path.join(work, "z", f"{name}-8080.yaml")) as f:
                config = merge(yaml.safe_load(f), overlay)
            dump(os.path.join(out, name, "node.yaml"), config)
    finally:
        shutil.rmtree(work)

    fullnode = load("fullnode.yaml")
    dump(os.path.join(out, "fullnode1", "node.yaml"), fullnode)

    with open(os.path.join(out, "validator1", "node.yaml")) as f:
        peer = network_public_key(yaml.safe_load(f))
    observer = merge(load("fullnode.yaml"), {
        "p2p-config": {"anemo-config": {"inbound-connection-rate-limit-per-ip": 0}},
        "fullnode-sync-mode": "consensus-observer",
        "consensus-config": {
            "db-path": "db/consensus_db",
            "db-retention-epochs": 1,
            "parameters": {"observer": {
                "server_port": 8085,
                "peers": [{"public_key": peer, "address": "/dns/validator1/udp/8085"}],
            }},
        },
    })
    dump(os.path.join(out, "observer-fullnode1", "node.yaml"), observer)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])

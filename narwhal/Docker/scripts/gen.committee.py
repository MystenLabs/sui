#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Generate a committee.json file
"""

import sys
import argparse
import json


def main():
    "mainly main."
    parser = argparse.ArgumentParser(
        description="committee file generator")

    parser.add_argument("-n", default=4, type=int,
                        help="number of primary instances")
    parser.add_argument("-f", default="committee.json",
                        help="committee.json file name")
    parser.add_argument("-d", default=None, help="target directory")
    args = parser.parse_args()

    # load keys
    keys = []
    for i in range(args.n):
        k = open("{}/validator-{:02d}/key.json".format(args.d, i)).read()
        keys.append(json.loads(k))

    network_keys = []
    for i in range(args.n):
        k = open("{}/validator-{:02d}/network-key.json".format(args.d, i)).read()
        network_keys.append(json.loads(k))


    temp = {}
    for i, (k, nk) in enumerate(zip(keys, network_keys)):
        temp[k['name']] = {
            "network_key": nk['name'],
            "primary": {
                "primary_to_primary": "/dns/primary_{:02d}/udp/3000".format(i),
                "worker_to_primary": "/dns/primary_{:02d}/udp/3001".format(i)
            },
            "stake": 1,
        }
    out = {"authorities": temp, "epoch": 0}
    print(json.dumps(out, indent=4))


if __name__ == '__main__':
    sys.exit(main())

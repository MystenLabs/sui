#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0
"""Generate a workers.json file
"""

import sys
import argparse
import json


def main():
    "mainly main."
    parser = argparse.ArgumentParser(
        description="workers file generator")

    parser.add_argument("-np", default=4, type=int,
                        help="number of primary instances")
    parser.add_argument("-nw", default=1, type=int,
                        help="number of worker instances per primary")
    parser.add_argument("-f", default="workers.json",
                        help="workers.json file name")
    parser.add_argument("-d", default=None, help="target directory")
    args = parser.parse_args()

    # load keys
    keys = []
    for i in range(args.np):
        k = open("{}/validator-{:02d}/key.json".format(args.d, i)).read()
        keys.append(json.loads(k))

    temp = {}
    starting_port = 4000
    for i, k in enumerate(keys):
        workers = {}
        port = starting_port
        for j in range(args.nw):
            workers[j] = {
                "transactions": "/dns/worker_{:02d}/tcp/{}/http".format(i, port+1),
                "worker_address": "/dns/worker_{:02d}/udp/{}".format(i, port+2)
            }
            port += 3
        temp[k['name']] = workers
    out = {"workers": temp, "epoch": 0}
    print(json.dumps(out, indent=4))


if __name__ == '__main__':
    sys.exit(main())

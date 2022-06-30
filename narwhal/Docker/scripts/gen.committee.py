#!/usr/bin/env python3
"""Generate a committee.json file
"""

import sys
import argparse
import json


def main():
    "mainly main."
    parser = argparse.ArgumentParser(description="committee file generator")

    parser.add_argument("-n", default=4, type=int, help="number of primary+worker instances")
    parser.add_argument("-f", default="committee.json", help="committee.json file name")
    parser.add_argument("-d", default=None, help="target directory")
    args = parser.parse_args()


    # load keys
    keys = []
    for i in range(args.n):
        k = open("{}/validator-{:02d}/key.json".format(args.d, i)).read()
        keys.append(json.loads(k))

    temp = {}
    for i, k in enumerate(keys):
        temp[k['name']] = {
            "primary": {
                "primary_to_primary": "/dns/primary_{:02d}/tcp/3000/http".format(i),
                "worker_to_primary": "/dns/primary_{:02d}/tcp/3001/http".format(i)
            },
            "stake": 1,
            "workers": {
                "0": {
                    "primary_to_worker": "/dns/worker_00/tcp/4000/http",
                    "transactions": "/dns/worker_00/tcp/4001/http",
                    "worker_to_worker": "/dns/worker_00/tcp/4002/http"
                }
            }
        }
    out = {"authorities": temp, "epoch": 0}
    print(json.dumps(out, indent=4))

if __name__ == '__main__':
    sys.exit(main())

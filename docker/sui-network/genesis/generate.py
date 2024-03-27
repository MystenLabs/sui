#!/usr/bin/env python3
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0


import argparse
import re
import os.path
import subprocess
import sys
import typing
import yaml

import hiyapyco  # https://pypi.org/project/HiYaPyCo/

BASE_DIR = os.path.dirname(os.path.abspath(__file__))

# Use the base directory to construct absolute paths
_COMMON_OVERLAY_PATH = os.path.join(BASE_DIR, "overlays", "common.yaml")


def parse_overlays(overlay_type: str) -> str:
    overlays: str = ""

    with open(_COMMON_OVERLAY_PATH, "r") as f:
        common_overlays = yaml.safe_load(f)[overlay_type]
        overlays = yaml.safe_dump(common_overlays)
        overlays = hiyapyco.load([overlays])

    return hiyapyco.dump(overlays)


def get_network_addresses(genesis_config: typing.Dict) -> typing.List:
    network_adr_pattern = (
        r"/(?P<type>dns|ip4|ip6|unix)/(?P<address>[^/]*)(/udp|/tcp)?/(?P<port>\d+)?"
    )
    network_addresses = []
    for validator in genesis_config.get("validator_config_info"):
        match = re.search(network_adr_pattern, validator["network_address"])
        network_addresses.append(f'{match.group("address")}-{match.group("port")}.yaml')
    return network_addresses


def set_validator_name(genesis_config: typing.Dict) -> typing.Dict:
    network_adr_pattern = (
        r"/(?P<type>dns|ip4|ip6|unix)/(?P<address>[^/]*)(/udp|/tcp)?/(?P<port>\d+)?"
    )
    for validator in genesis_config.get("validator_config_info"):
        match = re.search(network_adr_pattern, validator["network_address"])
        validator["name"] = match.group("address")
    return genesis_config


def main(args: argparse.ArgumentParser) -> None:
    # create target directory if it doesn't exist
    _ = subprocess.run(["mkdir", "-p", "z", f"{args.target_directory}"], check=True)

    # load genesis template
    with open(args.genesis_template, "r") as f:
        genesis_config = yaml.safe_load(f)

    validator_network_addresses = get_network_addresses(genesis_config)

    # set the validator name based on their address
    genesis_config = set_validator_name(genesis_config)

    # write genesis configuration file
    with open(f"{args.target_directory}/genesis.yaml", "w") as f:
        f.write(yaml.safe_dump(genesis_config))

    # run genesis with newly created genesis configuration file
    _ = subprocess.run(
        [
            "sui",
            "genesis",
            "--from-config",
            f"{args.target_directory}/genesis.yaml",
            "--working-dir",
            "z",
            "-f",
        ],
        # this should be inherited from the parent process, but just in case
        env=os.environ,
        check=True,
    )

    # parse validator overlays
    overlays = parse_overlays("validator")

    # process validator overlays
    for validator in validator_network_addresses:
        with open(f"z/{validator}", "r") as f:
            validator_config = f.read()

        merged_yaml = hiyapyco.load(
            [validator_config, overlays], method=hiyapyco.METHOD_MERGE
        )
        merged_yaml = hiyapyco.dump(merged_yaml)

        with open(f"{args.target_directory}/{validator}", "w") as f:
            f.write(merged_yaml)

    # move other required files to target
    subprocess.run(["mv", "z/genesis.blob", f"{args.target_directory}/"], check=True)

    _ = subprocess.run(["rm", "-rf", "z"], check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "-g",
        "--genesis-template",
        type=str,
        help="template to use for genesis.yaml generation",
        required=False,
    )
    parser.add_argument(
        "-t",
        "--target-directory",
        type=str,
        help="target directory for generated genesis and configuration files",
        default="target",
        required=False,
    )
    parser.add_argument(
        "-o",
        "--override-generation",
        type=str,
        help="do not generate and use override configuration instead",
        required=False,
    )
    parser.add_argument(
        "-p",
        "--protocol-config-override",
        type=str,
        help="protocol config override to set",
        required=False,
    )
    args = parser.parse_args()

    # exit if configuration already exists
    if os.path.exists(f"{args.target_directory}/genesis.blob"):
        print("configuration already exists, not generating")
        sys.exit(0)

    main(args)

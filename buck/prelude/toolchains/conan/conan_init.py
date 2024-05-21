#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse

import conan_common


def conan_profile(conan, user_home, trace_log):
    env = conan_common.conan_env(user_home=user_home, trace_log=trace_log)

    # TODO[AH] Allow users to define additional remotes.
    remotes = [
        ("conancenter", "https://center.conan.io"),
    ]

    for name, url in remotes:
        conan_common.run_conan(conan, "remote", "add", "-f", name, url, env=env)


def main():
    parser = argparse.ArgumentParser(
        prog="conan_init", description="Initialise a Conan home directory."
    )
    parser.add_argument(
        "--conan",
        metavar="FILE",
        type=str,
        required=True,
        help="Path to the Conan executable.",
    )
    parser.add_argument(
        "--user-home",
        metavar="PATH",
        type=str,
        required=True,
        help="Path to the Conan base directory.",
    )
    parser.add_argument(
        "--trace-file",
        metavar="PATH",
        type=str,
        required=True,
        help="Write Conan trace log to this file.",
    )
    args = parser.parse_args()

    conan_profile(args.conan, args.user_home, args.trace_file)


if __name__ == "__main__":
    main()

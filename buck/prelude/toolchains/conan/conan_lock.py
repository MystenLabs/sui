#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse

import conan_common


def conan_lock(conan, profile, conanfile, lockfile_out, lockfile, user_home, trace_log):
    env = conan_common.conan_env(user_home=user_home, trace_log=trace_log)

    args = ["lock", "create"]
    args.extend(["--profile", profile])
    args.extend(["--build", "missing"])
    if lockfile:
        args.extend(["--lockfile", lockfile])
    args.extend(["--lockfile-out", lockfile_out])
    args.append(conanfile)

    conan_common.run_conan(conan, *args, env=env)


def main():
    parser = argparse.ArgumentParser(
        prog="conan_lock", description="Generate a Conan lock-file."
    )
    parser.add_argument(
        "--conan",
        metavar="FILE",
        type=str,
        required=True,
        help="Path to the Conan executable.",
    )
    parser.add_argument(
        "--conan-init",
        metavar="PATH",
        type=str,
        required=True,
        help="Path to the base Conan user-home.",
    )
    parser.add_argument(
        "--profile",
        metavar="FILE",
        type=str,
        required=True,
        help="Path to the Conan profile.",
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
    parser.add_argument(
        "--conanfile",
        metavar="FILE",
        type=str,
        required=True,
        help="Path to the Conanfile.",
    )
    parser.add_argument(
        "--lockfile-out",
        metavar="FILE",
        type=str,
        required=True,
        help="Path to the lock-file to generate.",
    )
    parser.add_argument(
        "--lockfile",
        metavar="FILE",
        type=str,
        required=False,
        help="Path to an existing Conan lock-file to base resolution on.",
    )
    args = parser.parse_args()

    conan_common.install_user_home(args.user_home, args.conan_init)

    conan_lock(
        args.conan,
        args.profile,
        args.conanfile,
        args.lockfile_out,
        args.lockfile,
        args.user_home,
        args.trace_file,
    )


if __name__ == "__main__":
    main()

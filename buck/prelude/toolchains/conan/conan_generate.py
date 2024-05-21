#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import os
import shutil

import conan_common


def conan_install(
    conan,
    conanfile,
    lockfile,
    install_folder,
    output_folder,
    user_home,
    manifests,
    install_info,
    trace_log,
):
    env = conan_common.conan_env(user_home=user_home, trace_log=trace_log)

    args = ["install"]
    args.extend(["--build", "missing"])
    args.extend(["--generator", "BucklerGenerator"])
    args.extend(["--lockfile", lockfile])
    args.extend(["--install-folder", install_folder])
    args.extend(["--output-folder", output_folder])
    args.extend(["--manifests", manifests])
    args.extend(["--json", install_info])
    args.append(conanfile)

    conan_common.run_conan(conan, *args, env=env)


def extract_generated(install_folder, targets_out):
    src = os.path.join(install_folder, "conan-imports.bzl")
    dst = targets_out
    shutil.copy(src, dst)


def main():
    parser = argparse.ArgumentParser(
        prog="conan_generate",
        description="Generate Buck2 imports of Conan built packages.",
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
        "--buckler",
        metavar="FILE",
        type=str,
        required=True,
        help="Path to the Buckler generator.",
    )
    parser.add_argument(
        "--install-folder",
        metavar="PATH",
        type=str,
        required=True,
        help="Path to install directory to place generator files into.",
    )
    parser.add_argument(
        "--output-folder",
        metavar="PATH",
        type=str,
        required=True,
        help="Path to the root output folder for generated and built files.",
    )
    parser.add_argument(
        "--user-home",
        metavar="PATH",
        type=str,
        required=True,
        help="Path to the Conan base directory.",
    )
    parser.add_argument(
        "--manifests",
        metavar="PATH",
        type=str,
        required=True,
        help="Write dependency manifests into this directory.",
    )
    parser.add_argument(
        "--install-info",
        metavar="PATH",
        type=str,
        required=True,
        help="Write install information JSON file to this location.",
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
        "--lockfile",
        metavar="FILE",
        type=str,
        required=True,
        help="Path to the Conan lock-file.",
    )
    parser.add_argument(
        "--targets-out",
        metavar="PATH",
        type=str,
        required=True,
        help="Write the generated targets to this file.",
    )
    args = parser.parse_args()

    conan_common.install_user_home(args.user_home, args.conan_init)
    conan_common.install_generator(args.user_home, args.buckler)

    os.mkdir(args.install_folder)
    os.mkdir(args.output_folder)
    os.mkdir(args.manifests)

    conan_install(
        args.conan,
        args.conanfile,
        args.lockfile,
        args.install_folder,
        args.output_folder,
        args.user_home,
        args.manifests,
        args.install_info,
        args.trace_file,
    )
    extract_generated(args.install_folder, args.targets_out)


if __name__ == "__main__":
    main()

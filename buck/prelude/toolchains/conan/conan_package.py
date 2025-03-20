#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import json
import os

import conan_common


def conan_install(
    conan,
    reference,
    lockfile,
    options,
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
    args.extend(["--lockfile", lockfile])
    args.extend(["--install-folder", install_folder])
    args.extend(["--output-folder", output_folder])
    args.extend(["--manifests", manifests])
    args.extend(["--json", install_info])
    args.append(reference.split("#")[0] + "@")

    conan_common.run_conan(conan, *args, env=env)


def verify_build_and_cached_deps(install_info, package, deps):
    """Verify that the package was built and dependencies were cached."""
    with open(install_info, "r") as f:
        info = json.load(f)
    package_parsed = conan_common.parse_reference(package)
    deps_parsed = {conan_common.parse_reference(dep) for dep in deps}
    for installed in info["installed"]:
        recipe_id = installed["recipe"]["id"]
        ref = conan_common.parse_reference(recipe_id)
        is_package = ref == package_parsed
        is_dep = ref in deps_parsed

        if not is_package and not is_dep:
            raise RuntimeError(
                "Unexpected installed package found: {}".format(recipe_id)
            )

        recipe_downloaded = installed["recipe"]["downloaded"]
        if is_package and not recipe_downloaded:
            raise RuntimeError("Cached package to build detected: {}".format(recipe_id))
        elif is_dep and recipe_downloaded:
            raise RuntimeError("Downloaded dependency detected: {}".format(recipe_id))

        for package in installed["packages"]:
            package_id = package["id"]
            package_downloaded = package["downloaded"]
            package_built = package["built"]
            if is_package and not (package_downloaded or package_built):
                raise RuntimeError(
                    "Cached package to build detected: {}-{}".format(
                        recipe_id, package_id
                    )
                )
            elif is_dep and (recipe_downloaded or package_built):
                raise RuntimeError(
                    "Downloaded or built dependency detected: {}-{}".format(
                        recipe_id, package_id
                    )
                )


def main():
    parser = argparse.ArgumentParser(
        prog="conan_package", description="Build a Conan package."
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
        "--lockfile",
        metavar="FILE",
        type=str,
        required=True,
        help="Path to the Conan lockfile.",
    )
    parser.add_argument(
        "--reference",
        metavar="STRING",
        type=str,
        required=True,
        help="Reference of the Conan package to build.",
    )
    parser.add_argument(
        "--package-id",
        metavar="STRING",
        type=str,
        required=True,
        help="Package ID of the Conan package to build.",
    )
    parser.add_argument(
        "--option",
        metavar="STRING",
        type=str,
        required=False,
        action="append",
        help="Conan options for the package to build.",
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
        help="Path to the Conan base directory used for Conan's cache.",
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
        "--cache-out",
        metavar="PATH",
        type=str,
        required=True,
        help="Copy the package's cache directory to this path.",
    )
    parser.add_argument(
        "--package-out",
        metavar="PATH",
        type=str,
        required=True,
        help="Copy the package directory to this path.",
    )
    parser.add_argument(
        "--dep-reference",
        metavar="STRING",
        type=str,
        required=False,
        action="append",
        default=[],
        help="Conan package dependency reference. All --dep-* arguments must align.",
    )
    parser.add_argument(
        "--dep-cache-out",
        metavar="PATH",
        type=str,
        required=False,
        action="append",
        default=[],
        help="Conan package dependency cache output directory. All --dep-* arguments must align.",
    )
    # TODO[AH] Remove unused `--manifests` and `--verify` flags and outputs.
    # TODO[AH] Should we enable the `--no-imports` flag?
    # TODO[AH] Handle packages that are build requirements and set
    #   `--build-require` in that case.
    args = parser.parse_args()

    conan_common.install_user_home(args.user_home, args.conan_init)
    assert len(args.dep_reference) == len(
        args.dep_cache_out
    ), "Mismatching dependency arguments."
    for ref, cache_out in zip(args.dep_reference, args.dep_cache_out):
        conan_common.install_reference(args.user_home, ref, cache_out)

    os.mkdir(args.install_folder)
    os.mkdir(args.output_folder)
    os.mkdir(args.manifests)

    conan = args.conan
    conan_install(
        conan,
        args.reference,
        args.lockfile,
        args.option,
        args.install_folder,
        args.output_folder,
        args.user_home,
        args.manifests,
        args.install_info,
        args.trace_file,
    )
    verify_build_and_cached_deps(args.install_info, args.reference, args.dep_reference)
    conan_common.extract_reference(args.user_home, args.reference, args.cache_out)
    conan_common.extract_package(
        args.user_home, args.reference, args.package_id, args.package_out
    )


if __name__ == "__main__":
    main()

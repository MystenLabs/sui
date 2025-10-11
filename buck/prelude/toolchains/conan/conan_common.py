# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import os
import shutil
import subprocess


def _none(s):
    if not s or s == "_":
        return None
    else:
        return s


def parse_reference(ref):
    """Parse a Conan package reference.

    These take the shape `name/version@channel/name#revision`.
    Omitted values or `_` are read as `None`.
    """
    name = None
    version = None
    user = None
    channel = None
    revision = None

    if "#" in ref:
        ref, revision = ref.split("#", 1)

    if "@" in ref:
        ref, user_channel = ref.split("@", 1)
        if "/" in user_channel:
            user, channel = user_channel.split("/", 1)
        else:
            user = user_channel

    if "/" in ref:
        name, version = ref.split("/", 1)
    else:
        name = ref

    return _none(name), _none(version), _none(user), _none(channel), _none(revision)


CONAN_DIR = ".conan"
GENERATORS_DIR = "generators"
STORE_DIR = "data"
PACKAGE_DIR = "package"


def conan_dir(user_home):
    """Conan folder under the Conen user home."""
    return os.path.join(user_home, CONAN_DIR)


def generators_dir(user_home):
    """Custom generators folder under the Conen user home."""
    return os.path.join(conan_dir(user_home), "generators")


def store_dir(user_home):
    """Store folder under the Conen user home."""
    return os.path.join(conan_dir(user_home), "data")


def reference_subtree(name, version, user, channel):
    """Package base directory subtree under the Conan store folder."""
    return os.path.join(name or "_", version or "_", user or "_", channel or "_")


def package_subtree(package_id):
    """Package directory subtree under the package base directory."""
    return os.path.join(PACKAGE_DIR, package_id)


def reference_dir(user_home, name, version, user, channel):
    """Package base directory under the Conan store folder."""
    return os.path.join(
        store_dir(user_home), reference_subtree(name, version, user, channel)
    )


def package_dir(user_home, name, version, user, channel, package_id):
    """Package directory under the Conan store folder."""
    return os.path.join(
        reference_dir(user_home, name, version, user, channel),
        package_subtree(package_id),
    )


def _copytree(src, dst):
    """Recursively copy the source directory tree to the destination.

    Copies symbolic links and ignores dangling symbolic links.
    """
    shutil.copytree(src, dst, symlinks=True, ignore_dangling_symlinks=True)


def install_user_home(user_home, base_user_home):
    """Copy the given base user-home to the current user-home."""
    src = base_user_home
    dst = user_home
    _copytree(src, dst)


def install_generator(user_home, generator_file):
    """Copy the given custom generator into the generators path.

    Note, this will overwrite any pre-existing generators.
    """
    src = generator_file
    dstdir = generators_dir(user_home)
    dst = os.path.join(dstdir, "conanfile.py")
    os.makedirs(dstdir, exist_ok=True)
    shutil.copyfile(src, dst)


def install_reference(user_home, reference, path):
    """Copy the cache directory of a given package reference into the store."""
    name, version, user, channel, _ = parse_reference(reference)
    src = path
    dst = reference_dir(user_home, name, version, user, channel)
    _copytree(src, dst)


def extract_reference(user_home, reference, output):
    """Copy the cache directory of the given package reference out of the store."""
    name, version, user, channel, _ = parse_reference(reference)
    src = reference_dir(user_home, name, version, user, channel)
    dst = output
    _copytree(src, dst)


def extract_package(user_home, reference, package_id, output):
    """Copy the package directory of the given package out of the store."""
    name, version, user, channel, _ = parse_reference(reference)
    src = package_dir(user_home, name, version, user, channel, package_id)
    dst = output
    _copytree(src, dst)


def conan_env(user_home=None, trace_log=None):
    """Generate environment variables needed to invoke Conan."""
    env = dict(os.environ)

    if user_home is not None:
        # Set the Conan base directory.
        env["CONAN_USER_HOME"] = os.path.abspath(user_home)

    if trace_log is not None:
        # Enable Conan debug trace.
        env["CONAN_TRACE_FILE"] = os.path.abspath(trace_log)

    # TODO[AH] Enable Conan revisions for reproducibility
    # Enable Conan revisions for reproducibility
    # env["CONAN_REVISIONS_ENABLED"] = "1"

    # Prevent over-allocation.
    # TODO[AH] Support parallized package builds and set an appropriate action
    #   weight using the `weight` parameter to `ctx.actions.run`.
    #   Note that not all Conan packages respect the `CONAN_CPU_COUNT` setting.
    env["CONAN_CPU_COUNT"] = "1"

    # Prevent interactive prompts.
    env["CONAN_NON_INTERACTIVE"] = "1"

    # Print every `self.run` invocation.
    # TODO[AH] Remove this debug output.
    env["CONAN_PRINT_RUN_COMMANDS"] = "1"

    # Disable the short paths feature on Windows.
    # TODO[AH] Enable if needed with a hermetic short path.
    env["CONAN_USER_HOME_SHORT"] = "None"

    return env


def run_conan(conan, *args, env=None):
    return subprocess.check_call([conan] + list(args), env=env or {})

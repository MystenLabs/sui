# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

import argparse
import json
import os
import pathlib
import subprocess
import sys
import tempfile


def clean_relative_paths(parent_path: str, rel_path: str) -> pathlib.Path:
    """Removes the extra upper dir level from a relative path and properly joins."""
    return os.path.join(parent_path, pathlib.Path(*pathlib.Path(rel_path).parts[1:]))


class jll_artifact:
    """Parses artifact info from json file and stores the relevant info."""

    def __init__(self, artifact_entry, json_file_dir):
        self.artifact_name = artifact_entry[0]
        rel_path_of_binary = artifact_entry[2]
        self.artifacts_path = os.path.join(
            clean_relative_paths(json_file_dir, artifact_entry[1]), rel_path_of_binary
        )

    def form_artifact_dependency(self):
        """Creates artifact dependency line that goes inside julia source file."""
        return '{} = "{}"'.format(self.artifact_name, self.artifacts_path)


class jll_library:
    """Parses the provided json file and stores the relevant info."""

    def __init__(self, json_entry, json_file_dir):
        self.package_name = json_entry[0]
        self.uuid = json_entry[1]
        self.jll_artifacts = [jll_artifact(a, json_file_dir) for a in json_entry[2]]

    def write_library(self, root_directory):
        """Creates and populates the library sources and directories"""
        self.package_dir = pathlib.Path(root_directory) / self.package_name
        self.package_dir.mkdir(parents=True, exist_ok=True)
        self._create_jll_src()
        self._create_project_toml()

    def _create_jll_src(self):
        """Creates the library src.jl file."""
        src_filename = self.package_name + ".jl"
        src_path = self.package_dir / "src"
        src_path.mkdir(parents=True, exist_ok=True)

        exports = [a.artifact_name for a in self.jll_artifacts]
        with open(src_path / src_filename, "w") as src_file:
            src_file.write("module " + self.package_name + "\n")
            src_file.write("export " + ", ".join(exports) + "\n")
            for a in self.jll_artifacts:
                src_file.write(a.form_artifact_dependency() + "\n")
            src_file.write("end\n")

    def _create_project_toml(self):
        """Creates the library Project.toml file."""
        with open(self.package_dir / "Project.toml", "w") as toml_file:
            toml_file.write('name = "{}"\n'.format(self.package_name))
            toml_file.write('uuid = "{}"\n'.format(self.uuid))
            toml_file.close()


def parse_jll_libs(json_data, json_file_dir, lib_dir):
    """Pulls jll library data from json file and writes library files."""

    libs = []
    for entry in json_data:
        # parse the jll itself into our data structure
        jll_lib = jll_library(entry, json_file_dir)
        # use that structure to "create" a new library in a temp directory
        jll_lib.write_library(lib_dir)
        # store for later if needed.
        libs.append(jll_lib)

    return libs


def build_command(json_data, json_file_dir, lib_dir, depot_dir):
    """Builds the run command and env from the supplied args."""

    lib_path = clean_relative_paths(json_file_dir, json_data["lib_path"])

    # Compose the environment variables to pass
    my_env = os.environ.copy()
    my_env["JULIA_LOAD_PATH"] = "{}:{}::".format(lib_path, lib_dir)
    my_env["JULIA_DEPOT_PATH"] = "{}:{}::".format(lib_path, depot_dir)

    # For now, we hard code the path of the shlibs relative to the json file.
    my_env["LD_LIBRARY_PATH"] = "{}:{}".format(
        os.path.join(lib_path, "../__shared_libs_symlink_tree__"),
        my_env.setdefault("LD_LIBRARY_PATH", ""),
    )

    binary_path = clean_relative_paths(json_file_dir, json_data["julia_binary"])

    main_file = clean_relative_paths(json_file_dir, json_data["main"])

    # Compose main julia command
    my_command = (
        [binary_path] + json_data["julia_flags"] + [main_file] + json_data["julia_args"]
    )

    return my_command, my_env


def main() -> int:
    """Sets up the julia environment with appropriate library aliases."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--json-path", default="")
    args = parser.parse_args()

    # pull everything from json file
    json_file = args.json_path
    json_file_dir = os.path.split(json_file)[0]
    with open(json_file) as f:
        json_data = json.load(f)

    # create a temporary directory to store artifacts. Note that this temporary
    # directory will be deleted when the process exits.
    with tempfile.TemporaryDirectory() as lib_dir:
        with tempfile.TemporaryDirectory() as depot_dir:
            parse_jll_libs(json_data["jll_mapping"], json_file_dir, lib_dir)
            my_command, my_env = build_command(
                json_data, json_file_dir, lib_dir, depot_dir
            )
            code = subprocess.call(my_command, env=my_env)

    sys.exit(code)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

"""
Create a bootstrapper pex for inplace python binaries

This script:
    - Writes out a bootstrapper pex script that knows where this symlink tree is,
      and uses it, along with the provided entry point to run the python script.
      It does this by replacing a few special strings like <MODULES_DIR> and
      <MAIN_MODULE>

A full usage might be something like this:

$ cat template.in
(see prelude/python/run_inplace_lite.py.in)
$ ./make_py_package_inplace.py  \\
    --template prelude/python/run_inplace.py.in \\
    # These two args create the hashbang for the bootstrapper script \\
    --python="/usr/bin/python3" \\
    --python-interpreter-flags="-Es" \\
    # This is based on the path in dests. This is the module that gets executed \\
    # to start program execution \\
    --entry-point=lib.foo  \\
    --output=bin.pex \\
    # This is the symlink tree \\
    --modules-dir=bin__link-tree
$ ./bin.pex
...
"""

import argparse
import os
import platform
import stat
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Create a python inplace binary, writing a symlink tree to a directory, "
            "and a bootstrapper pex file to file"
        ),
        fromfile_prefix_chars="@",
    )
    parser.add_argument(
        "--template",
        required=True,
        type=Path,
        help="The template file for the .pex bootstrapper script",
    )
    parser.add_argument(
        "--template-lite",
        required=True,
        type=Path,
        help="The template file for the .pex bootstrapper script, if it's simple",
    )

    parser.add_argument(
        "--preload",
        type=Path,
        dest="preload_libraries",
        action="append",
        default=[],
        help="A list of native libraries to add to LD_PRELOAD",
    )
    parser.add_argument(
        "--python",
        required=True,
        help="The python binary to put in the bootstrapper hashbang",
    )
    parser.add_argument(
        "--host-python",
        required=True,
        help="The host python binary to use to e.g. compiling bytecode",
    )
    parser.add_argument(
        "--python-interpreter-flags",
        default="-Es",
        help="The interpreter flags for the hashbang",
    )
    entry_point = parser.add_mutually_exclusive_group(required=True)
    entry_point.add_argument(
        "--entry-point",
        help="The main module to execute. Mutually exclusive with --main-function.",
    )
    entry_point.add_argument(
        "--main-function",
        help=(
            "Fully qualified name of the function that serves as the entry point."
            " Mutually exclusive with --entry-point."
        ),
    )
    parser.add_argument(
        "--main-runner",
        help=(
            "Fully qualified name of a function that handles invoking the"
            " executable's entry point."
        ),
        required=True,
    )
    parser.add_argument(
        "--modules-dir",
        required=True,
        type=Path,
        help="The link tree directory to use at runtime",
    )
    parser.add_argument(
        "--use-lite",
        help="Whether to use the lite template",
        action="store_true",
    )
    parser.add_argument(
        "output",
        type=Path,
        help="Where to write the bootstrapper script to",
    )
    parser.add_argument(
        "--native-libs-env-var",
        default=(
            "DYLD_LIBRARY_PATH" if platform.system() == "Darwin" else "LD_LIBRARY_PATH"
        ),
        help="The dynamic loader env used to find native library deps",
    )
    parser.add_argument(
        "-e",
        "--runtime_env",
        action="append",
        default=[],
        help="environment variables to set before launching the runtime. (e.g. -e FOO=BAR BAZ=QUX)",
    )
    # Compatibility with existing make_par scripts
    parser.add_argument("--passthrough", action="append", default=[])

    return parser.parse_args()


def write_bootstrapper(args: argparse.Namespace) -> None:
    """Write the .pex bootstrapper script using a template"""

    template = (
        args.template_lite
        if (args.use_lite and not args.runtime_env)
        else args.template
    )
    with open(template, "r", encoding="utf8") as fin:
        data = fin.read()

    # Because this can be invoked from other directories, find the relative path
    # from this .par to the modules dir, and use that.
    relative_modules_dir = os.path.relpath(args.modules_dir, args.output.parent)

    # TODO(nmj): Remove this hack. So, if arg0 in your shebang is a bash script
    #                 (like /usr/local/fbcode/platform007/bin/python3.7 on macs is)
    #                 OSX just sort of ignores it and tries to run your thing with
    #                 the current shell. So, we hack in /usr/bin/env in the front
    #                 for now, and let it do the lifting. OSX: Bringing you the best
    #                 of 1980s BSD in 2021...
    #                 Also, make sure we add PYTHON_INTERPRETER_FLAGS back. We had to
    #                 exclude it for now, because linux doesn't like multiple args
    #                 after /usr/bin/env

    ld_preload = "None"
    if args.preload_libraries:
        ld_preload = repr(":".join(p.name for p in args.preload_libraries))

    new_data = data.replace("<PYTHON>", "/usr/bin/env " + str(args.python))
    new_data = new_data.replace("<PYTHON_INTERPRETER_FLAGS>", "")

    new_data = new_data.replace("<MODULES_DIR>", str(relative_modules_dir))
    main_module = args.entry_point
    main_function = ""
    if args.main_function:
        main_module, main_function = args.main_function.rsplit(".", 1)
    new_data = new_data.replace("<MAIN_MODULE>", main_module)
    new_data = new_data.replace("<MAIN_FUNCTION>", main_function)

    main_runner_module, main_runner_function = args.main_runner.rsplit(".", 1)
    new_data = new_data.replace("<MAIN_RUNNER_MODULE>", main_runner_module)
    new_data = new_data.replace("<MAIN_RUNNER_FUNCTION>", main_runner_function)

    # Things that are only required for the full template
    new_data = new_data.replace("<NATIVE_LIBS_ENV_VAR>", args.native_libs_env_var)
    new_data = new_data.replace("<NATIVE_LIBS_DIR>", repr(relative_modules_dir))
    new_data = new_data.replace("<NATIVE_LIBS_PRELOAD_ENV_VAR>", "LD_PRELOAD")
    new_data = new_data.replace("<NATIVE_LIBS_PRELOAD>", ld_preload)

    if args.runtime_env:
        runtime_env = dict(e.split("=", maxsplit=1) for e in args.runtime_env)
        env = f"os.environ.update({runtime_env!r})"
    else:
        env = ""
    new_data = new_data.replace("<ENV>", env)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with open(args.output, "w", encoding="utf8") as fout:
        fout.write(new_data)
    mode = os.stat(args.output).st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH
    os.chmod(args.output, mode)


def main() -> None:
    args = parse_args()
    write_bootstrapper(args)


if __name__ == "__main__":
    main()

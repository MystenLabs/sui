# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

"""
Run a crate's Cargo buildscript.
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Dict, IO, NamedTuple


IS_WINDOWS: bool = os.name == "nt"


def eprint(*args: Any, **kwargs: Any) -> None:
    print(*args, end="\n", file=sys.stderr, flush=True, **kwargs)


def cfg_env(rustc_cfg: Path) -> Dict[str, str]:
    with rustc_cfg.open(encoding="utf-8") as f:
        lines = f.readlines()

    cfgs: Dict[str, str] = {}
    for line in lines:
        if (
            line.startswith("unix")
            or line.startswith("windows")
            or line.startswith("target_")
        ):
            keyval = line.strip().split("=")
            key = keyval[0]
            val = keyval[1].replace('"', "") if len(keyval) > 1 else "1"

            key = "CARGO_CFG_" + key.upper()
            if key in cfgs:
                cfgs[key] = cfgs[key] + "," + val
            else:
                cfgs[key] = val

    return cfgs


def create_cwd(path: Path, manifest_dir: Path) -> Path:
    """Create a directory with most of the same contents as manifest_dir, but
    excluding Rustup's rust-toolchain.toml configuration file.

    Keeping rust-toolchain.toml goes wrong in the situation that all of the
    following happen:

      1. toolchains//:rust uses compiler = "rustc", like the
         system_rust_toolchain.

      2. The rustc in $PATH is rustup's rustc shim.

      3. A third-party dependency has both a rust-toolchain.toml and a build.rs
         that runs "rustc" or env::var_os("RUSTC"), such as to inspect `rustc
         --version` or to compile autocfg-style probe code.

    Cargo defines that build scripts run using the package's manifest directory
    as the current directory, so the rustc subprocess spawned from build.rs
    would also run in that manifest directory. But other rustc invocations
    performed by Buck run from the repo root.

    Rustup only looks at one rust-toolchain.toml file, using the nearest one
    present in any parent directory. The file can set `channel` to control which
    installed version of rustc to run.

    It is bad if it's possible for the rustc run by a build script vs rustc run
    by the rest of the build to be different toolchains. In order to configure
    their crate appropriately, build scripts rely on using the same rustc that
    their crate will be later compiled by.

    This problem doesn't happen during Cargo-based builds because rustup
    installs both a cargo shim and a rustc shim. When you run a rustup-managed
    Cargo, one of the first things it does is define a RUSTUP_TOOLCHAIN
    environment variable pointing to the rustup channel id of the currently
    selected cargo. Subsequent invocations of the rustup cargo shim or rustc
    shim with this variable in the environment no longer pay attention to any
    rust-toolchain.toml file.

    We cannot follow the same approach because there is no API in rustup for
    finding out a suitable RUSTUP_TOOLCHAIN value consistent with which
    toolchain "rustc" currently refers to, and even if there were, it isn't
    guaranteed that "rustc" refers to a rustup-managed toolchain in the first
    place.
    """

    path.mkdir(exist_ok=True)

    for dir_entry in manifest_dir.iterdir():
        if dir_entry.name not in ["rust-toolchain", "rust-toolchain.toml"]:
            link = path.joinpath(dir_entry.name)
            link.unlink(missing_ok=True)
            link.symlink_to(os.path.relpath(dir_entry, path))

    return path


# In some environments, invoking the rustc binary may actually invoke another
# tool that fetches the binary from a remote location. This fetch may encounter
# network errors. Ideally, build scripts that invoke rustc would reliably fail
# when such a thing happens, but in practice they don't. To mitigate, we
# manually invoke `rustc --version` and make sure that succeeds.
def ensure_rustc_available(
    env: Dict[str, str],
    cwd: Path,
) -> None:
    rustc, target = env.get("RUSTC"), env.get("TARGET")
    assert rustc is not None, "RUSTC env is missing"
    assert target is not None, "TARGET env is missing"

    # NOTE: `HOST` is optional.
    host = env.get("HOST")

    try:
        # Run through cmd.exe on Windows so if rustc is a batch script
        # (like the command_alias trampoline is), it is found relative to
        # cwd.
        #
        # Executing `os.path.join(cwd, rustc)` would also work, but because
        # of `../` in the path, it's possible to hit path length limits.
        # Resolving it would remove the `..` but then sometimes things
        # fail with exit code `3221225725` ("out of stack memory").
        # I suspect it's some infinite loop brought about by the trampoline
        # and symlinks.
        subprocess.check_output(  # noqa: P204
            [rustc, "--version"],
            cwd=cwd,
            shell=IS_WINDOWS,
        )
        # A multiplexed sysroot may involve another fetch,
        # so pass `--target` to check that too.
        if host != target:
            subprocess.check_output(  # noqa: P204
                [rustc, f"--target={target}", "--version"],
                cwd=cwd,
                shell=IS_WINDOWS,
            )
    except OSError as ex:
        eprint(f"Failed to run {rustc} because {ex}")
        sys.exit(1)
    except subprocess.CalledProcessError as ex:
        eprint(f"Command failed with exit code {ex.returncode}")
        eprint(f"Command: {ex.cmd}")
        if ex.stdout:
            eprint(f"Stdout: {ex.stdout}")
        sys.exit(1)


def run_buildscript(
    buildscript: str,
    env: Dict[str, str],
    cwd: Path,
) -> str:
    try:
        return subprocess.check_output(
            os.path.abspath(buildscript),
            encoding="utf-8",
            env=env,
            cwd=cwd,
        )
    except OSError as ex:
        print(f"Failed to run {buildscript} because {ex}", file=sys.stderr)
        sys.exit(1)
    except subprocess.CalledProcessError as ex:
        sys.exit(ex.returncode)


class Args(NamedTuple):
    buildscript: str
    rustc_cfg: Path
    manifest_dir: Path
    create_cwd: Path
    outfile: IO[str]


def arg_parse() -> Args:
    parser = argparse.ArgumentParser(description="Run Rust build script")
    parser.add_argument("--buildscript", type=str, required=True)
    parser.add_argument("--rustc-cfg", type=Path, required=True)
    parser.add_argument("--manifest-dir", type=Path, required=True)
    parser.add_argument("--create-cwd", type=Path, required=True)
    parser.add_argument("--outfile", type=argparse.FileType("w"), required=True)

    return Args(**vars(parser.parse_args()))


def main() -> None:  # noqa: C901
    args = arg_parse()

    env = cfg_env(args.rustc_cfg)

    out_dir = os.getenv("OUT_DIR")
    assert out_dir is not None, "OUT_DIR env is missing"
    os.makedirs(out_dir, exist_ok=True)
    env["OUT_DIR"] = os.path.abspath(out_dir)

    cwd = create_cwd(args.create_cwd, args.manifest_dir)
    env["CARGO_MANIFEST_DIR"] = os.path.abspath(cwd)

    env = dict(os.environ, **env)

    ensure_rustc_available(env=env, cwd=cwd)

    script_output = run_buildscript(args.buildscript, env=env, cwd=cwd)

    cargo_rustc_cfg_pattern = re.compile("^cargo:rustc-cfg=(.*)")
    flags = ""
    for line in script_output.split("\n"):
        cargo_rustc_cfg_match = cargo_rustc_cfg_pattern.match(line)
        if cargo_rustc_cfg_match:
            flags += "--cfg={}\n".format(cargo_rustc_cfg_match.group(1))
        else:
            print(line, end="\n")
    args.outfile.write(flags)


if __name__ == "__main__":
    main()

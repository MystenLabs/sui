"""
Run buildah to create an image and export it locally as an artifact/output
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Dict, List
from contextlib import contextmanager


def arg_parse():
    parser = argparse.ArgumentParser(description="Run Buildah build script")
    parser.add_argument("--buildah", type=str, required=True)
    parser.add_argument("--image_id", type=str, required=True)
    parser.add_argument("--docker_root", type=str, required=True)
    parser.add_argument("--out", type=str, required=True)
    return parser


@contextmanager
def set_directory(path: Path):
    origin = Path().absolute()
    try:
        os.chdir(path)
        yield
    finally:
        os.chdir(origin)


def run(cmd: List[str]) -> None:
    try:
        # Run the subprocess and capture stdout and stderr
        result = subprocess.run(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=os.environ.copy()
        )
    except Exception as e:
        print(f"Failed to run {cmd} because {e}", file=sys.stderr)
        sys.exit(1)

    if result.returncode != 0:
        print(f"stdout: {result.stdout}")
        print(f"stderr: {result.stderr}")
        sys.exit(result.returncode)

    print(result.stdout)


def main(args: argparse.Namespace) -> None:
    dirname, filename = os.path.split(args.out)
    cmd = [
        args.buildah,
        "push",
        args.image_id,
        f"oci-archive:{dirname}:{filename}joes_tag",
        "--logfile",
        "logfile.log",
    ]

    # buildah wants docker args as build-args.  re-map this from buck
    for ba in args.build_arg:
        ba.insert(0, "--build-arg")
        cmd.extend(ba)

    # terrible things happen if we don't execute buildah in the same directory as the
    # Dockerfile resides in.  Probabaly better to invest in a go bin instead of
    # horsing around with buildah directly
    with set_directory(Path(args.docker_root)):
        run(cmd)


if __name__ == "__main__":
    args = arg_parse().parse_args()
    main(args)

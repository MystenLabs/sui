"""
Run buildah to create an image and export it locally as an artifact/output
"""

import argparse
import os
import json
from datetime import datetime, timezone
import subprocess
import sys
from pathlib import Path
from typing import List, Tuple, Dict
from contextlib import contextmanager
import uuid
import shutil


def arg_parse():
    parser = argparse.ArgumentParser(description="Run Buildah build script")
    parser.add_argument("--buildah", type=str, required=True)
    parser.add_argument("--image_name", type=str, required=True)
    parser.add_argument("--registry", type=str, required=False)
    parser.add_argument("--gcloud", type=str, required=False)
    parser.add_argument("--docker_root", type=str, required=True)
    parser.add_argument("--log_level", type=str, required=True)
    parser.add_argument("--build-arg", type=str, action="append", nargs="*", default=[])
    parser.add_argument("--tag", type=str, action="append", nargs="*", default=[])
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


def run(cmd: List[str]) -> str:
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

    return result.stdout


def read_json(filepath: str) -> Dict:
    """given a filepath, read the contents as json and return a dict"""
    try:
        with open(filepath, "r") as file:
            return json.load(file)
    except FileNotFoundError:
        print(f"file not found: {filepath}", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as e:
        print(f"error decoding JSON: {e}", file=sys.stderr)
        sys.exit(1)


def timestamp_to_utc_string(timestamp: int) -> str:
    # Convert the timestamp to a datetime object
    dt_object = datetime.utcfromtimestamp(timestamp)

    # Convert the datetime object to a UTC string
    utc_string = dt_object.replace(tzinfo=timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    return utc_string


def now() -> int:
    """unix utc time now"""
    return int(datetime.utcnow().timestamp())


def split_registry(registry: str) -> Tuple[str, str, str]:
    # assuming registry is in the form us-central1-docker.pkg.dev/YOUR_PROJECT_ID/PROJECT_URI
    split = registry.split("/")
    if len(split) != 3:
        print(f"failed to split registry string {registry}", file=sys.stderr)
        sys.exit(1)
    (prefix, project_id, project_uri) = split[0], split[1], split[2]
    return (prefix, project_id, project_uri)


def valid_container_registry(registry: str) -> bool:
    """our known valid remote registries"""
    print(f"attempt to extract match for registry: {registry}")
    # google artifact registry has some wack urls, try to extract domains
    allowed = {"docker.pkg.dev": push_to_gcr}
    for uri, fn in allowed.items():
        if uri in registry:
            return fn
    return None


def login_to_gcr(buildah: str, gcloud: str, registry: str) -> None:
    (registry_uri, _, _) = split_registry(registry)
    gcloud = [gcloud, "auth", "print-access-token"]
    # decode the token and trim newlines.
    gcloud_access_token = run(gcloud).decode("utf-8").strip()
    cmd = [
        buildah,
        "login",
        "-v",
        "--tls-verify=true",
        "-u",
        "oauth2accesstoken",
        "--password",
        gcloud_access_token,
        registry_uri,
    ]
    run(cmd)


def push_to_gcr(build_id: str, args: argparse.Namespace):
    # buildah push --tls-verify=false localhost:5000/my-image:latest us-central1-docker.pkg.dev/YOUR_PROJECT_ID/PROJECT_URI/myimage:tag
    login_to_gcr(args.buildah, args.gcloud, args.registry)
    (registry_uri, project_id, project_uri) = split_registry(args.registry)
    # this path is guaranteed by buck
    meta = read_json("materialized_meta.json")
    tag = "-".join(meta.get("tags", ["latest"]))
    cmd = [
        args.buildah,
        "push",
        "--tls-verify=true",
        f"localhost/{args.image_name}:{build_id}",
        f"{registry_uri}/{project_id}/{project_uri}/{args.image_name}:{build_id}",
    ]
    print(" ".join(cmd))
    run(cmd)


def build(build_id: str, args: argparse.Namespace) -> None:
    """
    build will use buildah to create an image locally
    we do not push to remote repos in this stage
    """
    # this path is guaranteed by buck
    meta = read_json("materialized_meta.json")
    tag = "-".join(meta.get("tags", ["latest"]))
    cmd = [
        args.buildah,
        "bud",
        "-t",
        f"localhost/{args.image_name}:{build_id}",
        "--logfile",
        "build_logfile.log",
        "--pull",
        "--log-level",
        args.log_level,
    ]

    # stich in some build data from mypkg
    build_args = [
        ["AUTHOR={}".format(meta["author"])],
        ["BUILD_ID={}".format(build_id)],
        ["BUILD_SYSTEM=buildah"],  # only buildah atm
        ["BUILD_DATE={}".format(timestamp_to_utc_string(meta["built_on"]))],
        ["SHA256={}".format(meta.get("sha256_hash", "missing"))],
        ["COMMIT={}".format(meta.get("commit", "missing"))],
    ]
    build_args.extend(args.build_arg)

    # buildah wants docker args as build-args.  re-map this from buck
    for ba in build_args:
        ba.insert(0, "--build-arg")
        cmd.extend(ba)
    print(" ".join(cmd))
    run(cmd)


def push_tar_image_localhost(build_id: str, args: argparse.Namespace):
    """
    export our image from the local container registry into bucks build root
    for this scriipt.  we always store a container locally so we can push it
    to multiple other locations easily. it also allows us to use podman
    locally after the fact.
    """
    dirname, filename = os.path.split(args.out)
    print(f"using dirname {dirname} filename {filename}")
    # important to use the .. here to place the tar in our parent directory
    # buck needs it there bc of the _buildah_image_impl design
    # TODO possibly refactor that design
    exported_image = os.path.join(os.getcwd(), "..", filename)

    # this path is guaranteed by buck
    meta = read_json("materialized_meta.json")
    tag = "-".join(meta.get("tags", ["latest"]))

    export_cmd = [
        args.buildah,
        "push",
        f"localhost/{args.image_name}:{build_id}",
        f"oci-archive:{exported_image}",
    ]
    run(export_cmd)


def main(args: argparse.Namespace) -> None:
    build_id = uuid.uuid4().hex
    # terrible things happen if we don't execute buildah in the same directory as the
    # Dockerfile resides in.  Probabaly better to invest in a go bin instead of
    # horsing around with buildah directly
    with set_directory(Path(args.docker_root)):
        build(build_id, args)
        push_tar_image_localhost(build_id, args)

        if not args.registry:
            return

        fn = valid_container_registry(args.registry)
        if not fn:
            print(
                f"Failed to match registry with export func: {args.registry}",
                file=sys.stderr,
            )
            sys.exit(1)
        fn(build_id, args)


if __name__ == "__main__":
    args = arg_parse().parse_args()
    main(args)

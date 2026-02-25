#!/bin/sh
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# fast fail.
set -e

DIR="$( cd "$( dirname "$0" )" && pwd )"
REPO_ROOT="$(git rev-parse --show-toplevel)"
OCI_OUTPUT="$REPO_ROOT/build/oci"
DOCKERFILE="$DIR/Dockerfile"
GIT_REVISION="$(git describe --always --abbrev=12 --dirty --exclude '*')"
BUILD_DATE="$(date -u +'%Y-%m-%d')"
PLATFORM="linux/amd64"

echo
echo "Building multisig-toolkit docker image"
echo "Dockerfile: \t$DOCKERFILE"
echo "docker context: $REPO_ROOT"
echo "git revision: \t$GIT_REVISION"
echo "output directory: \t$OCI_OUTPUT"
echo

export DOCKER_BUILDKIT=1
export SOURCE_DATE_EPOCH=1

# Create output directory if it doesn't exist
mkdir -p "$OCI_OUTPUT"

docker build -f "$DOCKERFILE" "$REPO_ROOT" \
	--build-arg GIT_REVISION="$GIT_REVISION" \
	--build-arg SOURCE_DATE_EPOCH="$SOURCE_DATE_EPOCH" \
	--output type=oci,rewrite-timestamp=true,force-compression=true,tar=false,dest=$OCI_OUTPUT/multisig-toolkit,name=multisig-toolkit \
	--platform "$PLATFORM" \
	--tag multisig-toolkit:latest \
	"$@"

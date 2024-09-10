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

# option to build using debug symbols
if [ "$1" = "--debug-symbols" ]; then
	PROFILE="bench"
	echo "Building with full debug info enabled ... WARNING: binary size might significantly increase"
	shift
else
	PROFILE="release"
fi

echo
echo "Building sui-node docker image"
echo "Dockerfile: \t$DOCKERFILE"
echo "docker context: $REPO_ROOT"
echo "build date: \t$BUILD_DATE"
echo "git revision: \t$GIT_REVISION"
echo "output directory: \t$OCI_OUTPUT"
echo

export DOCKER_BUILDKIT=1
export SOURCE_DATE_EPOCH=1

docker build -f "$DOCKERFILE" "$REPO_ROOT" \
	--build-arg GIT_REVISION="$GIT_REVISION" \
	--build-arg BUILD_DATE="$BUILD_DATE" \
	--build-arg PROFILE="$PROFILE" \
	--platform "$PLATFORM" \
	--output type=oci,rewrite-timestamp=true,force-compression=true,tar=false,dest=$OCI_OUTPUT/sui-node,name=sui-node \
	"$@"

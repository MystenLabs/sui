#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

if ! cosign version &> /dev/null
then
    echo "cosign is not installed, Please install cosign for binary verification."
    echo "https://docs.sigstore.dev/cosign/installation"
    exit 1
fi

if [ -z "$1" ]; then
    echo "Usage: $0 <commit-sha>"
    exit 1
fi

commit_sha=$1
pub_key=https://sui-private.s3.us-west-2.amazonaws.com/sui_security_release.pem
url=https://sui-releases.s3-accelerate.amazonaws.com/$commit_sha

echo "[+] Downloading sui binaries for $commit_sha ..."
for binary in sui sui-node sui-tool; do
    if ! curl -fSs "$url/$binary" -o "$binary"; then
        echo "Error: failed to download $url/$binary (check the commit sha)"
        exit 1
    fi
done

echo "[+] Verifying sui binaries for $commit_sha ..."
for binary in sui sui-node sui-tool; do
    cosign verify-blob --insecure-ignore-tlog --key "$pub_key" --signature "$url/$binary.sig" "$binary"
done

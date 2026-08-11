#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

if ! cosign version &> /dev/null
then
    echo "cosign is not installed, Please install cosign for binary verification."
    echo "https://docs.sigstore.dev/cosign/installation"
    exit 1
fi

if [ -z "$1" ] || [ -z "$2" ]; then
    echo "Usage: $0 <commit-sha> <binary-name>"
    exit 1
fi

commit_sha=$1
binary_name=$2
pub_key=https://sui-private.s3.us-west-2.amazonaws.com/sui_security_release.pem
url=https://sui-releases.s3-accelerate.amazonaws.com/$commit_sha

echo "[+] Downloading binary '$binary_name' for $commit_sha ..."
if ! curl -fSs "$url/$binary_name" -o "$binary_name"; then
    echo "Error: failed to download $url/$binary_name (check the commit sha and binary name)"
    exit 1
fi

echo "[+] Verifying binary '$binary_name' for $commit_sha ..."
cosign verify-blob --insecure-ignore-tlog --key "$pub_key" --signature "$url/$binary_name.sig" "$binary_name"

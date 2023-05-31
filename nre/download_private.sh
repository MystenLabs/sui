#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

if ! cosign version &> /dev/null
then
    echo "cosign in not installed, Please install cosign for binary verification."
    echo "https://docs.sigstore.dev/cosign/installation"
    exit
fi

commit_sha=$1
pub_key=https://sui-private.s3.us-west-2.amazonaws.com/sui_security_release.pem
url=https://sui-releases.s3-accelerate.amazonaws.com/$commit_sha

echo "[+] Downloading sui binaries for $commit_sha ..."
curl $url/sui -o sui
curl $url/sui-indexer -o sui-indexer
curl $url/sui-node -o sui-node
curl $url/sui-tool -o sui-tool

echo "[+] Verifying sui binaries for $commit_sha ..."
cosign verify-blob --insecure-ignore-tlog --key $pub_key --signature $url/sui.sig sui
cosign verify-blob --insecure-ignore-tlog --key $pub_key --signature $url/sui-indexer.sig sui-indexer
cosign verify-blob --insecure-ignore-tlog --key $pub_key --signature $url/sui-node.sig sui-node
cosign verify-blob --insecure-ignore-tlog --key $pub_key --signature $url/sui-tool.sig sui-tool

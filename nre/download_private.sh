#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

if ! cosign version &> /dev/null
then
    echo "cosign in not installed, Please install cosign for binary verification."
    echo "https://docs.sigstore.dev/cosign/installation"
    exit
fi

randomstring=$1
pub_key=https://sui-private.s3.us-west-2.amazonaws.com/sui_security_release.pem
url=https://sui-private.s3.us-west-2.amazonaws.com/$randomstring

echo "[+] Downloading docker artifacts for $randomstring ..."
curl $url/sui-node-docker.tar -o sui-node-docker.tar
curl $url/sui-tools-docker.tar -o sui-tools-docker.tar
curl $url/sui-indexer-docker.tar -o sui-indexer-docker.tar

echo "[+] Verifying docker artifacts for $randomstring ..."
cosign verify-blob --insecure-ignore-tlog --key $pub_key --signature $url/sui-node-docker.tar.sig sui-node-docker.tar
cosign verify-blob --insecure-ignore-tlog --key $pub_key --signature $url/sui-tools-docker.tar.sig sui-tools-docker.tar
cosign verify-blob --insecure-ignore-tlog --key $pub_key --signature $url/sui-indexer-docker.tar.sig sui-indexer-docker.tar

echo "[+] Downloading sui binaries for $randomstring ..."
curl $url/sui -o sui
curl $url/sui-indexer -o sui-indexer
curl $url/sui-node -o sui-node
curl $url/sui-tool -o sui-tool

echo "[+] Verifying sui binaries for $randomstring ..."
cosign verify-blob --insecure-ignore-tlog --key $pub_key --signature $url/sui.sig sui
cosign verify-blob --insecure-ignore-tlog --key $pub_key --signature $url/sui-indexer.sig sui-indexer
cosign verify-blob --insecure-ignore-tlog --key $pub_key --signature $url/sui-node.sig sui-node


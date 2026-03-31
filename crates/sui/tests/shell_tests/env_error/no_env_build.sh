# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# This tests the error message when you set your local client to an ephemeral network and then do `sui move build`

echo "== should fail and ask user to provide -e =="
sui move --client.config client.yaml build

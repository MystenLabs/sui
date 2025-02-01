# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# simple test just to make sure the test runner works with the network
sui client --client.config $CONFIG objects --json | jq 'length'

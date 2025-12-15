# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that sui move new followed by sui move build succeeds

sui move --client.config $CONFIG new example
cd example && sui move --client.config $CONFIG build

# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# check that sui move new followed by sui move test succeeds
sui move new example

cd example && sui move test

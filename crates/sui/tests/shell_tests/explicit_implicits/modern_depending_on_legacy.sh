# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that building a package that depends on a legacy that has explicit System deps can build successfully 
sui move --client.config $CONFIG build -p modern_depending_on_legacy

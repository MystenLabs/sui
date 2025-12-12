# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that building a legacy package that has explicit deps works fine
sui move --client.config $CONFIG build -p legacy_can_use_legacy

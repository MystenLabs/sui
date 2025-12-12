# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# tests that building a package that has explicit deps (through dep-replacements) on legacy system names errors
sui move --client.config $CONFIG build -p modern_using_legacy_name_as_replacement

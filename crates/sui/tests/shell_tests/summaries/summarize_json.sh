# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Default output format is JSON
sui move summary --path data/move_package
ls -1 data/move_package/package_summaries
ls -1 data/move_package/package_summaries/move_package
ls -1 data/move_package/package_summaries/sui
ls -1 data/move_package/package_summaries/std
# NB: Not root_package_metadata for source packages as these are not generated w.r.t. a specific network.
cat data/move_package/package_summaries/root_package_metadata.json
# This will contain the address mapping for the package from the literal value
# of the address to the named value of the address (which is what the source
# package metadata uses for storage).
cat data/move_package/package_summaries/address_mapping.json

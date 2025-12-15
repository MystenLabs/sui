# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Default output format is JSON
sui move --client.config $CONFIG summary --path data/move_package
ls -1 data/move_package/package_summaries | LC_ALL=C sort -f
ls -1 data/move_package/package_summaries/move_package | LC_ALL=C sort -f
ls -1 data/move_package/package_summaries/sui | LC_ALL=C sort -f
ls -1 data/move_package/package_summaries/std | LC_ALL=C sort -f
# NB: Not root_package_metadata for source packages as these are not generated w.r.t. a specific network.
cat data/move_package/package_summaries/root_package_metadata.json
echo
# This will contain the address mapping for the package from the literal value
# of the address to the named value of the address (which is what the source
# package metadata uses for storage).
cat data/move_package/package_summaries/address_mapping.json

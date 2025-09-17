# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

sui move --client.config $CONFIG summary --package-id 0x2 -o yaml --bytecode
ls -1 package_summaries | LC_ALL=C sort -f
ls -1 package_summaries/0x0000000000000000000000000000000000000000000000000000000000000002 | LC_ALL=C sort -f
ls -1 package_summaries/0x0000000000000000000000000000000000000000000000000000000000000001 | LC_ALL=C sort -f
cat package_summaries/root_package_metadata.yaml
cat package_summaries/address_mapping.yaml

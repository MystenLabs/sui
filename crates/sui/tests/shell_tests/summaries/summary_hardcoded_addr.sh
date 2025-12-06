# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Default output format is JSON
sui move --client.config $CONFIG summary --path data/pkg_with_hardcoded_addr
ls -1 data/pkg_with_hardcoded_addr/package_summaries/
cat data/pkg_with_hardcoded_addr/package_summaries/summary_pkg/a.json
cat data/pkg_with_hardcoded_addr/package_summaries/summary_pkg/b.json
cat data/pkg_with_hardcoded_addr/package_summaries/summary_pkg/b.json
cat data/pkg_with_hardcoded_addr/package_summaries/address_mapping.json

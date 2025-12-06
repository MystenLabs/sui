# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# This should succeed - the manifest has a broken dep, but the lockfile has it pinned to the correct location
# Repinning shouldn't be retriggered just from an added comment
echo '# comment' >> Move.toml
sui move --client.config $CONFIG build

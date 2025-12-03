# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Active environment chain ID matches multiple envs in the manifest
echo 'duplicate_env = "1234"' >> Move.toml
sui move --client.config configs/name_mismatch_id_match.yaml build

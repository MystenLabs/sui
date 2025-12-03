# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# If the active environment has no chain ID and we can't get it, we use the environment name
sui move --client.config configs/name_match_id_none.yaml build

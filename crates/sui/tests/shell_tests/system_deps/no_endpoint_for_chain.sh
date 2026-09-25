# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# Building for `custom_env` when no configured environment serves its chain, and it isn't one of
# the well-known public networks either, so there is nothing to ask.
sui move --client.config configs/serves_other_chain.yaml build -e custom_env -p example

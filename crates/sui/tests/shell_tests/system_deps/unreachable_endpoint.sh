# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# The configured environment serves the right chain, but its endpoint can't be reached, so the
# framework version for `custom_env` is unknown. We must not guess one: pinning the newest
# framework we ship would compile locally and then fail at publish.
sui move --client.config configs/serves_chain.yaml build -p example

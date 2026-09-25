# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# A package with no system dependencies builds even though no endpoint can be reached: the
# framework version is only resolved when some package actually needs it.
sui move --client.config configs/serves_chain.yaml build -p no_system_deps

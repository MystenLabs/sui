# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# If `-y` is passed and the config file doesn't exist, it is created
sui client --client.config ./client.yaml envs -y
cat client.yaml
cat sui.keystore

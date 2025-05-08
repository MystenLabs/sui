# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

sui client --client.config config.yaml switch --env base

sui client --client.config config.yaml envs
sui client --client.config config.yaml --client.env one envs
sui client --client.config config.yaml --client.env two envs

sui client --client.config config.yaml active-env
sui client --client.config config.yaml --client.env one active-env
sui client --client.config config.yaml --client.env two active-env

# Unknown name -- Should give you None and nothing active
sui client --client.config config.yaml --client.env not_an_env envs
sui client --client.config config.yaml --client.env not_an_env active-env

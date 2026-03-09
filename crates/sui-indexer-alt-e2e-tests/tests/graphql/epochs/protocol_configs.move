// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --accounts A --simulator

//# create-checkpoint

//# run-graphql
{ # Protocol Configs that don't exist (because they haven't been used in the
  # chain being indexed) -- their config lists will be empty.
  before: protocolConfigs(version: 107) {
    protocolVersion
    configs { key value }
  }

  after: protocolConfigs(version: 109) {
    protocolVersion
    configs { key value }
  }
}

//# run-graphql
{
  protocolConfigs(version: 108) {
    config(key: "max_move_object_size") { key value }
    featureFlag(key: "enable_effects_v2") { key value }
  }
}

//# run-graphql
{ # Latest protocol config
  protocolConfigs { protocolVersion }
}

//# run-graphql
{ # Fetch protocol config version via epoch
  epoch(epochId: 0) { protocolConfigs { protocolVersion } }

  # Fetch protocol config via version
  protocolConfigs(version: 108) { protocolVersion }
}

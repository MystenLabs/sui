// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 48 --simulator --accounts C

//# create-checkpoint

//# run-graphql
{
    protocolConfig {
        config(key: "max_move_identifier_len") {
            value
        }
        featureFlag(key: "enable_coin_deny_list") {
            value
        }
    }
}

//# run-graphql
{
    protocolConfig(protocolVersion: 8) {
        config(key: "max_move_identifier_len") {
            value
        }
        featureFlag(key: "enable_coin_deny_list") {
            value
        }
    }
}

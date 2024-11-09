// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::super::ids::UserId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct UserCommon {
    pub id: UserId,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct Person {
    pub email: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct Bot {
    pub email: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum User {
    Person {
        #[serde(flatten)]
        common: UserCommon,
        person: Person,
    },
    Bot {
        #[serde(flatten)]
        common: UserCommon,
        bot: Bot,
    },
}

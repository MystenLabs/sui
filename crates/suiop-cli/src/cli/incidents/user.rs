use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::cli::slack::SlackUser;

use super::notion::NotionPerson;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub(crate) slack_user: Option<SlackUser>,
    pub(crate) notion_user: Option<NotionPerson>,
}

impl User {
    pub fn new(slack_user: Option<SlackUser>, notion_user: Option<NotionPerson>) -> Option<User> {
        if slack_user.is_none() && notion_user.is_none() {
            None
        } else {
            Some(User {
                slack_user,
                notion_user,
            })
        }
    }
}

impl Display for User {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let name = self.slack_user.as_ref().map(|u| u.name.clone());
        if let Some(name) = name {
            write!(f, "{}", name)
        } else {
            write!(
                f,
                "{}",
                self.notion_user
                    .as_ref()
                    .expect("expected notion user")
                    .name
            )
        }
    }
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]
pub mod block;
pub mod error;
pub mod paging;
pub mod properties;
pub mod search;
#[cfg(test)]
mod tests;
pub mod text;
pub mod users;

use super::Error;
use block::ExternalFileObject;
use properties::{PropertyConfiguration, PropertyValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use text::RichText;

use super::ids::{AsIdentifier, DatabaseId, PageId};
use block::{Block, CreateBlock, FileObject};
pub use chrono::{DateTime, Utc};
use error::ErrorResponse;
use paging::PagingCursor;
pub use serde_json::value::Number;
use users::User;

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Copy, Clone)]
#[serde(rename_all = "snake_case")]
enum ObjectType {
    Database,
    List,
}

/// Represents a Notion Database
/// See <https://developers.notion.com/reference/database>
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Database {
    /// Unique identifier for the database.
    pub id: DatabaseId,
    /// Date and time when this database was created.
    pub created_time: DateTime<Utc>,
    /// Date and time when this database was updated.
    pub last_edited_time: DateTime<Utc>,
    /// Name of the database as it appears in Notion.
    pub title: Vec<RichText>,
    /// Schema of properties for the database as they appear in Notion.
    //
    // key string
    // The name of the property as it appears in Notion.
    //
    // value object
    // A Property object.
    pub icon: Option<IconObject>,
    pub properties: HashMap<String, PropertyConfiguration>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum IconObject {
    File {
        #[serde(flatten)]
        file: FileObject,
    },
    External {
        external: ExternalFileObject,
    },
    Emoji {
        emoji: String,
    },
}

impl AsIdentifier<DatabaseId> for Database {
    fn as_id(&self) -> &DatabaseId {
        &self.id
    }
}

impl Database {
    pub fn title_plain_text(&self) -> String {
        self.title
            .iter()
            .flat_map(|rich_text| rich_text.plain_text().chars())
            .collect()
    }
}

/// <https://developers.notion.com/reference/pagination#responses>
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct ListResponse<T> {
    pub results: Vec<T>,
    pub next_cursor: Option<PagingCursor>,
    pub has_more: bool,
}

impl<T> ListResponse<T> {
    pub fn results(&self) -> &[T] {
        &self.results
    }
}

impl ListResponse<Object> {
    pub fn only_databases(self) -> ListResponse<Database> {
        let databases = self
            .results
            .into_iter()
            .filter_map(|object| match object {
                Object::Database { database } => Some(database),
                _ => None,
            })
            .collect();

        ListResponse {
            results: databases,
            has_more: self.has_more,
            next_cursor: self.next_cursor,
        }
    }

    pub(crate) fn expect_databases(self) -> Result<ListResponse<Database>, super::Error> {
        let databases: Result<Vec<_>, _> = self
            .results
            .into_iter()
            .map(|object| match object {
                Object::Database { database } => Ok(database),
                response => Err(Error::UnexpectedResponse { response }),
            })
            .collect();

        Ok(ListResponse {
            results: databases?,
            has_more: self.has_more,
            next_cursor: self.next_cursor,
        })
    }

    pub(crate) fn expect_pages(self) -> Result<ListResponse<Page>, super::Error> {
        let items: Result<Vec<_>, _> = self
            .results
            .into_iter()
            .map(|object| match object {
                Object::Page { page } => Ok(page),
                response => Err(Error::UnexpectedResponse { response }),
            })
            .collect();

        Ok(ListResponse {
            results: items?,
            has_more: self.has_more,
            next_cursor: self.next_cursor,
        })
    }

    pub(crate) fn expect_blocks(self) -> Result<ListResponse<Block>, super::Error> {
        let items: Result<Vec<_>, _> = self
            .results
            .into_iter()
            .map(|object| match object {
                Object::Block { block } => Ok(block),
                response => Err(Error::UnexpectedResponse { response }),
            })
            .collect();

        Ok(ListResponse {
            results: items?,
            has_more: self.has_more,
            next_cursor: self.next_cursor,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum Parent {
    #[serde(rename = "database_id")]
    Database {
        database_id: DatabaseId,
    },
    #[serde(rename = "page_id")]
    Page {
        page_id: PageId,
    },
    Workspace,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Properties {
    #[serde(flatten)]
    pub properties: HashMap<String, PropertyValue>,
}

impl Properties {
    pub fn title(&self) -> Option<String> {
        self.properties.values().find_map(|p| match p {
            PropertyValue::Title { title, .. } => {
                Some(title.iter().map(|t| t.plain_text()).collect())
            }
            _ => None,
        })
    }
}

#[derive(Serialize, Debug, Eq, PartialEq)]
pub struct PageCreateRequest {
    pub parent: Parent,
    pub properties: Properties,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<CreateBlock>>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Page {
    pub id: PageId,
    /// Date and time when this page was created.
    pub created_time: DateTime<Utc>,
    /// Date and time when this page was updated.
    pub last_edited_time: DateTime<Utc>,
    /// The archived status of the page.
    pub archived: bool,
    pub properties: Properties,
    pub icon: Option<IconObject>,
    pub parent: Parent,
}

#[allow(dead_code)]
impl Page {
    pub fn title(&self) -> Option<String> {
        self.properties.title()
    }
}

impl AsIdentifier<PageId> for Page {
    fn as_id(&self) -> &PageId {
        &self.id
    }
}

#[derive(Eq, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "object")]
#[serde(rename_all = "snake_case")]
pub enum Object {
    Block {
        #[serde(flatten)]
        block: Block,
    },
    Database {
        #[serde(flatten)]
        database: Database,
    },
    Page {
        #[serde(flatten)]
        page: Page,
    },
    List {
        #[serde(flatten)]
        list: ListResponse<Object>,
    },
    User {
        #[serde(flatten)]
        user: User,
    },
    Error {
        #[serde(flatten)]
        error: ErrorResponse,
    },
}

impl Object {
    pub fn is_database(&self) -> bool {
        matches!(self, Object::Database { .. })
    }
}

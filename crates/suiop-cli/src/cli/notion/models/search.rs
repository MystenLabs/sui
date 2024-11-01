// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::super::ids::{PageId, UserId};
use super::paging::{Pageable, Paging, PagingCursor};
use super::Number;
use chrono::{DateTime, Utc};
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

#[derive(Serialize, Debug, Eq, PartialEq, Hash, Copy, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Serialize, Debug, Eq, PartialEq, Hash, Copy, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum SortTimestamp {
    LastEditedTime,
}

#[derive(Serialize, Debug, Eq, PartialEq, Hash, Copy, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum FilterValue {
    Page,
    Database,
}

#[derive(Serialize, Debug, Eq, PartialEq, Hash, Copy, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum FilterProperty {
    Object,
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
pub struct Sort {
    /// The name of the timestamp to sort against.
    timestamp: SortTimestamp,
    direction: SortDirection,
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
pub struct Filter {
    property: FilterProperty,
    value: FilterValue,
}

#[derive(Serialize, Debug, Eq, PartialEq, Default)]
pub struct SearchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<Sort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<Filter>,
    #[serde(flatten)]
    paging: Option<Paging>,
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(unused)]
pub enum TextCondition {
    Equals(String),
    DoesNotEqual(String),
    Contains(String),
    DoesNotContain(String),
    StartsWith(String),
    EndsWith(String),
    #[serde(serialize_with = "serialize_to_true")]
    IsEmpty,
    #[serde(serialize_with = "serialize_to_true")]
    IsNotEmpty,
}

fn serialize_to_true<S>(serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_bool(true)
}

fn serialize_to_empty_object<S>(serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // Todo: there has to be a better way?
    serializer.serialize_map(Some(0))?.end()
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(unused)]
pub enum NumberCondition {
    Equals(Number),
    DoesNotEqual(Number),
    GreaterThan(Number),
    LessThan(Number),
    GreaterThanOrEqualTo(Number),
    LessThanOrEqualTo(Number),
    #[serde(serialize_with = "serialize_to_true")]
    IsEmpty,
    #[serde(serialize_with = "serialize_to_true")]
    IsNotEmpty,
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(unused)]

pub enum CheckboxCondition {
    Equals(bool),
    DoesNotEqual(bool),
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(unused)]
pub enum SelectCondition {
    /// Only return pages where the page property value matches the provided value exactly.
    Equals(String),
    /// Only return pages where the page property value does not match the provided value exactly.
    DoesNotEqual(String),
    /// Only return pages where the page property value is empty.
    #[serde(serialize_with = "serialize_to_true")]
    IsEmpty,
    /// Only return pages where the page property value is present.
    #[serde(serialize_with = "serialize_to_true")]
    IsNotEmpty,
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum MultiSelectCondition {
    /// Only return pages where the page property value contains the provided value.
    Contains(String),
    /// Only return pages where the page property value does not contain the provided value.
    DoesNotContain(String),
    /// Only return pages where the page property value is empty.
    #[serde(serialize_with = "serialize_to_true")]
    IsEmpty,
    /// Only return pages where the page property value is present.
    #[serde(serialize_with = "serialize_to_true")]
    IsNotEmpty,
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum DateCondition {
    /// Only return pages where the page property value matches the provided date exactly.
    /// Note that the comparison is done against the date.
    /// Any time information sent will be ignored.
    Equals(DateTime<Utc>),
    /// Only return pages where the page property value is before the provided date.
    /// Note that the comparison is done against the date.
    /// Any time information sent will be ignored.
    Before(DateTime<Utc>),
    /// Only return pages where the page property value is after the provided date.
    /// Note that the comparison is done against the date.
    /// Any time information sent will be ignored.
    After(DateTime<Utc>),
    /// Only return pages where the page property value is on or before the provided date.
    /// Note that the comparison is done against the date.
    /// Any time information sent will be ignored.
    OnOrBefore(DateTime<Utc>),
    /// Only return pages where the page property value is on or after the provided date.
    /// Note that the comparison is done against the date.
    /// Any time information sent will be ignored.
    OnOrAfter(DateTime<Utc>),
    /// Only return pages where the page property value is empty.
    #[serde(serialize_with = "serialize_to_true")]
    IsEmpty,
    /// Only return pages where the page property value is present.
    #[serde(serialize_with = "serialize_to_true")]
    IsNotEmpty,
    /// Only return pages where the page property value is within the past week.
    #[serde(serialize_with = "serialize_to_empty_object")]
    PastWeek,
    /// Only return pages where the page property value is within the past month.
    #[serde(serialize_with = "serialize_to_empty_object")]
    PastMonth,
    /// Only return pages where the page property value is within the past year.
    #[serde(serialize_with = "serialize_to_empty_object")]
    PastYear,
    /// Only return pages where the page property value is within the next week.
    #[serde(serialize_with = "serialize_to_empty_object")]
    NextWeek,
    /// Only return pages where the page property value is within the next month.
    #[serde(serialize_with = "serialize_to_empty_object")]
    NextMonth,
    /// Only return pages where the page property value is within the next year.
    #[serde(serialize_with = "serialize_to_empty_object")]
    NextYear,
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum PeopleCondition {
    Contains(UserId),
    /// Only return pages where the page property value does not contain the provided value.
    DoesNotContain(UserId),
    /// Only return pages where the page property value is empty.
    #[serde(serialize_with = "serialize_to_true")]
    IsEmpty,
    /// Only return pages where the page property value is present.
    #[serde(serialize_with = "serialize_to_true")]
    IsNotEmpty,
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum FilesCondition {
    /// Only return pages where the page property value is empty.
    #[serde(serialize_with = "serialize_to_true")]
    IsEmpty,
    /// Only return pages where the page property value is present.
    #[serde(serialize_with = "serialize_to_true")]
    IsNotEmpty,
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum RelationCondition {
    /// Only return pages where the page property value contains the provided value.
    Contains(PageId),
    /// Only return pages where the page property value does not contain the provided value.
    DoesNotContain(PageId),
    /// Only return pages where the page property value is empty.
    #[serde(serialize_with = "serialize_to_true")]
    IsEmpty,
    /// Only return pages where the page property value is present.
    #[serde(serialize_with = "serialize_to_true")]
    IsNotEmpty,
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(unused)]
pub enum FormulaCondition {
    /// Only return pages where the result type of the page property formula is "text"
    /// and the provided text filter condition matches the formula's value.
    Text(TextCondition),
    /// Only return pages where the result type of the page property formula is "number"
    /// and the provided number filter condition matches the formula's value.
    Number(NumberCondition),
    /// Only return pages where the result type of the page property formula is "checkbox"
    /// and the provided checkbox filter condition matches the formula's value.
    Checkbox(CheckboxCondition),
    /// Only return pages where the result type of the page property formula is "date"
    /// and the provided date filter condition matches the formula's value.
    Date(DateCondition),
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(unused)]
pub enum PropertyCondition {
    RichText(TextCondition),
    Number(NumberCondition),
    Checkbox(CheckboxCondition),
    Select(SelectCondition),
    MultiSelect(MultiSelectCondition),
    Date(DateCondition),
    People(PeopleCondition),
    Files(FilesCondition),
    Relation(RelationCondition),
    Formula(FormulaCondition),
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(untagged)]
#[allow(dead_code)]
pub enum FilterCondition {
    Property {
        property: String,
        #[serde(flatten)]
        condition: PropertyCondition,
    },
    /// Returns pages when **all** of the filters inside the provided vector match.
    And { and: Vec<FilterCondition> },
    /// Returns pages when **any** of the filters inside the provided vector match.
    Or { or: Vec<FilterCondition> },
}

#[derive(Serialize, Debug, Eq, PartialEq, Hash, Copy, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum DatabaseSortTimestamp {
    CreatedTime,
    LastEditedTime,
}

#[derive(Serialize, Debug, Eq, PartialEq, Clone)]
pub struct DatabaseSort {
    // Todo: Should property and timestamp be mutually exclusive? (i.e a flattened enum?)
    //  the documentation is not clear:
    //  https://developers.notion.com/reference/post-database-query#post-database-query-sort
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property: Option<String>,
    /// The name of the timestamp to sort against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DatabaseSortTimestamp>,
    pub direction: SortDirection,
}

#[derive(Serialize, Debug, Eq, PartialEq, Default, Clone)]
pub struct DatabaseQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sorts: Option<Vec<DatabaseSort>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<FilterCondition>,
    #[serde(flatten)]
    pub paging: Option<Paging>,
}

impl Pageable for DatabaseQuery {
    fn start_from(self, starting_point: Option<PagingCursor>) -> Self {
        DatabaseQuery {
            paging: Some(Paging {
                start_cursor: starting_point,
                page_size: self.paging.and_then(|p| p.page_size),
            }),
            ..self
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum NotionSearch {
    /// When supplied, limits which pages are returned by comparing the query to the page title.
    Query(String),
    /// When supplied, sorts the results based on the provided criteria.
    ///
    /// Limitation: Currently only a single sort is allowed and is limited to `last_edited_time`
    Sort {
        timestamp: SortTimestamp,
        direction: SortDirection,
    },
    /// When supplied, filters the results based on the provided criteria.
    ///
    /// Limitation: Currently the only filter allowed is `object` which will filter by type of object (either page or database)
    Filter {
        /// The name of the property to filter by.
        /// Currently the only property you can filter by is the `object` type.
        property: FilterProperty,
        /// The value of the property to filter the results by.
        /// Possible values for object type include `page` or `database`.
        value: FilterValue,
    },
}

#[allow(dead_code)]
impl NotionSearch {
    pub fn filter_by_databases() -> Self {
        Self::Filter {
            property: FilterProperty::Object,
            value: FilterValue::Database,
        }
    }
}

impl From<NotionSearch> for SearchRequest {
    fn from(search: NotionSearch) -> Self {
        match search {
            NotionSearch::Query(query) => SearchRequest {
                query: Some(query),
                ..Default::default()
            },
            NotionSearch::Sort {
                direction,
                timestamp,
            } => SearchRequest {
                sort: Some(Sort {
                    timestamp,
                    direction,
                }),
                ..Default::default()
            },
            NotionSearch::Filter { property, value } => SearchRequest {
                filter: Some(Filter { property, value }),
                ..Default::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    mod text_filters {
        use crate::cli::notion::models::search::PropertyCondition::{
            Checkbox, Number, RichText, Select,
        };
        use crate::cli::notion::models::search::{
            CheckboxCondition, FilterCondition, NumberCondition, SelectCondition, TextCondition,
        };
        use serde_json::json;

        #[test]
        fn text_property_equals() -> Result<(), Box<dyn std::error::Error>> {
            let json = serde_json::to_value(&FilterCondition::Property {
                property: "Name".to_string(),
                condition: RichText(TextCondition::Equals("Test".to_string())),
            })?;
            assert_eq!(
                json,
                json!({"property":"Name","rich_text":{"equals":"Test"}})
            );

            Ok(())
        }

        #[test]
        fn text_property_contains() -> Result<(), Box<dyn std::error::Error>> {
            let json = serde_json::to_value(&FilterCondition::Property {
                property: "Name".to_string(),
                condition: RichText(TextCondition::Contains("Test".to_string())),
            })?;
            assert_eq!(
                dbg!(json),
                json!({"property":"Name","rich_text":{"contains":"Test"}})
            );

            Ok(())
        }

        #[test]
        fn text_property_is_empty() -> Result<(), Box<dyn std::error::Error>> {
            let json = serde_json::to_value(&FilterCondition::Property {
                property: "Name".to_string(),
                condition: RichText(TextCondition::IsEmpty),
            })?;
            assert_eq!(
                dbg!(json),
                json!({"property":"Name","rich_text":{"is_empty":true}})
            );

            Ok(())
        }

        #[test]
        fn text_property_is_not_empty() -> Result<(), Box<dyn std::error::Error>> {
            let json = serde_json::to_value(&FilterCondition::Property {
                property: "Name".to_string(),
                condition: RichText(TextCondition::IsNotEmpty),
            })?;
            assert_eq!(
                dbg!(json),
                json!({"property":"Name","rich_text":{"is_not_empty":true}})
            );

            Ok(())
        }

        #[test]
        fn compound_query_and() -> Result<(), Box<dyn std::error::Error>> {
            let json = serde_json::to_value(&FilterCondition::And {
                and: vec![
                    FilterCondition::Property {
                        property: "Seen".to_string(),
                        condition: Checkbox(CheckboxCondition::Equals(false)),
                    },
                    FilterCondition::Property {
                        property: "Yearly visitor count".to_string(),
                        condition: Number(NumberCondition::GreaterThan(serde_json::Number::from(
                            1000000,
                        ))),
                    },
                ],
            })?;
            assert_eq!(
                dbg!(json),
                json!({"and":[
                    {"property":"Seen","checkbox":{"equals":false}},
                    {"property":"Yearly visitor count","number":{"greater_than":1000000}}
                ]})
            );

            Ok(())
        }

        #[test]
        fn compound_query_or() -> Result<(), Box<dyn std::error::Error>> {
            let json = serde_json::to_value(&FilterCondition::Or {
                or: vec![
                    FilterCondition::Property {
                        property: "Description".to_string(),
                        condition: RichText(TextCondition::Contains("fish".to_string())),
                    },
                    FilterCondition::And {
                        and: vec![
                            FilterCondition::Property {
                                property: "Food group".to_string(),
                                condition: Select(SelectCondition::Equals(
                                    "🥦Vegetable".to_string(),
                                )),
                            },
                            FilterCondition::Property {
                                property: "Is protein rich?".to_string(),
                                condition: Checkbox(CheckboxCondition::Equals(true)),
                            },
                        ],
                    },
                ],
            })?;
            assert_eq!(
                dbg!(json),
                json!({"or":[
                    {"property":"Description","rich_text":{"contains":"fish"}},
                    {"and":[
                        {"property":"Food group","select":{"equals":"🥦Vegetable"}},
                        {"property":"Is protein rich?","checkbox":{"equals":true}}
                    ]}
                ]})
            );

            Ok(())
        }
    }
}

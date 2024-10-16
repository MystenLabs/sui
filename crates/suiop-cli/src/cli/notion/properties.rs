// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::models::text::RichText;
use crate::models::users::User;

use crate::ids::{DatabaseId, PageId, PropertyId};
use crate::models::{DateTime, Number, Utc};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

pub mod formulas;

#[cfg(test)]
mod tests;

/// How the number is displayed in Notion.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Copy, Clone, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NumberFormat {
    Number,
    NumberWithCommas,
    Percent,
    Dollar,
    Euro,
    Pound,
    Yen,
    Ruble,
    Rupee,
    Won,
    Yuan,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Hash, Clone)]
pub struct NumberDetails {
    pub format: NumberFormat,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
#[serde(transparent)]
pub struct SelectOptionId(String);

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    Default,
    Gray,
    Brown,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Pink,
    Red,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct SelectOption {
    pub name: String,
    pub id: SelectOptionId,
    pub color: Color,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Select {
    /// Sorted list of options available for this property.
    pub options: Vec<SelectOption>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct StatusGroupOption {
    pub name: String,
    pub id: SelectOptionId,
    pub color: Color,
    pub option_ids: Vec<SelectOptionId>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Status {
    /// Sorted list of options available for this property.
    pub options: Vec<SelectOption>,
    /// Sorted list of groups available for this property.
    pub groups: Vec<StatusGroupOption>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Formula {
    /// Formula to evaluate for this property
    pub expression: String,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Relation {
    /// The database this relation refers to.
    /// New linked pages must belong to this database in order to be valid.
    pub database_id: DatabaseId,
    /// By default, relations are formed as two synced properties across databases:
    ///     if you make a change to one property, it updates the synced property at the same time.
    /// `synced_property_name` refers to the name of the property in the related database.
    pub synced_property_name: Option<String>,
    /// By default, relations are formed as two synced properties across databases:
    ///     if you make a change to one property, it updates the synced property at the same time.
    /// `synced_property_id` refers to the id of the property in the related database.
    /// This is usually a short string of random letters and symbols.
    pub synced_property_id: Option<PropertyId>,
}

/// The function used to roll up the values of the relation property.
/// <https://developers.notion.com/reference/page-property-values#rollup>
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
pub enum RollupFunction {
    Average,
    Checked,
    Count,
    CountPerGroup,
    CountValues,
    DateRange,
    EarliestDate,
    Empty,
    LatestDate,
    Max,
    Median,
    Min,
    NotEmpty,
    PercentChecked,
    PercentEmpty,
    PercentNotEmpty,
    PercentPerGroup,
    PercentUnchecked,
    Range,
    ShowOriginal,
    ShowUnique,
    Sum,
    Unchecked,
    Unique,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Rollup {
    /// The name of the relation property this property is responsible for rolling up.
    pub relation_property_name: String,
    /// The id of the relation property this property is responsible for rolling up.
    pub relation_property_id: PropertyId,
    /// The name of the property of the pages in the related database
    /// that is used as an input to `function`.
    pub rollup_property_name: String,
    /// The id of the property of the pages in the related database
    /// that is used as an input to `function`.
    pub rollup_property_id: String,
    /// The function that is evaluated for every page in the relation of the rollup.
    pub function: RollupFunction,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum PropertyConfiguration {
    /// Represents the special Title property required on every database.
    /// See <https://developers.notion.com/reference/database#title-configuration>
    Title {
        id: PropertyId,
    },
    /// Represents a Text property
    /// <https://developers.notion.com/reference/database#text-configuration>
    #[serde(rename = "rich_text")]
    Text {
        id: PropertyId,
    },
    /// Represents a Number Property
    /// See <https://developers.notion.com/reference/database#number-configuration>
    Number {
        id: PropertyId,
        /// How the number is displayed in Notion.
        number: NumberDetails,
    },
    /// Represents a Select Property
    /// See <https://developers.notion.com/reference/database#select-configuration>
    Select {
        id: PropertyId,
        select: Select,
    },
    /// Represents a Status property
    Status {
        id: PropertyId,
        status: Status,
    },
    /// Represents a Multi-select Property
    /// See <https://developers.notion.com/reference/database#multi-select-configuration>
    MultiSelect {
        id: PropertyId,
        multi_select: Select,
    },
    /// Represents a Date Property
    /// See <https://developers.notion.com/reference/database#date-configuration>
    Date {
        id: PropertyId,
    },
    /// Represents a People Property
    /// See <https://developers.notion.com/reference/database#people-configuration>
    People {
        id: PropertyId,
    },
    /// Represents a File Property
    /// See <https://developers.notion.com/reference/database#file-configuration>
    // Todo: File a bug with notion
    //       Documentation issue: docs claim type name is `file` but it is in fact `files`
    Files {
        id: PropertyId,
    },
    /// Represents a Checkbox Property
    /// See <https://developers.notion.com/reference/database#checkbox-configuration>
    Checkbox {
        id: PropertyId,
    },
    /// Represents a URL Property
    /// See <https://developers.notion.com/reference/database#url-configuration>
    Url {
        id: PropertyId,
    },
    /// Represents a Email Property
    /// See <https://developers.notion.com/reference/database#email-configuration>
    Email {
        id: PropertyId,
    },
    /// Represents a Phone number Property
    /// See <https://developers.notion.com/reference/database#phone-number-configuration>
    PhoneNumber {
        id: PropertyId,
    },
    /// See <https://developers.notion.com/reference/database#formula-configuration>
    Formula {
        id: PropertyId,
        formula: Formula,
    },
    /// See <https://developers.notion.com/reference/database#relation-configuration>
    Relation {
        id: PropertyId,
        relation: Relation,
    },
    /// See <https://developers.notion.com/reference/database#rollup-configuration>
    Rollup {
        id: PropertyId,
        rollup: Rollup,
    },
    /// See <https://developers.notion.com/reference/database#created-time-configuration>
    CreatedTime {
        id: PropertyId,
    },
    /// See <https://developers.notion.com/reference/database#created-by-configuration>
    CreatedBy {
        id: PropertyId,
    },
    /// See <https://developers.notion.com/reference/database#last-edited-time-configuration>
    LastEditedTime {
        id: PropertyId,
    },
    /// See <https://developers.notion.com/reference/database#last-edited-by-configuration>
    LastEditBy {
        id: PropertyId,
    },
    UniqueId {
        id: PropertyId,
    },
    Button {
        id: PropertyId,
    },
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct SelectedValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<SelectOptionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub color: Color,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(untagged)]
pub enum DateOrDateTime {
    Date(NaiveDate),
    DateTime(DateTime<Utc>),
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct DateValue {
    pub start: DateOrDateTime,
    pub end: Option<DateOrDateTime>,
    pub time_zone: Option<String>,
}

/// Formula property value objects represent the result of evaluating a formula
/// described in the database's properties.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum FormulaResultValue {
    String { string: Option<String> },
    Number { number: Option<Number> },
    Boolean { boolean: Option<bool> },
    Date { date: Option<DateValue> },
}

/// Relation property value objects contain an array of page references within the relation property.
/// A page reference is an object with an id property,
/// with a string value (UUIDv4) corresponding to a page ID in another database.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct RelationValue {
    pub id: PageId,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RollupValue {
    Number { number: Option<Number> },
    Date { date: Option<DateTime<Utc>> },
    Array { array: Vec<RollupPropertyValue> },
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct FileReference {
    pub name: String,
    pub url: String,
    pub mime_type: String,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum PropertyValue {
    // <https://developers.notion.com/reference/property-object#title-configuration>
    Title {
        id: PropertyId,
        title: Vec<RichText>,
    },
    /// <https://developers.notion.com/reference/property-object#text-configuration>
    #[serde(rename = "rich_text")]
    Text {
        id: PropertyId,
        rich_text: Vec<RichText>,
    },
    /// <https://developers.notion.com/reference/property-object#number-configuration>
    Number {
        id: PropertyId,
        number: Option<Number>,
    },
    /// <https://developers.notion.com/reference/property-object#select-configuration>
    Select {
        id: PropertyId,
        select: Option<SelectedValue>,
    },
    /// <https://developers.notion.com/reference/property-object#status-configuration>
    Status {
        id: PropertyId,
        status: Option<SelectedValue>,
    },
    /// <https://developers.notion.com/reference/property-object#multi-select-configuration>
    MultiSelect {
        id: PropertyId,
        multi_select: Option<Vec<SelectedValue>>,
    },
    /// <https://developers.notion.com/reference/property-object#date-configuration>
    Date {
        id: PropertyId,
        date: Option<DateValue>,
    },
    /// <https://developers.notion.com/reference/property-object#formula-configuration>
    Formula {
        id: PropertyId,
        formula: FormulaResultValue,
    },
    /// <https://developers.notion.com/reference/property-object#relation-configuration>
    /// It is actually an array of relations
    Relation {
        id: PropertyId,
        relation: Option<Vec<RelationValue>>,
    },
    /// <https://developers.notion.com/reference/property-object#rollup-configuration>
    Rollup {
        id: PropertyId,
        rollup: Option<RollupValue>,
    },
    /// <https://developers.notion.com/reference/property-object#people-configuration>
    People {
        id: PropertyId,
        people: Vec<User>,
    },
    /// <https://developers.notion.com/reference/property-object#files-configuration>
    Files {
        id: PropertyId,
        files: Option<Vec<FileReference>>,
    },
    /// <https://developers.notion.com/reference/property-object#checkbox-configuration>
    Checkbox {
        id: PropertyId,
        checkbox: bool,
    },
    /// <https://developers.notion.com/reference/property-object#url-configuration>
    Url {
        id: PropertyId,
        url: Option<String>,
    },
    /// <https://developers.notion.com/reference/property-object#email-configuration>
    Email {
        id: PropertyId,
        email: Option<String>,
    },
    /// <https://developers.notion.com/reference/property-object#phone-number-configuration>
    PhoneNumber {
        id: PropertyId,
        phone_number: String,
    },
    /// <https://developers.notion.com/reference/property-object#created-time-configuration>
    CreatedTime {
        id: PropertyId,
        created_time: DateTime<Utc>,
    },
    /// <https://developers.notion.com/reference/property-object#created-by-configuration>
    CreatedBy {
        id: PropertyId,
        created_by: User,
    },
    /// <https://developers.notion.com/reference/property-object#last-edited-time-configuration>
    LastEditedTime {
        id: PropertyId,
        last_edited_time: DateTime<Utc>,
    },
    /// <https://developers.notion.com/reference/property-object#last-edited-by-configuration>
    LastEditedBy {
        id: PropertyId,
        last_edited_by: User,
    },
    UniqueId {
        id: PropertyId,
        unique_id: UniqueidValue,
    },
    Button {
        id: PropertyId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UniqueidValue {
    pub prefix: Option<String>,
    pub number: u32,
}

/// <https://developers.notion.com/reference/page#rollup-property-value-element>
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum RollupPropertyValue {
    /// <https://developers.notion.com/reference/page#rich-text-property-values>
    #[serde(rename = "rich_text")]
    Text {
        rich_text: Vec<RichText>,
    },
    /// <https://developers.notion.com/reference/page#number-property-values>
    Number {
        number: Option<Number>,
    },
    /// <https://developers.notion.com/reference/page#select-property-values>
    Select {
        select: Option<SelectedValue>,
    },
    Status {
        status: Option<SelectedValue>,
    },
    MultiSelect {
        multi_select: Option<Vec<SelectedValue>>,
    },
    Date {
        date: Option<DateValue>,
    },
    /// <https://developers.notion.com/reference/page#formula-property-values>
    Formula {
        formula: FormulaResultValue,
    },
    /// <https://developers.notion.com/reference/page#relation-property-values>
    /// It is actually an array of relations
    Relation {
        relation: Option<Vec<RelationValue>>,
    },
    /// <https://developers.notion.com/reference/page#rollup-property-values>
    Rollup {
        rollup: Option<RollupValue>,
    },
    People {
        people: Vec<User>,
    },
    Files {
        files: Option<Vec<FileReference>>,
    },
    Checkbox {
        checkbox: bool,
    },
    Url {
        url: Option<String>,
    },
    Email {
        email: Option<String>,
    },
    PhoneNumber {
        phone_number: String,
    },
    CreatedTime {
        created_time: DateTime<Utc>,
    },
    CreatedBy {
        created_by: User,
    },
    LastEditedTime {
        last_edited_time: DateTime<Utc>,
    },
    LastEditedBy {
        last_edited_by: User,
    },
}

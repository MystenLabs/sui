// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::text::{RichText, TextColor};
use super::users::UserCommon;
use crate::cli::notion::ids::{AsIdentifier, BlockId, DatabaseId, PageId};

mod tests;

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct BlockCommon {
    pub id: BlockId,
    pub created_time: DateTime<Utc>,
    pub last_edited_time: DateTime<Utc>,
    pub has_children: bool,
    pub created_by: UserCommon,
    pub last_edited_by: UserCommon,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct TextAndChildren {
    pub rich_text: Vec<RichText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Block>>,
    pub color: TextColor,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Text {
    pub rich_text: Vec<RichText>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct InternalFileObject {
    url: String,
    expiry_time: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ExternalFileObject {
    url: String,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum FileOrEmojiObject {
    Emoji { emoji: String },
    File { file: InternalFileObject },
    External { external: ExternalFileObject },
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum FileObject {
    File { file: InternalFileObject },
    External { external: ExternalFileObject },
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Callout {
    pub rich_text: Vec<RichText>,
    pub icon: FileOrEmojiObject,
    pub color: TextColor,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ToDoFields {
    pub rich_text: Vec<RichText>,
    pub checked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Block>>,
    pub color: TextColor,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ChildPageFields {
    pub title: String,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ChildDatabaseFields {
    pub title: String,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct EmbedFields {
    pub url: String,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct BookmarkFields {
    pub url: String,
    pub caption: Vec<RichText>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum CodeLanguage {
    Abap,
    Arduino,
    Bash,
    Basic,
    C,
    Clojure,
    Coffeescript,
    #[serde(rename = "c++")]
    CPlusPlus,
    #[serde(rename = "c#")]
    CSharp,
    Css,
    Dart,
    Diff,
    Docker,
    Elixir,
    Elm,
    Erlang,
    Flow,
    Fortran,
    #[serde(rename = "f#")]
    FSharp,
    Gherkin,
    Glsl,
    Go,
    Graphql,
    Groovy,
    Haskell,
    Html,
    Java,
    Javascript,
    Json,
    Julia,
    Kotlin,
    Latex,
    Less,
    Lisp,
    Livescript,
    Lua,
    Makefile,
    Markdown,
    Markup,
    Matlab,
    Mermaid,
    Nix,
    #[serde(rename = "objective-c")]
    ObjectiveC,
    Ocaml,
    Pascal,
    Perl,
    Php,
    #[serde(rename = "plain text")]
    PlainText,
    Powershell,
    Prolog,
    Protobuf,
    Python,
    R,
    Reason,
    Ruby,
    Rust,
    Sass,
    Scala,
    Scheme,
    Scss,
    Shell,
    Sql,
    Swift,
    Typescript,
    #[serde(rename = "vb.net")]
    VbNet,
    Verilog,
    Vhdl,
    #[serde(rename = "visual basic")]
    VisualBasic,
    Webassembly,
    Xml,
    Yaml,
    #[serde(rename = "java/c/c++/c#")]
    JavaCAndCPlusPlusAndCSharp,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct CodeFields {
    pub rich_text: Vec<RichText>,
    pub caption: Vec<RichText>,
    pub language: CodeLanguage,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct Equation {
    pub expression: String,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct TableOfContents {
    pub color: TextColor,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ColumnListFields {
    pub children: Vec<Block>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ColumnFields {
    pub children: Vec<Block>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct LinkPreviewFields {
    pub url: String,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct TemplateFields {
    pub rich_text: Vec<RichText>,
    pub children: Vec<Block>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum LinkToPageFields {
    PageId { page_id: PageId },
    DatabaseId { database_id: DatabaseId },
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct SyncedFromObject {
    pub block_id: BlockId,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct SyncedBlockFields {
    pub synced_from: Option<SyncedFromObject>,
    pub children: Vec<Block>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct TableFields {
    pub table_width: u64,
    pub has_column_header: bool,
    pub has_row_header: bool,
    pub children: Vec<Block>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct TableRowFields {
    pub cells: Vec<RichText>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum Block {
    Paragraph {
        #[serde(flatten)]
        common: BlockCommon,
        paragraph: TextAndChildren,
    },
    #[serde(rename = "heading_1")]
    Heading1 {
        #[serde(flatten)]
        common: BlockCommon,
        heading_1: Text,
    },
    #[serde(rename = "heading_2")]
    Heading2 {
        #[serde(flatten)]
        common: BlockCommon,
        heading_2: Text,
    },
    #[serde(rename = "heading_3")]
    Heading3 {
        #[serde(flatten)]
        common: BlockCommon,
        heading_3: Text,
    },
    Callout {
        #[serde(flatten)]
        common: BlockCommon,
        callout: Callout,
    },
    Quote {
        #[serde(flatten)]
        common: BlockCommon,
        quote: TextAndChildren,
    },
    BulletedListItem {
        #[serde(flatten)]
        common: BlockCommon,
        bulleted_list_item: TextAndChildren,
    },
    NumberedListItem {
        #[serde(flatten)]
        common: BlockCommon,
        numbered_list_item: TextAndChildren,
    },
    ToDo {
        #[serde(flatten)]
        common: BlockCommon,
        to_do: ToDoFields,
    },
    Toggle {
        #[serde(flatten)]
        common: BlockCommon,
        toggle: TextAndChildren,
    },
    Code {
        #[serde(flatten)]
        common: BlockCommon,
        code: CodeFields,
    },
    ChildPage {
        #[serde(flatten)]
        common: BlockCommon,
        child_page: ChildPageFields,
    },
    ChildDatabase {
        #[serde(flatten)]
        common: BlockCommon,
        child_page: ChildDatabaseFields,
    },
    Embed {
        #[serde(flatten)]
        common: BlockCommon,
        embed: EmbedFields,
    },
    Image {
        #[serde(flatten)]
        common: BlockCommon,
        image: FileObject,
    },
    Video {
        #[serde(flatten)]
        common: BlockCommon,
        video: FileObject,
    },
    File {
        #[serde(flatten)]
        common: BlockCommon,
        file: FileObject,
        caption: Text,
    },
    Pdf {
        #[serde(flatten)]
        common: BlockCommon,
        pdf: FileObject,
    },
    Bookmark {
        #[serde(flatten)]
        common: BlockCommon,
        bookmark: BookmarkFields,
    },
    Equation {
        #[serde(flatten)]
        common: BlockCommon,
        equation: Equation,
    },
    Divider {
        #[serde(flatten)]
        common: BlockCommon,
    },
    TableOfContents {
        #[serde(flatten)]
        common: BlockCommon,
        table_of_contents: TableOfContents,
    },
    Breadcrumb {
        #[serde(flatten)]
        common: BlockCommon,
    },
    ColumnList {
        #[serde(flatten)]
        common: BlockCommon,
        column_list: ColumnListFields,
    },
    Column {
        #[serde(flatten)]
        common: BlockCommon,
        column: ColumnFields,
    },
    LinkPreview {
        #[serde(flatten)]
        common: BlockCommon,
        link_preview: LinkPreviewFields,
    },
    Template {
        #[serde(flatten)]
        common: BlockCommon,
        template: TemplateFields,
    },
    LinkToPage {
        #[serde(flatten)]
        common: BlockCommon,
        link_to_page: LinkToPageFields,
    },
    Table {
        #[serde(flatten)]
        common: BlockCommon,
        table: TableFields,
    },
    SyncedBlock {
        #[serde(flatten)]
        common: BlockCommon,
        synced_block: SyncedBlockFields,
    },
    TableRow {
        #[serde(flatten)]
        common: BlockCommon,
        table_row: TableRowFields,
    },
    Unsupported {
        #[serde(flatten)]
        common: BlockCommon,
    },
    #[serde(other)]
    Unknown,
}

impl AsIdentifier<BlockId> for Block {
    fn as_id(&self) -> &BlockId {
        use Block::*;
        match self {
            Paragraph { common, .. }
            | Heading1 { common, .. }
            | Heading2 { common, .. }
            | Heading3 { common, .. }
            | Callout { common, .. }
            | Quote { common, .. }
            | BulletedListItem { common, .. }
            | NumberedListItem { common, .. }
            | ToDo { common, .. }
            | Toggle { common, .. }
            | Code { common, .. }
            | ChildPage { common, .. }
            | ChildDatabase { common, .. }
            | Embed { common, .. }
            | Image { common, .. }
            | Video { common, .. }
            | File { common, .. }
            | Pdf { common, .. }
            | Bookmark { common, .. }
            | Equation { common, .. }
            | Divider { common, .. }
            | TableOfContents { common, .. }
            | Breadcrumb { common, .. }
            | ColumnList { common, .. }
            | Column { common, .. }
            | LinkPreview { common, .. }
            | Template { common, .. }
            | LinkToPage { common, .. }
            | SyncedBlock { common, .. }
            | Table { common, .. }
            | TableRow { common, .. }
            | Unsupported { common, .. } => &common.id,
            Unknown => {
                panic!("Trying to reference identifier for unknown block!")
            }
        }
    }
}

impl From<Block> for CreateBlock {
    fn from(val: Block) -> Self {
        match val {
            Block::Paragraph { paragraph, .. } => CreateBlock::Paragraph { paragraph },
            Block::Heading1 { heading_1, .. } => CreateBlock::Heading1 { heading_1 },
            Block::Heading2 { heading_2, .. } => CreateBlock::Heading2 { heading_2 },
            Block::Heading3 { heading_3, .. } => CreateBlock::Heading3 { heading_3 },
            Block::Callout { callout, .. } => CreateBlock::Callout { callout },
            Block::Quote { quote, .. } => CreateBlock::Quote { quote },
            Block::BulletedListItem {
                bulleted_list_item, ..
            } => CreateBlock::BulletedListItem { bulleted_list_item },
            Block::NumberedListItem {
                numbered_list_item, ..
            } => CreateBlock::NumberedListItem { numbered_list_item },
            Block::ToDo { to_do, .. } => CreateBlock::ToDo { to_do },
            Block::Toggle { toggle, .. } => CreateBlock::Toggle { toggle },
            Block::Code { code, .. } => CreateBlock::Code { code },
            Block::ChildPage { child_page, .. } => CreateBlock::ChildPage { child_page },
            Block::ChildDatabase { child_page, .. } => CreateBlock::ChildDatabase { child_page },
            Block::Embed { embed, .. } => CreateBlock::Embed { embed },
            Block::Image { image, .. } => CreateBlock::Image { image },
            Block::Video { video, .. } => CreateBlock::Video { video },
            Block::File { file, caption, .. } => CreateBlock::File { file, caption },
            Block::Pdf { pdf, .. } => CreateBlock::Pdf { pdf },
            Block::Bookmark { bookmark, .. } => CreateBlock::Bookmark { bookmark },
            Block::Equation { equation, .. } => CreateBlock::Equation { equation },
            Block::Divider { .. } => CreateBlock::Divider {},
            Block::TableOfContents {
                table_of_contents, ..
            } => CreateBlock::TableOfContents { table_of_contents },
            Block::Breadcrumb { .. } => CreateBlock::Breadcrumb {},
            Block::ColumnList { column_list, .. } => CreateBlock::ColumnList { column_list },
            Block::Column { column, .. } => CreateBlock::Column { column },

            Block::LinkPreview { link_preview, .. } => CreateBlock::LinkPreview { link_preview },
            Block::Template { template, .. } => CreateBlock::Template { template },
            Block::LinkToPage { link_to_page, .. } => CreateBlock::LinkToPage { link_to_page },
            Block::Table { table, .. } => CreateBlock::Table { table },
            Block::SyncedBlock { synced_block, .. } => CreateBlock::SyncedBlock { synced_block },
            Block::TableRow { table_row, .. } => CreateBlock::TableRow { table_row },
            Block::Unsupported { .. } => CreateBlock::Unsupported,
            Block::Unknown => CreateBlock::Unknown,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum CreateBlock {
    Paragraph {
        paragraph: TextAndChildren,
    },
    #[serde(rename = "heading_1")]
    Heading1 {
        heading_1: Text,
    },
    #[serde(rename = "heading_2")]
    Heading2 {
        heading_2: Text,
    },
    #[serde(rename = "heading_3")]
    Heading3 {
        heading_3: Text,
    },
    Callout {
        callout: Callout,
    },
    Quote {
        quote: TextAndChildren,
    },
    BulletedListItem {
        bulleted_list_item: TextAndChildren,
    },
    NumberedListItem {
        numbered_list_item: TextAndChildren,
    },
    ToDo {
        to_do: ToDoFields,
    },
    Toggle {
        toggle: TextAndChildren,
    },
    Code {
        code: CodeFields,
    },
    ChildPage {
        child_page: ChildPageFields,
    },
    ChildDatabase {
        child_page: ChildDatabaseFields,
    },
    Embed {
        embed: EmbedFields,
    },
    Image {
        image: FileObject,
    },
    Video {
        video: FileObject,
    },
    File {
        file: FileObject,
        caption: Text,
    },
    Pdf {
        pdf: FileObject,
    },
    Bookmark {
        bookmark: BookmarkFields,
    },
    Equation {
        equation: Equation,
    },
    Divider,
    TableOfContents {
        table_of_contents: TableOfContents,
    },
    Breadcrumb,
    ColumnList {
        column_list: ColumnListFields,
    },
    Column {
        column: ColumnFields,
    },
    LinkPreview {
        link_preview: LinkPreviewFields,
    },
    Template {
        template: TemplateFields,
    },
    LinkToPage {
        link_to_page: LinkToPageFields,
    },
    Table {
        table: TableFields,
    },
    SyncedBlock {
        synced_block: SyncedBlockFields,
    },
    TableRow {
        table_row: TableRowFields,
    },
    Unsupported,
    #[serde(other)]
    Unknown,
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::super::ids::UserId;
use super::properties::{DateOrDateTime, DateValue};
use super::text::{Annotations, Link, MentionObject, RichText, RichTextCommon, Text, TextColor};
use super::users::{Person, User, UserCommon};
use super::{ListResponse, Object, Page};
use chrono::{DateTime, NaiveDate};
use std::str::FromStr;

#[test]
fn deserialize_page() {
    let _page: Page = serde_json::from_str(include_str!("tests/page.json")).unwrap();
}

#[test]
fn deserialize_query_result() {
    let _page: ListResponse<Page> =
        serde_json::from_str(include_str!("tests/query_result.json")).unwrap();
}

#[test]
fn deserialize_number_format() {
    let _search_results: ListResponse<Object> =
        serde_json::from_str(include_str!("tests/issue_15.json")).unwrap();
}

#[test]
fn rich_text() {
    let rich_text_text: RichText =
        serde_json::from_str(include_str!("tests/rich_text_text.json")).unwrap();
    assert_eq!(
        rich_text_text,
        RichText::Text {
            rich_text: RichTextCommon {
                plain_text: "Rich".to_string(),
                href: Some("https://github.com/jakeswenson/notion".to_string()),
                annotations: Some(Annotations {
                    bold: Some(true),
                    code: Some(true),
                    color: Some(TextColor::Default),
                    italic: Some(true),
                    strikethrough: Some(true),
                    underline: Some(true),
                }),
            },
            text: Text {
                content: "Rich".to_string(),
                link: Some(Link {
                    url: "https://github.com/jakeswenson/notion".to_string()
                }),
            },
        }
    )
}

#[test]
fn rich_text_mention_user_person() {
    let rich_text_mention_user_person: RichText =
        serde_json::from_str(include_str!("tests/rich_text_mention_user_person.json")).unwrap();
    assert_eq!(
    rich_text_mention_user_person,
    RichText::Mention {
      rich_text: RichTextCommon {
        plain_text: "@John Doe".to_string(),
        href: None,
        annotations: Some(Annotations {
          bold: Some(false),
          code: Some(false),
          color: Some(TextColor::Default),
          italic: Some(false),
          strikethrough: Some(false),
          underline: Some(false),
        }),
      },
      mention: MentionObject::User {
        user: User::Person {
          common: UserCommon {
            id: UserId::from_str("1118608e-35e8-4fa3-aef7-a4ced85ce8e0").unwrap(),
            name: Some("John Doe".to_string()),
            avatar_url: Some(
              "https://secure.notion-static.com/e6a352a8-8381-44d0-a1dc-9ed80e62b53d.jpg"
                .to_string()
            ),
          },
          person: Person {
            email: "john.doe@gmail.com".to_string()
          },
        }
      },
    }
  )
}

#[test]
fn rich_text_mention_date() {
    let rich_text_mention_date: RichText =
        serde_json::from_str(include_str!("tests/rich_text_mention_date.json")).unwrap();
    assert_eq!(
        rich_text_mention_date,
        RichText::Mention {
            rich_text: RichTextCommon {
                plain_text: "2022-04-16 → ".to_string(),
                href: None,
                annotations: Some(Annotations {
                    bold: Some(false),
                    code: Some(false),
                    color: Some(TextColor::Default),
                    italic: Some(false),
                    strikethrough: Some(false),
                    underline: Some(false),
                }),
            },
            mention: MentionObject::Date {
                date: DateValue {
                    start: DateOrDateTime::Date(NaiveDate::from_str("2022-04-16").unwrap()),
                    end: None,
                    time_zone: None,
                }
            },
        }
    )
}

#[test]
fn rich_text_mention_date_with_time() {
    let rich_text_mention_date_with_time: RichText =
        serde_json::from_str(include_str!("tests/rich_text_mention_date_with_time.json")).unwrap();
    assert_eq!(
        rich_text_mention_date_with_time,
        RichText::Mention {
            rich_text: RichTextCommon {
                plain_text: "2022-05-14T09:00:00.000-04:00 → ".to_string(),
                href: None,
                annotations: Some(Annotations {
                    bold: Some(false),
                    code: Some(false),
                    color: Some(TextColor::Default),
                    italic: Some(false),
                    strikethrough: Some(false),
                    underline: Some(false),
                }),
            },
            mention: MentionObject::Date {
                date: DateValue {
                    start: DateOrDateTime::DateTime(
                        DateTime::from_str("2022-05-14T09:00:00.000-04:00").unwrap()
                    ),
                    end: None,
                    time_zone: None,
                }
            },
        }
    )
}

#[test]
fn rich_text_mention_date_with_end() {
    let rich_text_mention_date_with_end: RichText =
        serde_json::from_str(include_str!("tests/rich_text_mention_date_with_end.json")).unwrap();
    assert_eq!(
        rich_text_mention_date_with_end,
        RichText::Mention {
            rich_text: RichTextCommon {
                plain_text: "2022-05-12 → 2022-05-13".to_string(),
                href: None,
                annotations: Some(Annotations {
                    bold: Some(false),
                    code: Some(false),
                    color: Some(TextColor::Default),
                    italic: Some(false),
                    strikethrough: Some(false),
                    underline: Some(false),
                }),
            },
            mention: MentionObject::Date {
                date: DateValue {
                    start: DateOrDateTime::Date(NaiveDate::from_str("2022-05-12").unwrap()),
                    end: Some(DateOrDateTime::Date(
                        NaiveDate::from_str("2022-05-13").unwrap()
                    )),
                    time_zone: None,
                }
            },
        }
    )
}

#[test]
fn rich_text_mention_date_with_end_and_time() {
    let rich_text_mention_date_with_end_and_time: RichText = serde_json::from_str(include_str!(
        "tests/rich_text_mention_date_with_end_and_time.json"
    ))
    .unwrap();
    assert_eq!(
        rich_text_mention_date_with_end_and_time,
        RichText::Mention {
            rich_text: RichTextCommon {
                plain_text: "2022-04-16T12:00:00.000-04:00 → 2022-04-16T12:00:00.000-04:00"
                    .to_string(),
                href: None,
                annotations: Some(Annotations {
                    bold: Some(false),
                    code: Some(false),
                    color: Some(TextColor::Default),
                    italic: Some(false),
                    strikethrough: Some(false),
                    underline: Some(false),
                }),
            },
            mention: MentionObject::Date {
                date: DateValue {
                    start: DateOrDateTime::DateTime(
                        DateTime::from_str("2022-04-16T12:00:00.000-04:00").unwrap()
                    ),
                    end: Some(DateOrDateTime::DateTime(
                        DateTime::from_str("2022-04-16T12:00:00.000-04:00").unwrap()
                    )),
                    time_zone: None,
                }
            },
        }
    )
}

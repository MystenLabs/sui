// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use chrono::DateTime;

    use crate::cli::notion::{
        ids::{BlockId, UserId},
        models::{
            block::{
                Block, BlockCommon, Callout, ExternalFileObject, FileOrEmojiObject,
                InternalFileObject, Text as TextBlockModel,
            },
            text::{Annotations, RichText, RichTextCommon, Text, TextColor},
            users::UserCommon,
            Object,
        },
    };

    #[test]
    fn heading_1() {
        let heading_1: Block = serde_json::from_str(include_str!("tests/heading_1.json")).unwrap();
        assert_eq!(
            heading_1,
            Block::Heading1 {
                common: BlockCommon {
                    id: BlockId::from_str("9e891834-6a03-475c-a2b8-421e17f0f3aa").unwrap(),
                    created_time: DateTime::from_str("2022-05-12T21:15:00.000Z").unwrap(),
                    last_edited_time: DateTime::from_str("2022-05-12T22:10:00.000Z").unwrap(),
                    has_children: false,
                    created_by: UserCommon {
                        id: UserId::from_str("6419f912-5293-4ea8-b2c8-9c3ce44f90e3").unwrap(),
                        name: None,
                        avatar_url: None,
                    },
                    last_edited_by: UserCommon {
                        id: UserId::from_str("6419f912-5293-4ea8-b2c8-9c3ce44f90e3").unwrap(),
                        name: None,
                        avatar_url: None,
                    },
                },
                heading_1: TextBlockModel {
                    rich_text: vec![
                        RichText::Text {
                            rich_text: RichTextCommon {
                                plain_text: "This".to_string(),
                                href: None,
                                annotations: Some(Annotations {
                                    bold: Some(false),
                                    code: Some(true),
                                    color: Some(TextColor::Default),
                                    italic: Some(false),
                                    strikethrough: Some(false),
                                    underline: Some(false),
                                }),
                            },
                            text: Text {
                                content: "This".to_string(),
                                link: None,
                            },
                        },
                        RichText::Text {
                            rich_text: RichTextCommon {
                                plain_text: " ".to_string(),
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
                            text: Text {
                                content: " ".to_string(),
                                link: None,
                            },
                        },
                        RichText::Text {
                            rich_text: RichTextCommon {
                                plain_text: "is".to_string(),
                                href: None,
                                annotations: Some(Annotations {
                                    bold: Some(false),
                                    code: Some(false),
                                    color: Some(TextColor::Default),
                                    italic: Some(false),
                                    strikethrough: Some(false),
                                    underline: Some(true),
                                }),
                            },
                            text: Text {
                                content: "is".to_string(),
                                link: None,
                            },
                        },
                        RichText::Text {
                            rich_text: RichTextCommon {
                                plain_text: " ".to_string(),
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
                            text: Text {
                                content: " ".to_string(),
                                link: None,
                            },
                        },
                        RichText::Text {
                            rich_text: RichTextCommon {
                                plain_text: "a".to_string(),
                                href: None,
                                annotations: Some(Annotations {
                                    bold: Some(false),
                                    code: Some(false),
                                    color: Some(TextColor::Default),
                                    italic: Some(true),
                                    strikethrough: Some(false),
                                    underline: Some(true),
                                }),
                            },
                            text: Text {
                                content: "a".to_string(),
                                link: None,
                            },
                        },
                        RichText::Text {
                            rich_text: RichTextCommon {
                                plain_text: " ".to_string(),
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
                            text: Text {
                                content: " ".to_string(),
                                link: None,
                            },
                        },
                        RichText::Text {
                            rich_text: RichTextCommon {
                                plain_text: "Heading".to_string(),
                                href: None,
                                annotations: Some(Annotations {
                                    bold: Some(false),
                                    code: Some(false),
                                    color: Some(TextColor::Default),
                                    italic: Some(true),
                                    strikethrough: Some(false),
                                    underline: Some(false),
                                }),
                            },
                            text: Text {
                                content: "Heading".to_string(),
                                link: None,
                            },
                        },
                        RichText::Text {
                            rich_text: RichTextCommon {
                                plain_text: " ".to_string(),
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
                            text: Text {
                                content: " ".to_string(),
                                link: None,
                            },
                        },
                        RichText::Text {
                            rich_text: RichTextCommon {
                                plain_text: "1".to_string(),
                                href: None,
                                annotations: Some(Annotations {
                                    bold: Some(false),
                                    code: Some(false),
                                    color: Some(TextColor::Default),
                                    italic: Some(false),
                                    strikethrough: Some(true),
                                    underline: Some(false),
                                }),
                            },
                            text: Text {
                                content: "1".to_string(),
                                link: None,
                            },
                        },
                    ]
                },
            }
        )
    }

    #[test]
    fn emoji_object() {
        let emoji_object: FileOrEmojiObject =
            serde_json::from_str(include_str!("tests/emoji_object.json")).unwrap();
        assert_eq!(
            emoji_object,
            FileOrEmojiObject::Emoji {
                emoji: "💡".to_string()
            }
        )
    }

    #[test]
    fn file_object() {
        let file_object: FileOrEmojiObject =
            serde_json::from_str(include_str!("tests/file_object.json")).unwrap();
        assert_eq!(file_object, FileOrEmojiObject::File {
            file: InternalFileObject {
                url: "https://s3.us-west-2.amazonaws.com/secure.notion-static.com/2703e742-ace5-428c-a74d-1c587ceddc32/DiRT_Rally.png?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Content-Sha256=UNSIGNED-PAYLOAD&X-Amz-Credential=AKIAT73L2G45EIPT3X45%2F20220513%2Fus-west-2%2Fs3%2Faws4_request&X-Amz-Date=20220513T201035Z&X-Amz-Expires=3600&X-Amz-Signature=714b49bde0b499fb8f3aae1a88a8cbd374f2b09c1d128e91cac49e85ce0e00fb&X-Amz-SignedHeaders=host&x-id=GetObject".to_string(),
                expiry_time: DateTime::from_str("2022-05-13T21:10:35.817Z").unwrap(),
            }
        })
    }

    #[test]
    fn external_file_object() {
        let external_file_object: FileOrEmojiObject =
            serde_json::from_str(include_str!("tests/external_file_object.json")).unwrap();
        assert_eq!(
            external_file_object,
            FileOrEmojiObject::External {
                external: ExternalFileObject {
                    url: "https://nerdist.com/wp-content/uploads/2020/07/maxresdefault.jpg"
                        .to_string(),
                }
            }
        )
    }

    #[test]
    fn callout() {
        let callout: Object = serde_json::from_str(include_str!("tests/callout.json")).unwrap();
        assert_eq!(
            callout,
            Object::Block {
                block: Block::Callout {
                    common: BlockCommon {
                        id: BlockId::from_str("00e8829a-a7b8-4075-884a-8f53be145d2f").unwrap(),
                        created_time: DateTime::from_str("2022-05-13T20:08:00.000Z").unwrap(),
                        last_edited_time: DateTime::from_str("2022-05-13T20:08:00.000Z").unwrap(),
                        has_children: true,
                        created_by: UserCommon {
                            id: UserId::from_str("e2507360-468c-4e0f-a928-7bbcbbb45353").unwrap(),
                            name: None,
                            avatar_url: None,
                        },
                        last_edited_by: UserCommon {
                            id: UserId::from_str("e2507360-468c-4e0f-a928-7bbcbbb45353").unwrap(),
                            name: None,
                            avatar_url: None,
                        },
                    },
                    callout: Callout {
                        rich_text: vec![RichText::Text {
                            rich_text: RichTextCommon {
                                plain_text: "Test callout".to_string(),
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
                            text: Text {
                                content: "Test callout".to_string(),
                                link: None
                            },
                        }],
                        icon: FileOrEmojiObject::Emoji {
                            emoji: "💡".to_string()
                        },
                        color: TextColor::Green,
                    },
                }
            }
        )
    }
}

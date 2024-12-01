// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{
    debug_query, upsert::excluded, BoolExpressionMethods, ExpressionMethods, QueryDsl, Queryable,
};
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use insta::assert_display_snapshot;
use sui_pg_db::temp::TempDb;

use super::*;

diesel::table! {
    objects (object_id) {
        object_id -> Bytea,
        version -> Int8,
        kind -> Int2,
        owner -> Nullable<Bytea>,
        type_ -> Nullable<Text>,
    }
}

#[derive(Insertable, Queryable, Debug, Clone, Eq, PartialEq)]
#[diesel(table_name = objects, primary_key(object_id))]
#[diesel(treat_none_as_default_value = false)]
struct StoredObject {
    object_id: Vec<u8>,
    version: i64,
    kind: i16,
    owner: Option<Vec<u8>>,
    type_: Option<String>,
}

#[test]
fn test_update_from() {
    let query = update_from(objects::table)
        .set((
            objects::version.eq(excluded(objects::version)),
            objects::kind.eq(excluded(objects::kind)),
            objects::owner.eq(excluded(objects::owner)),
            objects::type_.eq(excluded(objects::type_)),
        ))
        .filter(objects::object_id.eq(excluded(objects::object_id)))
        .values(vec![
            StoredObject {
                object_id: vec![1, 2, 3],
                version: 1,
                kind: 2,
                owner: None,
                type_: Some("type".to_string()),
            },
            StoredObject {
                object_id: vec![4, 5, 6],
                version: 2,
                kind: 3,
                owner: Some(vec![7, 8, 9]),
                type_: None,
            },
        ]);

    assert_display_snapshot!(debug_query::<Pg, _>(&query), @r###"UPDATE "objects" SET "version" = excluded."version", "kind" = excluded."kind", "owner" = excluded."owner", "type_" = excluded."type_" FROM (VALUES ($1, $2, $3, $4, $5), ($6, $7, $8, $9, $10)) AS excluded ("object_id", "version", "kind", "owner", "type_") WHERE ("objects"."object_id" = excluded."object_id") -- binds: [[1, 2, 3], 1, 2, None, Some("type"), [4, 5, 6], 2, 3, Some([7, 8, 9]), None]"###);
}

#[test]
fn test_update_from_empty() {
    let query = update_from(objects::table)
        .set((
            objects::version.eq(excluded(objects::version)),
            objects::kind.eq(excluded(objects::kind)),
            objects::owner.eq(excluded(objects::owner)),
            objects::type_.eq(excluded(objects::type_)),
        ))
        .filter(objects::object_id.eq(excluded(objects::object_id)))
        .values::<Vec<StoredObject>>(vec![]);

    assert_display_snapshot!(debug_query::<Pg, _>(&query), @r###"UPDATE "objects" SET "version" = excluded."version", "kind" = excluded."kind", "owner" = excluded."owner", "type_" = excluded."type_" WHERE 1=0 -- binds: []"###);
}

/// Update all the columns from the model type.
#[tokio::test]
async fn test_bulk_update() {
    let temp_db = TempDb::new().unwrap();
    setup_objects_table(&temp_db).await;

    update_from(objects::table)
        .set((
            objects::version.eq(excluded(objects::version)),
            objects::kind.eq(excluded(objects::kind)),
            objects::owner.eq(excluded(objects::owner)),
            objects::type_.eq(excluded(objects::type_)),
        ))
        .filter(objects::object_id.eq(excluded(objects::object_id)))
        .values(vec![
            StoredObject {
                object_id: vec![1, 2, 3],
                version: 200,
                kind: 300,
                owner: Some(vec![4, 5, 6]),
                type_: None,
            },
            StoredObject {
                object_id: vec![4, 5, 6],
                version: 300,
                kind: 200,
                owner: None,
                type_: Some("qux".to_string()),
            },
        ])
        .execute(&mut conn(&temp_db).await)
        .await
        .unwrap();

    let objects: Vec<StoredObject> = objects::table
        .order_by(objects::dsl::object_id)
        .load(&mut conn(&temp_db).await)
        .await
        .unwrap();

    assert_eq!(
        objects,
        vec![
            StoredObject {
                object_id: vec![1, 2, 3],
                version: 200,
                kind: 300,
                owner: Some(vec![4, 5, 6]),
                type_: None,
            },
            StoredObject {
                object_id: vec![4, 5, 6],
                version: 300,
                kind: 200,
                owner: None,
                type_: Some("qux".to_string()),
            },
            StoredObject {
                object_id: vec![10, 11, 12],
                version: 3,
                kind: 0,
                owner: Some(vec![13, 14, 15]),
                type_: Some("bar".to_string()),
            },
            StoredObject {
                object_id: vec![16, 17, 18],
                version: 4,
                kind: 3,
                owner: None,
                type_: None,
            },
            StoredObject {
                object_id: vec![19, 20, 21],
                version: 5,
                kind: 2,
                owner: Some(vec![22, 23, 24]),
                type_: Some("baz".to_string()),
            },
        ]
    );
}

/// Only update certain columns from the model type.
#[tokio::test]
async fn test_bulk_update_partial_rows() {
    let temp_db = TempDb::new().unwrap();
    setup_objects_table(&temp_db).await;

    update_from(objects::table)
        .set((
            objects::version.eq(excluded(objects::version)),
            objects::type_.eq(excluded(objects::type_)),
        ))
        .filter(objects::object_id.eq(excluded(objects::object_id)))
        .values(vec![
            StoredObject {
                object_id: vec![1, 2, 3],
                version: 200,
                kind: 300,
                owner: Some(vec![4, 5, 6]),
                type_: None,
            },
            StoredObject {
                object_id: vec![4, 5, 6],
                version: 300,
                kind: 200,
                owner: None,
                type_: Some("qux".to_string()),
            },
        ])
        .execute(&mut conn(&temp_db).await)
        .await
        .unwrap();

    let objects: Vec<StoredObject> = objects::table
        .order_by(objects::dsl::object_id)
        .load(&mut conn(&temp_db).await)
        .await
        .unwrap();

    assert_eq!(
        objects,
        vec![
            StoredObject {
                object_id: vec![1, 2, 3],
                version: 200,
                kind: 2,
                owner: None,
                type_: None,
            },
            StoredObject {
                object_id: vec![4, 5, 6],
                version: 300,
                kind: 1,
                owner: Some(vec![7, 8, 9]),
                type_: Some("qux".to_string()),
            },
            StoredObject {
                object_id: vec![10, 11, 12],
                version: 3,
                kind: 0,
                owner: Some(vec![13, 14, 15]),
                type_: Some("bar".to_string()),
            },
            StoredObject {
                object_id: vec![16, 17, 18],
                version: 4,
                kind: 3,
                owner: None,
                type_: None,
            },
            StoredObject {
                object_id: vec![19, 20, 21],
                version: 5,
                kind: 2,
                owner: Some(vec![22, 23, 24]),
                type_: Some("baz".to_string()),
            },
        ]
    );
}

#[tokio::test]
async fn test_bulk_update_filtered() {
    let temp_db = TempDb::new().unwrap();
    setup_objects_table(&temp_db).await;

    update_from(objects::table)
        .set((
            objects::version.eq(excluded(objects::version)),
            objects::kind.eq(excluded(objects::kind)),
            objects::owner.eq(excluded(objects::owner)),
            objects::type_.eq(excluded(objects::type_)),
        ))
        .filter(
            objects::object_id
                .eq(excluded(objects::object_id))
                .and(objects::version.ge(3)),
        )
        .values(vec![
            StoredObject {
                object_id: vec![1, 2, 3],
                version: 200,
                kind: 300,
                owner: Some(vec![4, 5, 6]),
                type_: None,
            },
            StoredObject {
                object_id: vec![4, 5, 6],
                version: 300,
                kind: 200,
                owner: None,
                type_: Some("qux".to_string()),
            },
            StoredObject {
                object_id: vec![10, 11, 12],
                version: 400,
                kind: 300,
                owner: None,
                type_: None,
            },
            StoredObject {
                object_id: vec![19, 20, 21],
                version: 500,
                kind: 200,
                owner: Some(vec![24, 23, 22]),
                type_: Some("quy".to_string()),
            },
        ])
        .execute(&mut conn(&temp_db).await)
        .await
        .unwrap();

    let objects: Vec<StoredObject> = objects::table
        .order_by(objects::dsl::object_id)
        .load(&mut conn(&temp_db).await)
        .await
        .unwrap();

    assert_eq!(
        objects,
        vec![
            StoredObject {
                object_id: vec![1, 2, 3],
                version: 1,
                kind: 2,
                owner: None,
                type_: Some("foo".to_string()),
            },
            StoredObject {
                object_id: vec![4, 5, 6],
                version: 2,
                kind: 1,
                owner: Some(vec![7, 8, 9]),
                type_: None,
            },
            StoredObject {
                object_id: vec![10, 11, 12],
                version: 400,
                kind: 300,
                owner: None,
                type_: None,
            },
            StoredObject {
                object_id: vec![16, 17, 18],
                version: 4,
                kind: 3,
                owner: None,
                type_: None,
            },
            StoredObject {
                object_id: vec![19, 20, 21],
                version: 500,
                kind: 200,
                owner: Some(vec![24, 23, 22]),
                type_: Some("quy".to_string()),
            },
        ]
    );
}

async fn conn(temp_db: &TempDb) -> AsyncPgConnection {
    AsyncPgConnection::establish(temp_db.database().url().as_str())
        .await
        .expect("Failed to establish connection")
}

async fn setup_objects_table(temp_db: &TempDb) {
    let mut conn = conn(temp_db).await;

    diesel::sql_query(
        r#"
        CREATE TABLE objects (
            object_id       BYTEA           PRIMARY KEY,
            version         BIGINT          NOT NULL,
            kind            SMALLINT        NOT NULL,
            owner           BYTEA,
            type_           TEXT
        )
        "#,
    )
    .execute(&mut conn)
    .await
    .unwrap();

    diesel::insert_into(objects::table)
        .values(vec![
            StoredObject {
                object_id: vec![1, 2, 3],
                version: 1,
                kind: 2,
                owner: None,
                type_: Some("foo".to_string()),
            },
            StoredObject {
                object_id: vec![4, 5, 6],
                version: 2,
                kind: 1,
                owner: Some(vec![7, 8, 9]),
                type_: None,
            },
            StoredObject {
                object_id: vec![10, 11, 12],
                version: 3,
                kind: 0,
                owner: Some(vec![13, 14, 15]),
                type_: Some("bar".to_string()),
            },
            StoredObject {
                object_id: vec![16, 17, 18],
                version: 4,
                kind: 3,
                owner: None,
                type_: None,
            },
            StoredObject {
                object_id: vec![19, 20, 21],
                version: 5,
                kind: 2,
                owner: Some(vec![22, 23, 24]),
                type_: Some("baz".to_string()),
            },
        ])
        .execute(&mut conn)
        .await
        .unwrap();
}

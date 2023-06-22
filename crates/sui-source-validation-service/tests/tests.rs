// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use expect_test::expect;
use reqwest::Client;
use serde::Deserialize;

use sui_source_validation_service::{initialize, serve, CloneCommand, Config, Packages};

use test_utils::network::TestClusterBuilder;

#[derive(Deserialize)]
struct Response {
    source: String,
}

#[tokio::test]
async fn test_api_route() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await;
    let context = &mut cluster.wallet;
    let config = Config { packages: vec![] };
    let tmp_dir = tempfile::tempdir()?;

    initialize(context, &config, tmp_dir.path()).await?;
    tokio::spawn(serve().expect("Cannot start service."));

    let client = Client::new();
    let json = client
        .get("http://0.0.0.0:8000/api")
        .send()
        .await
        .expect("Request failed.")
        .json::<Response>()
        .await?;

    let expected = expect!["code"];
    expected.assert_eq(&json.source);
    Ok(())
}

#[test]
fn test_parse_package_config() -> anyhow::Result<()> {
    let config = r#"
    [[packages]]
    repository = "https://github.com/mystenlabs/sui"
    paths = [
        "crates/sui-framework/packages/deepbook",
        "crates/sui-framework/packages/move-stdlib",
        "crates/sui-framework/packages/sui-framework",
        "crates/sui-framework/packages/sui-system",
    ]
"#;

    let config: Config = toml::from_str(config).unwrap();
    let expect = expect![
        r#"Config {
    packages: [
        Packages {
            repository: "https://github.com/mystenlabs/sui",
            paths: [
                "crates/sui-framework/packages/deepbook",
                "crates/sui-framework/packages/move-stdlib",
                "crates/sui-framework/packages/sui-framework",
                "crates/sui-framework/packages/sui-system",
            ],
        },
    ],
}"#
    ];
    expect.assert_eq(&format!("{:#?}", config));
    Ok(())
}

#[test]
fn test_clone_command() -> anyhow::Result<()> {
    let packages = Packages {
        repository: "https://github.com/user/repo".into(),
        paths: vec!["a".into(), "b".into()],
    };

    let command = CloneCommand::new(&packages, PathBuf::from("/tmp").as_path())?;
    let expect = expect![
        r#"CloneCommand {
    args: [
        [
            "clone",
            "-n",
            "--depth=1",
            "--filter=tree:0",
            "https://github.com/user/repo",
            "/tmp/repo",
        ],
        [
            "-C",
            "/tmp/repo",
            "sparse-checkout",
            "set",
            "--no-cone",
            "a",
            "b",
        ],
        [
            "-C",
            "/tmp/repo",
            "checkout",
        ],
    ],
    repo_url: "https://github.com/user/repo",
}"#
    ];
    expect.assert_eq(&format!("{:#?}", command));
    Ok(())
}

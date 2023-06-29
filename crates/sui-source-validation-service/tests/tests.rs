// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use expect_test::expect;
use reqwest::Client;
use serde::Deserialize;
use std::path::PathBuf;
use sui_source_validation_service::{
    initialize, serve, verify_packages, CloneCommand, Config, Packages,
};
use test_cluster::TestClusterBuilder;

const TEST_FIXTURES_DIR: &str = "tests/fixture";

#[derive(Deserialize)]
struct Response {
    source: String,
}

#[tokio::test]
async fn test_verify_packages() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await;
    let context = &mut cluster.wallet;

    let config = Config {
        packages: vec![Packages {
            repository: "https://github.com/mystenlabs/sui".into(),
            branch: "main".into(),
            paths: vec!["move-stdlib".into()],
        }],
    };

    let fixtures = tempfile::tempdir()?;
    fs_extra::dir::copy(
        PathBuf::from(TEST_FIXTURES_DIR).join("sui"),
        fixtures.path(),
        &fs_extra::dir::CopyOptions::default(),
    )?;
    let result = verify_packages(context, &config, fixtures.path()).await;
    let truncated_error_message = &result
        .unwrap_err()
        .to_string()
        .lines()
        .take(3)
        .map(|s| s.into())
        .collect::<Vec<String>>()
        .join("\n");
    let expected = expect![
        r#"
Multiple source verification errors found:

- Local dependency did not match its on-chain version at 0000000000000000000000000000000000000000000000000000000000000001::MoveStdlib::address"#
    ];
    expected.assert_eq(truncated_error_message);
    Ok(())
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
    branch = "main"
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
            branch: "main",
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
        branch: "main".into(),
        paths: vec!["a".into(), "b".into()],
    };

    let command = CloneCommand::new(&packages, PathBuf::from("/tmp").as_path())?;
    let expect = expect![
        r#"CloneCommand {
    args: [
        [
            "clone",
            "--no-checkout",
            "--depth=1",
            "--filter=tree:0",
            "--branch=main",
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

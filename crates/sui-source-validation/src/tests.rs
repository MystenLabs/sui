// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use expect_test::expect;
use move_core_types::account_address::AccountAddress;
use std::collections::HashMap;
use std::{fs, io, path::Path};
use std::{path::PathBuf, str};
use sui_move_build::{BuildConfig, CompiledPackage, SuiPackageHooks};
use sui_sdk::wallet_context::WalletContext;
use sui_types::base_types::ObjectID;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    SUI_SYSTEM_STATE_OBJECT_ID,
};
use test_utils::network::TestClusterBuilder;
use test_utils::transaction::{publish_package_with_wallet, upgrade_package_with_wallet};

use crate::{BytecodeSourceVerifier, SourceMode};

#[tokio::test]
async fn successful_verification() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;

    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_published_package(&fixtures, "b", SuiAddress::ZERO).await?;
        publish_package(context, sender, b_src).await.0
    };

    let b_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_published_package(&fixtures, "b", b_ref.0.into()).await?;
        compile_package(b_src)
    };

    let (a_pkg, a_ref) = {
        let fixtures = tempfile::tempdir()?;
        copy_published_package(&fixtures, "b", b_ref.0.into()).await?;
        let a_src = copy_published_package(&fixtures, "a", SuiAddress::ZERO).await?;
        (
            compile_package(a_src.clone()),
            publish_package(context, sender, a_src).await.0,
        )
    };

    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    // Skip deps and root
    verifier
        .verify_package(&a_pkg, /* verify_deps */ false, SourceMode::Skip)
        .await
        .unwrap();

    // Verify root without updating the address
    verifier
        .verify_package(&b_pkg, /* verify_deps */ false, SourceMode::Verify)
        .await
        .unwrap();

    // Verify deps but skip root
    verifier.verify_package_deps(&a_pkg).await.unwrap();

    // Skip deps but verify root
    verifier
        .verify_package_root(&a_pkg, a_ref.0.into())
        .await
        .unwrap();

    // Verify both deps and root
    verifier
        .verify_package_root_and_deps(&a_pkg, a_ref.0.into())
        .await
        .unwrap();

    Ok(())
}

#[tokio::test]
async fn successful_verification_unpublished_deps() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;
    let fixtures = tempfile::tempdir()?;

    let a_src = {
        copy_published_package(&fixtures, "b", SuiAddress::ZERO).await?;
        copy_published_package(&fixtures, "a", SuiAddress::ZERO).await?
    };

    let a_pkg = compile_package(a_src.clone());
    let a_ref = publish_package_and_deps(context, sender, a_src).await;

    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    // Verify the root package which now includes dependency modules
    verifier
        .verify_package_root(&a_pkg, a_ref.0.into())
        .await
        .unwrap();

    Ok(())
}

#[tokio::test]
async fn successful_verification_module_ordering() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;

    // This package contains a module that refers to itself, and also to the sui framework.  Its
    // self-address is `0x0` (i.e. compares lower than the framework's `0x2`) before publishing,
    // and will be greater after publishing.
    //
    // This is a regression test for a source validation bug related to module order instability
    // where the on-chain package (which is compiled with self-address = 0x0, and later substituted)
    // orders module handles (references to other modules) differently to the package compiled as a
    // dependency with its self-address already set as its published address.
    let z_ref = {
        let fixtures = tempfile::tempdir()?;
        let z_src = copy_published_package(&fixtures, "z", SuiAddress::ZERO).await?;
        publish_package(context, sender, z_src).await.0
    };

    let z_pkg = {
        let fixtures = tempfile::tempdir()?;
        let z_src = copy_published_package(&fixtures, "z", z_ref.0.into()).await?;
        compile_package(z_src)
    };

    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    let verify_deps = false;
    verifier
        .verify_package(&z_pkg, verify_deps, SourceMode::Verify)
        .await
        .unwrap();

    Ok(())
}

#[tokio::test]
async fn successful_verification_upgrades() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;

    let (b_v1, b_cap) = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_published_package(&fixtures, "b", SuiAddress::ZERO).await?;
        publish_package(context, sender, b_src).await
    };

    let b_v2 = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_published_package(&fixtures, "b-v2", SuiAddress::ZERO).await?;
        upgrade_package(context, sender, b_v1.0, b_cap.0, b_src).await
    };

    let (b_pkg, e_pkg) = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_upgraded_package(&fixtures, "b-v2", b_v2.0.into(), b_v1.0.into()).await?;
        let e_src = copy_published_package(&fixtures, "e", SuiAddress::ZERO).await?;
        (compile_package(b_src), compile_package(e_src))
    };

    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    // Verify the upgraded package b-v2 as the root.
    let verify_deps = false;
    verifier
        .verify_package(&b_pkg, verify_deps, SourceMode::Verify)
        .await
        .unwrap();

    // Verify the upgraded package b-v2 as a dep of e.
    verifier.verify_package_deps(&e_pkg).await.unwrap();

    Ok(())
}

#[tokio::test]
async fn fail_verification_bad_address() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;

    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_published_package(&fixtures, "b", SuiAddress::ZERO).await?;
        publish_package(context, sender, b_src).await.0
    };

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        copy_published_package(&fixtures, "b", b_ref.0.into()).await?;
        let a_src = copy_published_package(&fixtures, "a", SuiAddress::ZERO).await?;
        publish_package(context, sender, a_src.clone()).await;
        compile_package(a_src)
    };

    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    let expected = expect!["On-chain address cannot be zero"];
    expected.assert_eq(
        &verifier
            .verify_package_root_and_deps(&a_pkg, AccountAddress::ZERO)
            .await
            .unwrap_err()
            .to_string(),
    );

    Ok(())
}

#[tokio::test]
async fn fail_to_verify_unpublished_root() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let context = &mut cluster.wallet;

    let b_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_published_package(&fixtures, "b", SuiAddress::ZERO).await?;
        compile_package(b_src)
    };

    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    // Trying to verify the root package, which hasn't been published -- this is going to fail
    // because there is no on-chain package to verify against.
    let expected = expect!["Invalid module b with error: Can't verify unpublished source"];
    expected.assert_eq(
        &verifier
            .verify_package(&b_pkg, /* verify_deps */ false, SourceMode::Verify)
            .await
            .unwrap_err()
            .to_string(),
    );

    Ok(())
}

#[tokio::test]
async fn rpc_call_failed_during_verify() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;

    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_published_package(&fixtures, "b", SuiAddress::ZERO).await?;
        publish_package(context, sender, b_src).await.0
    };

    let a_ref = {
        let fixtures = tempfile::tempdir()?;
        copy_published_package(&fixtures, "b", b_ref.0.into()).await?;
        let a_src = copy_published_package(&fixtures, "a", SuiAddress::ZERO).await?;
        publish_package(context, sender, a_src).await.0
    };
    let _a_addr: SuiAddress = a_ref.0.into();

    let client = context.get_client().await?;
    let _verifier = BytecodeSourceVerifier::new(client.read_api());

    /*
    // TODO: Dropping cluster no longer stops the network. Need to look into this and see
    // what we want to do with it.
    // Stop the network, so future RPC requests fail.
    drop(cluster);

    assert!(matches!(
        verifier.verify_package_deps(&a_pkg).await,
        Err(SourceVerificationError::DependencyObjectReadFailure(_)),
    ),);

    assert!(matches!(
        verifier
            .verify_package_root_and_deps(&a_pkg, a_addr.into())
            .await,
        Err(SourceVerificationError::DependencyObjectReadFailure(_)),
    ),);

    assert!(matches!(
        verifier
            .verify_package_root(&a_pkg, a_addr.into())
            .await,
        Err(SourceVerificationError::DependencyObjectReadFailure(_)),
    ),);

     */

    Ok(())
}

#[tokio::test]
async fn package_not_found() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let context = &mut cluster.wallet;
    let mut stable_addrs = HashMap::new();

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_id = SuiAddress::random_for_testing_only();
        stable_addrs.insert(b_id, "<id>");
        copy_published_package(&fixtures, "b", b_id).await?;
        let a_src = copy_published_package(&fixtures, "a", SuiAddress::ZERO).await?;
        compile_package(a_src)
    };

    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    let Err(err) = verifier.verify_package_deps(&a_pkg).await else {
        panic!("Expected verification to fail");
    };

    let expected =
        expect!["Dependency object does not exist or was deleted: NotExists { object_id: 0x<id> }"];
    expected.assert_eq(&sanitize_id(err.to_string(), &stable_addrs));

    let package_root = AccountAddress::random();
    stable_addrs.insert(SuiAddress::from(package_root), "<id>");
    let Err(err) = verifier.verify_package_root_and_deps(
	&a_pkg,
	package_root,
    ).await else {
	panic!("Expected verification to fail");
    };

    // <id> below may refer to either the package_root or dependent package `b`
    // (the check reports the first missing object nondeterministically)
    let expected =
        expect!["Dependency object does not exist or was deleted: NotExists { object_id: 0x<id> }"];
    expected.assert_eq(&sanitize_id(err.to_string(), &stable_addrs));

    let package_root = AccountAddress::random();
    stable_addrs.insert(SuiAddress::from(package_root), "<id>");
    let Err(err) = verifier.verify_package_root(
	&a_pkg,
	package_root,
    ).await else {
	panic!("Expected verification to fail");
    };

    let expected =
        expect!["Dependency object does not exist or was deleted: NotExists { object_id: 0x<id> }"];
    expected.assert_eq(&sanitize_id(err.to_string(), &stable_addrs));

    Ok(())
}

#[tokio::test]
async fn dependency_is_an_object() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let context = &mut cluster.wallet;

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_id = SUI_SYSTEM_STATE_OBJECT_ID.into();
        copy_published_package(&fixtures, "b", b_id).await?;
        let a_src = copy_published_package(&fixtures, "a", SuiAddress::ZERO).await?;
        compile_package(a_src)
    };
    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    let expected = expect!["Dependency ID contains a Sui object, not a Move package: 0x0000000000000000000000000000000000000000000000000000000000000005"];
    expected.assert_eq(
        &verifier
            .verify_package_deps(&a_pkg)
            .await
            .unwrap_err()
            .to_string(),
    );

    Ok(())
}

#[tokio::test]
async fn module_not_found_on_chain() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;

    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_published_package(&fixtures, "b", SuiAddress::ZERO).await?;
        tokio::fs::remove_file(b_src.join("sources").join("c.move")).await?;
        publish_package(context, sender, b_src).await.0
    };

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        copy_published_package(&fixtures, "b", b_ref.0.into()).await?;
        let a_src = copy_published_package(&fixtures, "a", SuiAddress::ZERO).await?;
        compile_package(a_src)
    };
    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    let Err(err) = verifier.verify_package_deps(&a_pkg).await else {
        panic!("Expected verification to fail");
    };

    let expected = expect!["On-chain version of dependency b::c was not found."];
    expected.assert_eq(&err.to_string());

    Ok(())
}

#[tokio::test]
async fn module_not_found_locally() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;
    let mut stable_addrs = HashMap::new();

    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_published_package(&fixtures, "b", SuiAddress::ZERO).await?;
        publish_package(context, sender, b_src).await.0
    };

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_id = b_ref.0.into();
        stable_addrs.insert(b_id, "b_id");
        let b_src = copy_published_package(&fixtures, "b", b_id).await?;
        let a_src = copy_published_package(&fixtures, "a", SuiAddress::ZERO).await?;
        tokio::fs::remove_file(b_src.join("sources").join("d.move")).await?;
        compile_package(a_src)
    };

    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    let Err(err) = verifier.verify_package_deps(&a_pkg).await else {
        panic!("Expected verification to fail");
    };

    let expected = expect!["Local version of dependency b_id::d was not found."];
    expected.assert_eq(&sanitize_id(err.to_string(), &stable_addrs));

    Ok(())
}

#[tokio::test]
async fn module_bytecode_mismatch() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;
    let mut stable_addrs = HashMap::new();

    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_published_package(&fixtures, "b", SuiAddress::ZERO).await?;

        // Modify a module before publishing
        let c_path = b_src.join("sources").join("c.move");
        let c_file = tokio::fs::read_to_string(&c_path)
            .await?
            .replace("43", "44");
        tokio::fs::write(&c_path, c_file).await?;

        publish_package(context, sender, b_src).await.0
    };

    let (a_pkg, a_ref) = {
        let fixtures = tempfile::tempdir()?;
        let b_id = b_ref.0.into();
        stable_addrs.insert(b_id, "<b_id>");
        copy_published_package(&fixtures, "b", b_id).await?;
        let a_src = copy_published_package(&fixtures, "a", SuiAddress::ZERO).await?;

        let compiled = compile_package(a_src.clone());
        // Modify a module before publishing
        let c_path = a_src.join("sources").join("a.move");
        let c_file = tokio::fs::read_to_string(&c_path)
            .await?
            .replace("123", "1234");
        tokio::fs::write(&c_path, c_file).await?;

        (compiled, publish_package(context, sender, a_src).await.0)
    };
    let a_addr: SuiAddress = a_ref.0.into();
    stable_addrs.insert(a_addr, "<a_addr>");

    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    let Err(err) = verifier.verify_package_deps(&a_pkg).await else {
        panic!("Expected verification to fail");
    };

    let expected = expect!["Local dependency did not match its on-chain version at <b_id>::b::c"];
    expected.assert_eq(&sanitize_id(err.to_string(), &stable_addrs));

    let Err(err) = verifier.verify_package_root(&a_pkg, a_addr.into()).await else {
        panic!("Expected verification to fail");
    };

    let expected = expect!["Local dependency did not match its on-chain version at <a_addr>::a::a"];
    expected.assert_eq(&sanitize_id(err.to_string(), &stable_addrs));

    Ok(())
}

#[tokio::test]
async fn multiple_failures() -> anyhow::Result<()> {
    let mut cluster = TestClusterBuilder::new().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;
    let mut stable_addrs = HashMap::new();

    // Publish package `b::b` on-chain without c.move.
    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_published_package(&fixtures, "b", SuiAddress::ZERO).await?;
        tokio::fs::remove_file(b_src.join("sources").join("c.move")).await?;
        publish_package(context, sender, b_src).await.0
    };

    // Publish package `c::c` on-chain, unmodified.
    let c_ref = {
        let fixtures = tempfile::tempdir()?;
        let c_src = copy_published_package(&fixtures, "c", SuiAddress::ZERO).await?;
        publish_package(context, sender, c_src).await.0
    };

    // Compile local package `d` that references:
    // - `b::b` (c.move exists locally but not on chain => error)
    // - `c::c` (d.move exists on-chain but we delete it locally before compiling => error)
    let d_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_id = b_ref.0.into();
        let c_id = c_ref.0.into();
        stable_addrs.insert(b_id, "<b_id>");
        stable_addrs.insert(c_id, "<c_id>");
        copy_published_package(&fixtures, "b", b_id).await?;
        let c_src = copy_published_package(&fixtures, "c", c_id).await?;
        let d_src = copy_published_package(&fixtures, "d", SuiAddress::ZERO).await?;
        tokio::fs::remove_file(c_src.join("sources").join("d.move")).await?; // delete local module in `c`
        compile_package(d_src)
    };

    let client = context.get_client().await?;
    let verifier = BytecodeSourceVerifier::new(client.read_api());

    let Err(err) = verifier.verify_package_deps(&d_pkg).await else {
        panic!("Expected verification to fail");
    };

    let expected = expect![[r#"
        Multiple source verification errors found:

        - On-chain version of dependency b::c was not found.
        - Local version of dependency <c_id>::d was not found."#]];
    expected.assert_eq(&sanitize_id(err.to_string(), &stable_addrs));

    Ok(())
}

/// Compile the package at absolute path `package`.
fn compile_package(package: impl AsRef<Path>) -> CompiledPackage {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    BuildConfig::new_for_testing()
        .build(package.as_ref().to_path_buf())
        .unwrap()
}

fn sanitize_id(mut message: String, m: &HashMap<SuiAddress, &str>) -> String {
    for (addr, label) in m {
        message = message.replace(format!("{addr}").strip_prefix("0x").unwrap(), label);
    }
    message
}

/// Compile and publish package at absolute path `package` to chain.
async fn publish_package(
    context: &WalletContext,
    sender: SuiAddress,
    package: impl AsRef<Path>,
) -> (ObjectRef, ObjectRef) {
    let package = compile_package(package);
    let package_bytes = package.get_package_bytes(/* with_unpublished_deps */ false);
    let package_deps = package.dependency_ids.published.into_values().collect();
    let (package, _, cap, _) =
        publish_package_with_wallet(context, sender, package_bytes, package_deps).await;
    (package, cap)
}

async fn upgrade_package(
    context: &WalletContext,
    sender: SuiAddress,
    package_id: ObjectID,
    upgrade_cap: ObjectID,
    package: impl AsRef<Path>,
) -> ObjectRef {
    let package = compile_package(package);
    let with_unpublished_deps = false;
    let hash_modules = true;
    let package_bytes = package.get_package_bytes(with_unpublished_deps);
    let package_digest = package
        .get_package_digest(with_unpublished_deps, hash_modules)
        .to_vec();
    let package_deps = package.dependency_ids.published.into_values().collect();

    upgrade_package_with_wallet(
        context,
        sender,
        package_id,
        upgrade_cap,
        package_bytes,
        package_deps,
        package_digest,
    )
    .await
    .0
}

/// Compile and publish package at absolute path `package` to chain, along with its unpublished
/// dependencies.
async fn publish_package_and_deps(
    context: &WalletContext,
    sender: SuiAddress,
    package: impl AsRef<Path>,
) -> ObjectRef {
    let package = compile_package(package);
    let package_bytes = package.get_package_bytes(/* with_unpublished_deps */ true);
    let package_deps = package.get_dependency_original_package_ids();
    publish_package_with_wallet(context, sender, package_bytes, package_deps)
        .await
        .0
}

/// Copy `package` from fixtures into `directory`, setting its named address in the copied package's
/// `Move.toml` to `address`. (A fixture's self-address is assumed to match its package name).
async fn copy_published_package<'s>(
    directory: impl AsRef<Path>,
    package: &str,
    address: SuiAddress,
) -> io::Result<PathBuf> {
    copy_upgraded_package(directory, package, address, address).await
}

async fn copy_upgraded_package<'s>(
    directory: impl AsRef<Path>,
    package: &str,
    storage_id: SuiAddress,
    runtime_id: SuiAddress,
) -> io::Result<PathBuf> {
    let cargo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = {
        let mut path = cargo_root.clone();
        path.pop(); // sui-source-validation
        path.pop(); // crates
        path
    };

    let dst = directory.as_ref().join(package);
    let src = {
        let mut buf = cargo_root.clone();
        buf.push("fixture");
        buf.push(package);
        buf
    };

    // Create destination directory
    tokio::fs::create_dir(&dst).await?;

    // Copy TOML, performing replacements
    let mut toml = tokio::fs::read_to_string(src.join("Move.toml")).await?;
    toml = toml.replace("$REPO_ROOT", &repo_root.to_string_lossy());
    toml = toml.replace("$STORAGE_ID", &storage_id.to_string());
    toml = toml.replace("$RUNTIME_ID", &runtime_id.to_string());
    tokio::fs::write(dst.join("Move.toml"), toml).await?;

    // Make destination source directory
    tokio::fs::create_dir(dst.join("sources")).await?;

    // Copy source files
    for entry in fs::read_dir(src.join("sources"))? {
        let entry = entry?;
        assert!(entry.file_type()?.is_file());

        let src_abs = entry.path();
        let src_rel = src_abs.strip_prefix(&src).unwrap();
        let dst_abs = dst.join(src_rel);
        tokio::fs::copy(src_abs, dst_abs).await?;
    }

    Ok(dst)
}

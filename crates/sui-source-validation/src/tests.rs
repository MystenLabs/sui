// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use sui::client_commands::WalletContext;
use sui_framework_build::compiled_package::{BuildConfig, CompiledPackage};
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    SUI_SYSTEM_STATE_OBJECT_ID,
};
use test_utils::network::init_cluster_builder_env_aware;
use test_utils::transaction::publish_package_with_wallet;

use crate::{BytecodeSourceVerifier, DependencyVerificationError};

#[tokio::test]
async fn successful_verification() -> anyhow::Result<()> {
    let mut cluster = init_cluster_builder_env_aware().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;

    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_package(&fixtures, "b", [("b", SuiAddress::ZERO)]).await?;
        publish_package(context, sender, b_src).await
    };

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_id = b_ref.0.into();
        copy_package(&fixtures, "b", [("b", b_id)]).await?;
        let a_src = copy_package(&fixtures, "a", [("a", SuiAddress::ZERO), ("b", b_id)]).await?;
        compile_package(a_src)
    };

    let verifier = BytecodeSourceVerifier::new(context.client.read_api(), false);
    verifier
        .verify_deployed_dependencies(&a_pkg.package)
        .await
        .unwrap();

    Ok(())
}

#[tokio::test]
async fn rpc_call_failed_during_verify() -> anyhow::Result<()> {
    let mut cluster = init_cluster_builder_env_aware().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;

    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_package(&fixtures, "b", [("b", SuiAddress::ZERO)]).await?;
        publish_package(context, sender, b_src).await
    };

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_id = b_ref.0.into();
        copy_package(&fixtures, "b", [("b", b_id)]).await?;
        let a_src = copy_package(&fixtures, "a", [("a", SuiAddress::ZERO), ("b", b_id)]).await?;
        compile_package(a_src)
    };

    let client = context.client.clone();
    let verifier = BytecodeSourceVerifier::new(client.read_api(), false);

    // Stop the network, so future RPC requests fail.
    drop(cluster);

    assert!(matches!(
        verifier.verify_deployed_dependencies(&a_pkg.package).await,
        Err(DependencyVerificationError::DependencyObjectReadFailure(_)),
    ),);

    Ok(())
}

#[tokio::test]
async fn package_not_found() -> anyhow::Result<()> {
    let mut cluster = init_cluster_builder_env_aware().build().await?;
    let context = &mut cluster.wallet;

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_id = SuiAddress::random_for_testing_only();
        copy_package(&fixtures, "b", [("b", b_id)]).await?;
        let a_src = copy_package(&fixtures, "a", [("a", SuiAddress::ZERO), ("b", b_id)]).await?;
        compile_package(a_src)
    };

    let verifier = BytecodeSourceVerifier::new(context.client.read_api(), false);

    assert!(matches!(
        verifier.verify_deployed_dependencies(&a_pkg.package).await,
        Err(DependencyVerificationError::SuiObjectRefFailure(_)),
    ),);

    Ok(())
}

#[tokio::test]
async fn dependency_is_an_object() -> anyhow::Result<()> {
    let mut cluster = init_cluster_builder_env_aware().build().await?;
    let context = &mut cluster.wallet;

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_id = SUI_SYSTEM_STATE_OBJECT_ID.into();
        copy_package(&fixtures, "b", [("b", b_id)]).await?;
        let a_src = copy_package(&fixtures, "a", [("a", SuiAddress::ZERO), ("b", b_id)]).await?;
        compile_package(a_src)
    };

    let verifier = BytecodeSourceVerifier::new(context.client.read_api(), false);

    assert!(matches!(
        verifier.verify_deployed_dependencies(&a_pkg.package).await,
        Err(DependencyVerificationError::ObjectFoundWhenPackageExpected(
            SUI_SYSTEM_STATE_OBJECT_ID,
            _,
        )),
    ),);

    Ok(())
}

#[tokio::test]
async fn module_not_found_on_chain() -> anyhow::Result<()> {
    let mut cluster = init_cluster_builder_env_aware().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;

    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_package(&fixtures, "b", [("b", SuiAddress::ZERO)]).await?;
        tokio::fs::remove_file(b_src.join("sources").join("c.move")).await?;
        publish_package(context, sender, b_src).await
    };

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_id = b_ref.0.into();
        copy_package(&fixtures, "b", [("b", b_id)]).await?;
        let a_src = copy_package(&fixtures, "a", [("a", SuiAddress::ZERO), ("b", b_id)]).await?;
        compile_package(a_src)
    };

    let verifier = BytecodeSourceVerifier::new(context.client.read_api(), false);

    let Err(err) = verifier.verify_deployed_dependencies(&a_pkg.package).await else {
        panic!("Expected verification to fail");
    };

    let DependencyVerificationError::OnChainDependencyNotFound { package, module } = err else {
        panic!("Expected OnChainDependencyNotFound, got: {:?}", err);
    };

    assert_eq!(package, "b".into());
    assert_eq!(module, "c".into());

    Ok(())
}

#[tokio::test]
async fn module_not_found_locally() -> anyhow::Result<()> {
    let mut cluster = init_cluster_builder_env_aware().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;

    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_package(&fixtures, "b", [("b", SuiAddress::ZERO)]).await?;
        publish_package(context, sender, b_src).await
    };

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_id = b_ref.0.into();
        let b_src = copy_package(&fixtures, "b", [("b", b_id)]).await?;
        let a_src = copy_package(&fixtures, "a", [("a", SuiAddress::ZERO), ("b", b_id)]).await?;
        tokio::fs::remove_file(b_src.join("sources").join("d.move")).await?;
        compile_package(a_src)
    };

    let verifier = BytecodeSourceVerifier::new(context.client.read_api(), false);

    let Err(err) = verifier.verify_deployed_dependencies(&a_pkg.package).await else {
        panic!("Expected verification to fail");
    };

    let DependencyVerificationError::LocalDependencyNotFound { package, module } = err else {
        panic!("Expected LocalDependencyNotFound, got: {:?}", err);
    };

    assert_eq!(package, "b".into());
    assert_eq!(module, "d");

    Ok(())
}

#[tokio::test]
async fn module_bytecode_mismatch() -> anyhow::Result<()> {
    let mut cluster = init_cluster_builder_env_aware().build().await?;
    let sender = cluster.get_address_0();
    let context = &mut cluster.wallet;

    let b_ref = {
        let fixtures = tempfile::tempdir()?;
        let b_src = copy_package(&fixtures, "b", [("b", SuiAddress::ZERO)]).await?;

        // Modify a module before publishing
        let c_path = b_src.join("sources").join("c.move");
        let c_file = tokio::fs::read_to_string(&c_path)
            .await?
            .replace("43", "44");
        tokio::fs::write(&c_path, c_file).await?;

        publish_package(context, sender, b_src).await
    };

    let a_pkg = {
        let fixtures = tempfile::tempdir()?;
        let b_id = b_ref.0.into();
        copy_package(&fixtures, "b", [("b", b_id)]).await?;
        let a_src = copy_package(&fixtures, "a", [("a", SuiAddress::ZERO), ("b", b_id)]).await?;
        compile_package(a_src)
    };

    let verifier = BytecodeSourceVerifier::new(context.client.read_api(), false);

    let Err(err) = verifier.verify_deployed_dependencies(&a_pkg.package).await else {
        panic!("Expected verification to fail");
    };

    let DependencyVerificationError::ModuleBytecodeMismatch { address, package, module } = err else {
        panic!("Expected ModuleBytecodeMismatch, got: {:?}", err);
    };

    assert_eq!(address, b_ref.0.into());
    assert_eq!(package, "b".into());
    assert_eq!(module, "c".into());

    Ok(())
}

/// Compile the package at absolute path `package`.
fn compile_package(package: impl AsRef<Path>) -> CompiledPackage {
    sui_framework::build_move_package(package.as_ref(), BuildConfig::default()).unwrap()
}

/// Compile and publish package at absolute path `package` to chain.
async fn publish_package(
    context: &WalletContext,
    sender: SuiAddress,
    package: impl AsRef<Path>,
) -> ObjectRef {
    let package_bytes = compile_package(package).get_package_bytes();
    publish_package_with_wallet(context, sender, package_bytes).await
}

/// Copy `package` from fixtures into `directory`, setting the named address mapping in the copied
/// package's `Move.toml` according to `addresses`.
async fn copy_package<'s>(
    directory: impl AsRef<Path>,
    package: &str,
    addresses: impl IntoIterator<Item = (&'s str, SuiAddress)>,
) -> io::Result<PathBuf> {
    let dst = directory.as_ref().join(package);
    let src = {
        let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        buf.push("fixture");
        buf.push(package);
        buf
    };

    // Create destination directory
    tokio::fs::create_dir(&dst).await?;

    // Copy TOML
    let dst_toml = dst.join("Move.toml");
    tokio::fs::copy(src.join("Move.toml"), &dst_toml).await?;

    {
        let mut toml = fs::OpenOptions::new().append(true).open(dst_toml)?;
        writeln!(toml, "[addresses]")?;
        for (name, addr) in addresses {
            writeln!(toml, "{name} = \"{addr}\"")?;
        }
    }

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

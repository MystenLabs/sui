// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    build_config::BuildConfig,
    build_plan::BuildPlan,
    compiled_package::{
        BuildNamedAddresses, CompiledPackage, CompiledPackageInfo, CompiledUnitWithSource,
    },
    documentation::build_docs,
    shared,
    source_discovery::get_sources,
};

use crate::{
    layout::CompiledPackageLayout,
    on_disk_package::{OnDiskCompiledPackage, OnDiskPackage},
};
use std::{collections::BTreeSet, path::Path};

use anyhow::Result;
use colored::Colorize;
use move_compiler::{
    Compiler, Flags,
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::warning_filters::WarningFiltersBuilder,
    editions::{Edition, Flavor},
    linters,
    shared::{
        PackageConfig, PackagePaths, SaveFlag, SaveHook, files::MappedFiles,
        known_attributes::ModeAttribute,
    },
    sui_mode,
};
use move_docgen::DocgenFlags;
use move_package_alt::{
    errors::PackageResult, flavor::MoveFlavor, graph::PackageInfo, package::RootPackage,
    schema::Environment,
};
use move_symbol_pool::Symbol;
use std::{collections::BTreeMap, io::Write, path::PathBuf, str::FromStr};
use tracing::debug;
use vfs::VfsPath;

pub async fn compile_package<W: Write, F: MoveFlavor>(
    path: &Path,
    build_config: &BuildConfig,
    env: &Environment,
    writer: &mut W,
) -> PackageResult<CompiledPackage> {
    let root_pkg = RootPackage::<F>::load(path, env.clone()).await?;
    BuildPlan::create(&root_pkg, build_config)?.compile(writer, |compiler| compiler)
}

pub async fn compile_from_root_package<W: Write, F: MoveFlavor>(
    root_pkg: &RootPackage<F>,
    build_config: &BuildConfig,
    writer: &mut W,
) -> PackageResult<CompiledPackage> {
    BuildPlan::create(root_pkg, build_config)?.compile(writer, |compiler| compiler)
}

pub fn compiler_flags(build_config: &BuildConfig) -> Flags {
    let flags =
        if build_config.test_mode || build_config.modes.contains(&ModeAttribute::TEST.into()) {
            Flags::testing()
        } else {
            Flags::empty()
        };

    flags
        .set_warnings_are_errors(build_config.warnings_are_errors)
        .set_json_errors(build_config.json_errors)
        .set_silence_warnings(build_config.silence_warnings)
        .set_modes(build_config.modes.clone())
}

pub fn build_all<W: Write, F: MoveFlavor>(
    w: &mut W,
    vfs_root: Option<VfsPath>,
    root_pkg: &RootPackage<F>,
    build_config: &BuildConfig,
    compiler_driver: impl FnOnce(Compiler) -> Result<(MappedFiles, Vec<AnnotatedCompiledUnit>)>,
) -> Result<CompiledPackage> {
    let deps_published_ids = root_pkg.deps_published_ids().clone();
    let project_root = root_pkg.path().as_ref().to_path_buf();
    let program_info_hook = SaveHook::new([SaveFlag::TypingInfo]);
    let package_name = Symbol::from(root_pkg.name().as_str());
    let (file_map, all_compiled_units) =
        build_for_driver(w, vfs_root, build_config, root_pkg, |compiler| {
            let compiler = compiler.add_save_hook(&program_info_hook);
            compiler_driver(compiler)
        })?;

    let mut all_compiled_units_vec = vec![];
    let mut root_compiled_units = vec![];
    let mut deps_compiled_units = vec![];

    // TODO: improve/rework this? Renaming the root pkg to have a unique name for the compiler
    // this has to match whatever we're doing in build_for_driver function
    let root_package_name = Symbol::from(package_name.to_string());

    for mut annot_unit in all_compiled_units {
        let source_path = PathBuf::from(
            file_map
                .get(&annot_unit.loc().file_hash())
                .unwrap()
                .0
                .as_str(),
        );
        let package_name = annot_unit.named_module.package_name.unwrap();
        // unwraps below are safe as the source path exists (or must have existed at some point)
        // so it would be syntactically correct
        let file_name = PathBuf::from(source_path.file_name().unwrap());
        if let Ok(p) = dunce::canonicalize(source_path.parent().unwrap()) {
            annot_unit
                .named_module
                .source_map
                .set_from_file_path(p.join(file_name));
        }
        let unit = CompiledUnitWithSource {
            unit: annot_unit.named_module,
            source_path,
        };
        if package_name == root_package_name {
            root_compiled_units.push(unit.clone())
        } else {
            deps_compiled_units.push((package_name, unit.clone()))
        }
        all_compiled_units_vec.push((unit.source_path, unit.unit));
    }

    let mut compiled_docs = None;

    // TODO: probably we want a separate command for this rather than doing it as part of
    // compilation
    if build_config.generate_docs {
        // TODO: fix this root_name_address_map
        let root_named_address_map = BTreeMap::new();
        let program_info = program_info_hook.take_typing_info();
        let model = move_model_2::source_model::Model::from_source(
            file_map.clone(),
            Some(root_package_name),
            root_named_address_map,
            program_info,
            all_compiled_units_vec,
        )?;

        compiled_docs = Some(build_docs(
            DocgenFlags::default(), // TODO this should be configurable
            root_package_name,
            &model,
            &project_root,
            //TODO Fix this, it needs immediate dependencies for this pkg
            &[],
            // &immediate_dependencies,
            &build_config.install_dir,
        )?);
    };

    let compiled_package_info = CompiledPackageInfo {
        package_name,
        // // TODO: correct address alias instantiation
        // address_alias_instantiation: BTreeMap::new(),
        // TODO: compute source digest
        // source_digest: None,
        build_flags: build_config.clone(),
    };

    let under_path = shared::get_build_output_path(&project_root, build_config);

    save_to_disk(
        root_compiled_units.clone(),
        compiled_package_info.clone(),
        deps_compiled_units.clone(),
        compiled_docs,
        package_name,
        under_path,
    )?;

    let compiled_package = CompiledPackage {
        compiled_package_info,
        root_compiled_units,
        deps_compiled_units,
        compiled_docs: None,
        deps_published_ids,
        file_map,
        // compiled_docs,
    };

    Ok(compiled_package)
}

#[allow(unreachable_code)] // TODO
pub fn build_for_driver<W: Write, T, F: MoveFlavor>(
    w: &mut W,
    vfs_root: Option<VfsPath>,
    build_config: &BuildConfig,
    root_pkg: &RootPackage<F>,
    compiler_driver: impl FnOnce(Compiler) -> Result<T>,
) -> Result<T> {
    let packages = root_pkg.packages()?;
    let package_paths = make_deps_for_compiler(w, packages, build_config)?;

    debug!("Package paths {:#?}", package_paths);

    writeln!(
        w,
        "{} {}",
        "BUILDING".bold().green(),
        root_pkg.display_name()
    )?;

    let lint_level = build_config.lint_flag.get();
    let sui_mode = build_config.default_flavor == Some(Flavor::Sui);
    let flags = compiler_flags(build_config);

    let mut compiler = Compiler::from_package_paths(vfs_root, package_paths, vec![])
        .unwrap()
        .set_flags(flags);
    if sui_mode {
        let (filter_attr_name, filters) = sui_mode::linters::known_filters();
        compiler = compiler
            .add_custom_known_filters(filter_attr_name, filters)
            .add_visitors(sui_mode::linters::linter_visitors(lint_level))
    }
    let (filter_attr_name, filters) = linters::known_filters();
    compiler = compiler
        .add_custom_known_filters(filter_attr_name, filters)
        .add_visitors(linters::linter_visitors(lint_level));

    compiler_driver(compiler)
}

/// Save the compiled package to disk
fn save_to_disk(
    root_compiled_units: Vec<CompiledUnitWithSource>,
    compiled_package_info: CompiledPackageInfo,
    deps_compiled_units: Vec<(Symbol, CompiledUnitWithSource)>,
    compiled_docs: Option<Vec<(String, String)>>,
    root_package: Symbol,
    under_path: PathBuf,
) -> Result<OnDiskCompiledPackage> {
    check_filepaths_ok(&root_compiled_units, compiled_package_info.package_name)?;
    assert!(under_path.ends_with(CompiledPackageLayout::Root.path()));
    let on_disk_package = OnDiskCompiledPackage {
        root_path: under_path.join(root_package.to_string()),
        package: OnDiskPackage {
            compiled_package_info: compiled_package_info.clone(),
            dependencies: deps_compiled_units
                .iter()
                .map(|(package_name, _)| *package_name)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        },
    };

    // Clear out the build dir for this package so we don't keep artifacts from previous
    // compilations
    if on_disk_package.root_path.is_dir() {
        std::fs::remove_dir_all(&on_disk_package.root_path)?;
    }

    std::fs::create_dir_all(&on_disk_package.root_path)?;

    for compiled_unit in root_compiled_units {
        on_disk_package.save_compiled_unit(root_package, &compiled_unit)?;
        if compiled_package_info.build_flags.save_disassembly {
            on_disk_package.save_disassembly_to_disk(root_package, &compiled_unit)?;
        }
    }
    for (dep_name, compiled_unit) in deps_compiled_units {
        let dep_name: Symbol = dep_name.as_str().into();
        on_disk_package.save_compiled_unit(dep_name, &compiled_unit)?;
        if compiled_package_info.build_flags.save_disassembly {
            on_disk_package.save_disassembly_to_disk(dep_name, &compiled_unit)?;
        }
    }

    if let Some(docs) = compiled_docs {
        for (doc_filename, doc_contents) in docs {
            on_disk_package.save_under(
                CompiledPackageLayout::CompiledDocs
                    .path()
                    .join(doc_filename)
                    .with_extension("md"),
                doc_contents.clone().as_bytes(),
            )?;
        }
    }

    on_disk_package.save_under(
        CompiledPackageLayout::BuildInfo.path(),
        serde_yaml::to_string(&on_disk_package.package)?.as_bytes(),
    )?;

    Ok(on_disk_package)
}

// We take the (restrictive) view that all filesystems are case insensitive to maximize
// portability of packages.
fn check_filepaths_ok(
    root_compiled_units: &Vec<CompiledUnitWithSource>,
    package_name: Symbol,
) -> Result<()> {
    // A mapping of (lowercase_name => [info_for_each_occurence]
    let mut insensitive_mapping = BTreeMap::new();
    for compiled_unit in root_compiled_units {
        let name = compiled_unit.unit.name.as_str();
        let entry = insensitive_mapping
            .entry(name.to_lowercase())
            .or_insert_with(Vec::new);
        entry.push((
            name,
            compiled_unit.source_path.to_string_lossy().to_string(),
        ));
    }
    let errs = insensitive_mapping
        .into_iter()
        .filter_map(|(insensitive_name, occurence_infos)| {
            if occurence_infos.len() > 1 {
                let name_conflict_error_msg = occurence_infos
                    .into_iter()
                    .map(|(name, fpath)| format!("\tModule '{}' at path '{}'", name, fpath))
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(format!(
                    "The following modules and/or scripts would collide as '{}' on the file system:\n{}",
                    insensitive_name, name_conflict_error_msg
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if !errs.is_empty() {
        anyhow::bail!(
            "Module and/or script names found that would cause failures on case insensitive \
                file systems when compiling package '{}':\n{}\nPlease rename these scripts and/or modules to resolve these conflicts.",
            package_name,
            errs.join("\n"),
        )
    }
    Ok(())
}

/// Return a list of package paths for the transitive dependencies.
pub fn make_deps_for_compiler<W: Write, F: MoveFlavor>(
    w: &mut W,
    packages: Vec<PackageInfo<'_, F>>,
    build_config: &BuildConfig,
) -> anyhow::Result<Vec<PackagePaths>> {
    let mut package_paths: Vec<PackagePaths> = vec![];
    // let cwd = std::env::current_dir()?;
    for pkg in packages.into_iter() {
        let name: Symbol = pkg.display_name().into();

        if !pkg.is_root() {
            writeln!(w, "{} {name}", "INCLUDING DEPENDENCY".bold().green())?;
        }

        let addresses: BuildNamedAddresses = pkg.named_addresses()?.into();

        // TODO: better default handling for edition and flavor
        let config = PackageConfig {
            is_dependency: !pkg.is_root(),
            edition: Edition::from_str(pkg.edition())?,
            flavor: Flavor::from_str(pkg.flavor().unwrap_or("sui"))?,
            warning_filter: WarningFiltersBuilder::new_for_source(),
        };

        // TODO: improve/rework this? Renaming the root pkg to have a unique name for the compiler
        let safe_name = Symbol::from(pkg.id().clone());

        // let sources = get_sources(pkg.path(), build_config)?
        //     .iter()
        //     .map(|x| x.replace(cwd.to_str().unwrap(), ".").into())
        //     .collect();

        debug!("Package name {:?} -- Safe name {:?}", name, safe_name);
        debug!("Named address map {:#?}", addresses);
        let paths = PackagePaths {
            name: Some((safe_name, config)),
            // paths: sources,
            paths: get_sources(pkg.path(), build_config)?,
            named_address_map: addresses.inner,
        };

        package_paths.push(paths);
    }

    Ok(package_paths)
}

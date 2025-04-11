use clap::Args;
use move_cli::base;
use move_compiler::{editions::{Edition, Flavor}, shared::{NumericalAddress, PackageConfig, PackagePaths}};
use move_package::{source_package::layout::SourcePackageLayout, BuildConfig as MoveBuildConfig, LintFlag, ModelConfig};
use move_core_types::account_address::AccountAddress;
use move_prover::{run_boogie_gen, run_move_prover_with_model};
use tracing::log::LevelFilter;
use std::{collections::BTreeMap, path::{Path,PathBuf}};
use codespan_reporting::term::termcolor::Buffer;
use crate::llm_explain::explain_err;
use move_symbol_pool::Symbol;

impl From<BuildConfig> for MoveBuildConfig {
    fn from(config: BuildConfig) -> Self {
        Self {
            dev_mode: true,
            test_mode: false,
            verify_mode: true,
            json_errors: false,
            generate_docs: false,
            silence_warnings: true,
            warnings_are_errors: false,
            default_flavor: Some(Flavor::Sui),
            lint_flag: LintFlag::default(),
            install_dir: config.install_dir,
            force_recompilation: config.force_recompilation,
            lock_file: config.lock_file,
            fetch_deps_only: config.fetch_deps_only,
            skip_fetch_latest_git_deps: config.skip_fetch_latest_git_deps,
            default_edition: config.default_edition,
            deps_as_root: config.deps_as_root,
            additional_named_addresses: config.additional_named_addresses,
            save_disassembly: false,
            implicit_dependencies: BTreeMap::new(),
        }
    }
}

/// General prove options
#[derive(Args)]
#[clap(next_help_heading = "General Options")]
pub struct GeneralConfig {
    /// Set verification timeout in seconds (default: 3000)
    #[clap(name = "timeout", long, short = 't', global = true)]
    pub timeout: Option<usize>,

    /// Don't delete temporary files after verification
    #[clap(name = "keep-temp", long, short = 'k', global = true)]
    pub keep_temp: bool,

    /// Display detailed verification progress
    #[clap(name = "verbose", long, short = 'v', global = true)]
    pub verbose: bool,

    /// Explain the proving outputs via LLM 
    #[clap(name = "explain", long, global = true)]
    pub explain: bool,

    /// Display detailed verification progress
    #[clap(name = "use_array_theory", long = "use_array_theory", global = true)]
    pub use_array_theory: bool,

    /// Split verification into separate proof goals for each execution path
    #[clap(name = "split-paths", long, short = 's', global = true)]
    pub split_paths: Option<usize>,
}

#[derive(Args)]
#[clap(next_help_heading = "Build Options (subset of sui move build)")]
pub struct BuildConfig {
    /// Installation directory for compiled artifacts. Defaults to current directory.
    #[clap(long = "install-dir", global = true)]
    pub install_dir: Option<PathBuf>,

    /// Force recompilation of all packages
    #[clap(name = "force-recompilation", long = "force", global = true)]
    pub force_recompilation: bool,

    /// Optional location to save the lock file to, if package resolution succeeds.
    #[clap(skip)]
    pub lock_file: Option<PathBuf>,

    /// Only fetch dependency repos to MOVE_HOME
    #[clap(long = "fetch-deps-only", global = true)]
    pub fetch_deps_only: bool,

    /// Skip fetching latest git dependencies
    #[clap(long = "skip-fetch-latest-git-deps", global = true)]
    pub skip_fetch_latest_git_deps: bool,

    /// Default edition for move compilation, if not specified in the package's config
    #[clap(long = "default-move-edition", global = true)]
    pub default_edition: Option<Edition>,

    /// If set, dependency packages are treated as root packages. Notably, this will remove
    /// warning suppression in dependency packages.
    #[clap(long = "dependencies-are-root", global = true)]
    pub deps_as_root: bool,

    /// Additional named address mapping. Useful for tools in rust
    #[clap(skip)]
    pub additional_named_addresses: BTreeMap<String, AccountAddress>,
}


static MOVE_STDLIB_ADDRESS: AccountAddress = AccountAddress::from_suffix(0x1);
static SUI_FRAMEWORK_ADDRESS: AccountAddress  = AccountAddress::from_suffix(0x2);
static SUI_SYSTEM_ADDRESS: AccountAddress = AccountAddress::from_suffix(0x3);
static BRIDGE_ADDRESS: AccountAddress = AccountAddress::from_suffix(0xb);
static DEEPBOOK_ADDRESS: AccountAddress = AccountAddress::from_suffix(0xdee9);

pub async fn execute(
    path: Option<&Path>,
    general_config: GeneralConfig,
    build_config: BuildConfig,
    boogie_config: Option<String>,
) -> anyhow::Result<()> {
    let rerooted_path = base::reroot_path(path)?;
    let move_build_config = resolve_lock_file_path(
        build_config.into(), 
        Some(&rerooted_path),
    )?;

    let named_addresses: BTreeMap<Symbol, NumericalAddress>  = {
        let mapping = [(Symbol::from("std"), Symbol::from("0x1"))];
        let mut map: BTreeMap<Symbol, NumericalAddress> = mapping
            .iter()
            .map(|(name, addr)| (Symbol::from(name.to_string()), NumericalAddress::parse_str(addr).unwrap()))
            .collect();
        map.insert(
            "sui".into(),
            NumericalAddress::new(
                SUI_FRAMEWORK_ADDRESS.into_bytes(),
                move_compiler::shared::NumberFormat::Hex,
            ),
        );
        map.insert(
            "sui_system".into(),
            NumericalAddress::new(
                SUI_SYSTEM_ADDRESS.into_bytes(),
                move_compiler::shared::NumberFormat::Hex,
            ),
        );
        map.insert(
            "deepbook".into(),
            NumericalAddress::new(
                DEEPBOOK_ADDRESS.into_bytes(),
                move_compiler::shared::NumberFormat::Hex,
            ),
        );
        map.insert(
            "bridge".into(),
            NumericalAddress::new(
                BRIDGE_ADDRESS.into_bytes(),
                move_compiler::shared::NumberFormat::Hex,
            ),
        );
        map
    };

    let sui_files: &Path = Path::new("/Users/mandriiss/work/sui/crates/sui-framework");
    let sui_system_sources = {
        let mut buf = sui_files.to_path_buf();
        buf.extend(["packages", "sui-system", "sources"]);
        buf.to_string_lossy().to_string()
    };
    let sui_sources = {
        let mut buf = sui_files.to_path_buf();
        buf.extend(["packages", "sui-framework", "sources"]);
        buf.to_string_lossy().to_string()
    };
    let sui_deps = {
        let mut buf = sui_files.to_path_buf();
        buf.extend(["packages", "move-stdlib", "sources"]);
        buf.to_string_lossy().to_string()
    };
    let deepbook_sources = {
        let mut buf = sui_files.to_path_buf();
        buf.extend(["packages", "deepbook", "sources"]);
        buf.to_string_lossy().to_string()
    };
    let config = PackageConfig {
        edition: Edition::E2024_BETA,
        flavor: Flavor::Sui,
        ..Default::default()
    };
    let bridge_sources = {
        let mut buf = sui_files.to_path_buf();
        buf.extend(["packages", "bridge", "sources"]);
        buf.to_string_lossy().to_string()
    };

    let precompiles: PackagePaths<Symbol, Symbol> = PackagePaths {
        name: Some(("sui-framework".into(), config)),
        paths: vec![
            sui_system_sources.into(),
            sui_sources.into(),
            sui_deps.into(),
            deepbook_sources.into(),
            bridge_sources.into(),
        ],
        named_address_map: named_addresses.clone(),
    };

    let model = move_build_config.move_model_for_package_legacy(
        &rerooted_path,
        ModelConfig {
            all_files_as_targets: false,
            target_filter: None,
        },
        Some(precompiles),
    )?;
    let mut options = move_prover::cli::Options::default();
    // don't spawn async tasks when running Boogie--causes a crash if we do
    options.backend.sequential_task = true;
    options.backend.use_array_theory = general_config.use_array_theory;
    options.backend.keep_artifacts = general_config.keep_temp;
    options.backend.vc_timeout = general_config.timeout.unwrap_or(3000);
    options.backend.path_split = general_config.split_paths;
    options.verbosity_level = if general_config.verbose { LevelFilter::Trace } else { LevelFilter::Info };
    options.backend.string_options = boogie_config;
    
    if general_config.explain {
        let mut error_writer = Buffer::no_color();
        match run_move_prover_with_model(&model, &mut error_writer, options, None) {
            Ok(_) => {
                let output = String::from_utf8_lossy(&error_writer.into_inner()).to_string();
                println!("Output: {}", output);
            }
            Err(e) => {
                let output = String::from_utf8_lossy(&error_writer.into_inner()).to_string();
                explain_err(&output, &e).await;
            }
        }
    } else {
       run_boogie_gen(&model, options)?;
    }

    Ok(())
}

fn resolve_lock_file_path(
    mut build_config: MoveBuildConfig,
    package_path: Option<&Path>,
) -> Result<MoveBuildConfig, anyhow::Error> {
    if build_config.lock_file.is_none() {
        let package_root = base::reroot_path(package_path)?;
        let lock_file_path = package_root.join(SourcePackageLayout::Lock.path());
        build_config.lock_file = Some(lock_file_path);
    }
    Ok(build_config)
}

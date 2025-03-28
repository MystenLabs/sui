use clap::*;
use move_binary_format::{CompiledModule, file_format::FunctionDefinitionIndex};
use move_bytecode_verifier::{
    ability_cache::AbilityCache, code_unit_verifier,
    verifier::verify_module_with_config_metered_up_to_code_units,
    verify_module_with_config_metered,
};
use move_bytecode_verifier_meter::{Meter, Scope, bound::BoundMeter};
use move_command_line_common::files::{MOVE_COMPILED_EXTENSION, extension_equals, find_filenames};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use move_vm_config::verifier::VerifierConfig;
use std::collections::{BTreeMap, HashMap};

use crate::data::*;

#[derive(Debug, Parser)]
#[clap(
    name = "move-bytecode-verifier-analyzer",
    about = "Run the bytecode verifier on specified bytecode packages",
    author,
    version
)]
pub struct Options {
    /// The directories (or direct files) containing Move bytecode modules
    #[clap(
        name = "PATH_TO_BYTECODE",
        num_args(1..),
        action = clap::ArgAction::Append,
    )]
    pub paths: Vec<String>,

    /// Filter specific packages, modules, or functions to analyze
    #[clap(
        name = "FILTER",
        short = 'f',
        long = "filter",
        help = "Filter for specific packages, modules, or functions, e.g. '0x1' or '0x42::m::foo'. NOTE: Ticks might not be accurate if filtering is applied"
    )]
    pub filter: Option<String>,

    #[clap(
        name = "SHOW_TICKS",
        short = 't',
        long = "show-ticks",
        help = "Show the number of ticks used for each package, module, and function"
    )]
    pub show_ticks: bool,

    #[clap(
        name = "VERBOSE",
        short = 'v',
        long = "verbose",
        help = "Print while analyzing each module"
    )]
    pub verbose: bool,
}

enum Filter {
    None,
    Address(AccountAddress),
    Identifier(String),
    AddressModule {
        address: AccountAddress,
        module: String,
    },
    ModuleFunction {
        module: String,
        function: String,
    },
    Full {
        address: AccountAddress,
        module: String,
        function: String,
    },
}

pub fn run() -> anyhow::Result<()> {
    let Options {
        paths,
        show_ticks,
        filter,
        verbose,
    } = Options::parse();
    assert!(!paths.is_empty(), "No paths provided");
    let filter = parse_filter(filter)?;
    let data = analyze_files(verbose, &paths, &filter)?;
    println!("{}", data.display(show_ticks));
    Ok(())
}

fn analyze_files(verbose: bool, paths: &[String], filter: &Filter) -> anyhow::Result<Data> {
    let files = find_filenames(paths, |p| extension_equals(p, MOVE_COMPILED_EXTENSION))?;
    let mut package_data: BTreeMap<AccountAddress, PackageData> = BTreeMap::new();
    let mut package_meters: BTreeMap<AccountAddress, BoundMeter> = BTreeMap::new();
    let mut deserialized_modules = BTreeMap::new();
    for file in files {
        if verbose {
            println!("READING: {}", file);
        }
        let bytes = std::fs::read(&file)?;
        let module = CompiledModule::deserialize_with_defaults(&bytes)?;
        let self_id = module.self_id();
        deserialized_modules.insert(self_id, module);
    }

    for (self_id, module) in deserialized_modules {
        let address = *self_id.address();
        let name = self_id.name().to_owned();
        if !(filter.visit_package(&address) && filter.visit_module(&name)) {
            if verbose {
                println!("SKIPPING: {}::{}", address, name);
            }
            continue;
        }
        if verbose {
            println!("ANALYZING: {}::{}", address, name);
        }
        let package_meter = package_meters.entry(address).or_insert_with(|| {
            let mut meter = new_meter();
            meter.enter_scope("package", Scope::Package);
            meter
        });
        let result = analyze_module(verbose, &module, package_meter, filter)?;
        let module_data = ModuleData {
            name: name.clone(),
            module,
            result,
        };
        package_data
            .entry(address)
            .or_default()
            .modules
            .insert(name, module_data);
    }
    Ok(Data { package_data })
}

fn analyze_module(
    verbose: bool,
    module: &CompiledModule,
    package_meter: &mut BoundMeter,
    filter: &Filter,
) -> anyhow::Result<ModuleVerificationResult> {
    let mut result = analyze_module_(verbose, module, filter)?;
    // everything passed so rerun to ensure accurate ticks at the package level
    package_meter.enter_scope(module.name().as_str(), Scope::Module);
    // ignore result since we ran it already
    let now = std::time::Instant::now();
    let _ = verify_module_with_config_metered(&config(), module, package_meter);
    result.time = now.elapsed().as_micros();
    result.ticks = package_meter.get_usage(Scope::Module);
    package_meter
        .transfer(Scope::Module, Scope::Package, 1.0)
        .unwrap();
    Ok(result)
}

fn analyze_module_(
    verbose: bool,
    module: &CompiledModule,
    filter: &Filter,
) -> anyhow::Result<ModuleVerificationResult> {
    let config = config();
    let ability_cache = &mut AbilityCache::new(module);
    let mut meter = new_meter();
    if let Err(error) = verify_module_with_config_metered_up_to_code_units(
        &config,
        module,
        ability_cache,
        &mut meter,
    ) {
        if verbose {
            println!("FAILED {}::{}: {}", module.address(), module.name(), error);
        }
        return Ok(ModuleVerificationResult {
            ticks: 0, // set above
            time: 0,  // set above
            function_ticks: BTreeMap::new(),
            status: ModuleVerificationStatus::Failed(error),
        });
    }

    let mut name_def_map = HashMap::new();
    for (idx, func_def) in module.function_defs().iter().enumerate() {
        let fh = module.function_handle_at(func_def.function);
        name_def_map.insert(fh.name, FunctionDefinitionIndex(idx as u16));
    }
    let mut function_ticks = BTreeMap::new();
    let mut functions_failed = BTreeMap::new();
    for (idx, function_definition) in module.function_defs().iter().enumerate() {
        let fh = module.function_handle_at(function_definition.function);
        let name = module.identifier_at(fh.name).as_str();
        if !filter.visit_function(name) {
            if verbose {
                println!(
                    "SKIPPING: {}::{}::{}",
                    module.address(),
                    module.name(),
                    name
                );
            }
            continue;
        }
        if verbose {
            println!(
                "ANALYZING: {}::{}::{}",
                module.address(),
                module.name(),
                name
            );
        }
        let now = std::time::Instant::now();
        if let Err(e) = code_unit_verifier::verify_function(
            &config,
            FunctionDefinitionIndex(idx as u16),
            function_definition,
            module,
            ability_cache,
            &name_def_map,
            &mut meter,
        ) {
            if verbose {
                println!(
                    "FAILED {}::{}::{}: {}",
                    module.address(),
                    module.name(),
                    name,
                    e
                );
            }
            meter.transfer(Scope::Function, Scope::Module, 1.0).unwrap();
            functions_failed.insert(
                name.to_owned(),
                e.finish(move_binary_format::errors::Location::Module(
                    module.self_id(),
                )),
            );
        }
        let time = now.elapsed().as_micros();
        let ticks = meter.get_usage(Scope::Function);
        function_ticks.insert(name.to_owned(), (ticks, time));
    }
    if !functions_failed.is_empty() {
        return Ok(ModuleVerificationResult {
            ticks: 0, // set above
            time: 0,  // set above
            function_ticks,
            status: ModuleVerificationStatus::FunctionsFailed(functions_failed),
        });
    }

    Ok(ModuleVerificationResult {
        ticks: 0, // set above
        time: 0,  // set above
        function_ticks,
        status: ModuleVerificationStatus::Verified,
    })
}

fn new_meter() -> BoundMeter {
    BoundMeter::new(move_vm_config::verifier::MeterConfig {
        max_per_pkg_meter_units: Some(u128::MAX),
        max_per_mod_meter_units: Some(u128::MAX),
        max_per_fun_meter_units: Some(u128::MAX),
    })
}

fn config() -> VerifierConfig {
    VerifierConfig::default()
}

fn parse_filter(s: Option<String>) -> anyhow::Result<Filter> {
    let Some(s) = s else {
        return Ok(Filter::None);
    };
    let items = s.split("::").collect::<Vec<_>>();
    anyhow::ensure!(
        items.len() <= 3,
        "Filter must be in the form of 'address' or 'identifier' or 'address::module' or 'module::function' or 'address::module::function'"
    );
    match items.as_slice() {
        [s1] => {
            if let Ok(addr) = AccountAddress::from_hex_literal(s1) {
                Ok(Filter::Address(addr))
            } else {
                Ok(Filter::Identifier(s1.to_string()))
            }
        }
        [n1, n2] => {
            if let Ok(addr) = AccountAddress::from_hex_literal(n1) {
                Ok(Filter::AddressModule {
                    address: addr,
                    module: n2.to_string(),
                })
            } else {
                Ok(Filter::ModuleFunction {
                    module: n1.to_string(),
                    function: n2.to_string(),
                })
            }
        }
        [n1, n2, n3] => {
            let addr = AccountAddress::from_hex_literal(n1)?;
            Ok(Filter::Full {
                address: addr,
                module: n2.to_string(),
                function: n3.to_string(),
            })
        }
        _ => unreachable!("Filter parsing should not reach here"),
    }
}

impl Filter {
    fn visit_package(&self, a: &AccountAddress) -> bool {
        match self {
            Filter::None | Filter::Identifier(_) | Filter::ModuleFunction { .. } => true,
            Filter::Address(address)
            | Filter::Full { address, .. }
            | Filter::AddressModule { address, .. } => address == a,
        }
    }

    fn visit_module(&self, m: &Identifier) -> bool {
        match self {
            Filter::None | Filter::Address(_) => true,
            Filter::Identifier(module)
            | Filter::ModuleFunction { module, .. }
            | Filter::Full { module, .. }
            | Filter::AddressModule { module, .. } => m.as_str() == module,
        }
    }

    fn visit_function(&self, f: &str) -> bool {
        match self {
            Filter::None
            | Filter::Address(_)
            | Filter::Identifier(_)
            | Filter::AddressModule { .. } => true,
            Filter::ModuleFunction { function, .. } | Filter::Full { function, .. } => {
                f == function
            }
        }
    }
}

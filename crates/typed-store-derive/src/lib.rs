// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashSet};

use itertools::Itertools;
use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use syn::Type::{self};
use syn::{
    parse_macro_input, AngleBracketedGenericArguments, Attribute, Generics, ItemStruct, Lit, Meta,
    PathArguments,
};

// This is used as default when none is specified
const DEFAULT_DB_OPTIONS_CUSTOM_FN: &str = "typed_store::rocks::default_db_options";
// Custom function which returns the option and overrides the defaults for this table
const DB_OPTIONS_CUSTOM_FUNCTION: &str = "default_options_override_fn";
// Use a different name for the column than the identifier
const DB_OPTIONS_RENAME: &str = "rename";
// Deprecate a column family
const DB_OPTIONS_DEPRECATE: &str = "deprecated";

/// Options can either be simplified form or
enum GeneralTableOptions {
    OverrideFunction(String),
}

impl Default for GeneralTableOptions {
    fn default() -> Self {
        Self::OverrideFunction(DEFAULT_DB_OPTIONS_CUSTOM_FN.to_owned())
    }
}

// Extracts the field names, field types, inner types (K,V in {map_type_name}<K, V>), and the options attrs
fn extract_struct_info(
    input: ItemStruct,
    allowed_map_type_names: HashSet<String>,
) -> ExtractedStructInfo {
    // There must only be one map type used for all entries
    let allowed_strs: Vec<_> = allowed_map_type_names
        .iter()
        .map(|s| format!("{s}<K, V>"))
        .collect();
    let allowed_strs = allowed_strs.join(" or ");
    let mut deprecated_cfs = vec![];

    let info = input.fields.iter().map(|f| {
        let attrs: BTreeMap<_, _> = f
            .attrs
            .iter()
            .filter(|a| {
                a.path.is_ident(DB_OPTIONS_CUSTOM_FUNCTION)
                    || a.path.is_ident(DB_OPTIONS_RENAME)
                    || a.path.is_ident(DB_OPTIONS_DEPRECATE)
            })
            .map(|a| (a.path.get_ident().unwrap().to_string(), a))
            .collect();

        let options = if let Some(options) = attrs.get(DB_OPTIONS_CUSTOM_FUNCTION) {
            GeneralTableOptions::OverrideFunction(get_options_override_function(options).unwrap())
        } else {
            GeneralTableOptions::default()
        };

        let ty = &f.ty;
        if let Type::Path(p) = ty {
            let type_info = &p.path.segments.first().unwrap();
            let inner_type =
                if let PathArguments::AngleBracketed(angle_bracket_type) = &type_info.arguments {
                    angle_bracket_type.clone()
                } else {
                    panic!("All struct members must be of type {allowed_strs}");
                };

            let type_str = format!("{}", &type_info.ident);
            // Rough way to check that this is map_type_name
            if allowed_map_type_names.contains(&type_str) {
                let field_name = f.ident.as_ref().unwrap().clone();
                let cf_name = if let Some(rename) = attrs.get(DB_OPTIONS_RENAME) {
                    match rename.parse_meta().expect("Cannot parse meta of attribute") {
                        Meta::NameValue(val) => {
                            if let Lit::Str(s) = val.lit {
                                // convert to ident
                                s.parse().expect("Rename value must be identifier")
                            } else {
                                panic!("Expected string value for rename")
                            }
                        }
                        _ => panic!("Expected string value for rename"),
                    }
                } else {
                    field_name.clone()
                };
                if attrs.get(DB_OPTIONS_DEPRECATE).is_some() {
                    deprecated_cfs.push(field_name.clone());
                }

                return ((field_name, cf_name, type_str), (inner_type, options));
            } else {
                panic!("All struct members must be of type {allowed_strs}");
            }
        }
        panic!("All struct members must be of type {allowed_strs}");
    });

    let (field_info, inner_types_with_opts): (Vec<_>, Vec<_>) = info.unzip();
    let (field_names, cf_names, simple_field_type_names): (Vec<_>, Vec<_>, Vec<_>) =
        field_info.into_iter().multiunzip();

    // Check for homogeneous types
    if let Some(first) = simple_field_type_names.first() {
        simple_field_type_names.iter().for_each(|q| {
            if q != first {
                panic!("All struct members must be of same type");
            }
        })
    } else {
        panic!("Cannot derive on empty struct");
    };

    let (inner_types, options): (Vec<_>, Vec<_>) = inner_types_with_opts.into_iter().unzip();

    ExtractedStructInfo {
        field_names,
        cf_names,
        inner_types,
        derived_table_options: options,
        simple_field_type_name_str: simple_field_type_names.first().unwrap().clone(),
        deprecated_cfs,
    }
}

/// Extracts the table options override function
/// The function must take no args and return Options
fn get_options_override_function(attr: &Attribute) -> syn::Result<String> {
    let meta = attr.parse_meta()?;

    let val = match meta.clone() {
        Meta::NameValue(val) => val,
        _ => {
            return Err(syn::Error::new_spanned(
                meta,
                format!("Expected function name in format `#[{DB_OPTIONS_CUSTOM_FUNCTION} = {{function_name}}]`"),
            ))
        }
    };

    if !val.path.is_ident(DB_OPTIONS_CUSTOM_FUNCTION) {
        return Err(syn::Error::new_spanned(
            meta,
            format!("Expected function name in format `#[{DB_OPTIONS_CUSTOM_FUNCTION} = {{function_name}}]`"),
        ));
    }

    let fn_name = match val.lit {
        Lit::Str(fn_name) => fn_name,
        _ => return Err(syn::Error::new_spanned(
            meta,
            format!("Expected function name in format `#[{DB_OPTIONS_CUSTOM_FUNCTION} = {{function_name}}]`"),
        ))
    };
    Ok(fn_name.value())
}

fn extract_generics_names(generics: &Generics) -> Vec<Ident> {
    generics
        .params
        .iter()
        .map(|g| match g {
            syn::GenericParam::Type(t) => t.ident.clone(),
            _ => panic!("Unsupported generic type"),
        })
        .collect()
}

struct ExtractedStructInfo {
    field_names: Vec<Ident>,
    cf_names: Vec<Ident>,
    inner_types: Vec<AngleBracketedGenericArguments>,
    derived_table_options: Vec<GeneralTableOptions>,
    simple_field_type_name_str: String,
    deprecated_cfs: Vec<Ident>,
}

#[proc_macro_derive(DBMapUtils, attributes(default_options_override_fn, rename))]
pub fn derive_dbmap_utils_general(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    let name = &input.ident;
    let generics = &input.generics;
    let generics_names = extract_generics_names(generics);

    let allowed_types_with_post_process_fn: BTreeMap<_, _> =
        [("SallyColumn", ""), ("DBMap", "")].into_iter().collect();
    let allowed_strs = allowed_types_with_post_process_fn
        .keys()
        .map(|s| s.to_string())
        .collect();

    // TODO: use `parse_quote` over `parse()`
    let ExtractedStructInfo {
        field_names,
        cf_names,
        inner_types,
        derived_table_options,
        simple_field_type_name_str,
        deprecated_cfs,
    } = extract_struct_info(input.clone(), allowed_strs);

    let (key_names, value_names): (Vec<_>, Vec<_>) = inner_types
        .iter()
        .map(|q| (q.args.first().unwrap(), q.args.last().unwrap()))
        .unzip();

    // This is the actual name of the type which was found
    let post_process_fn_str = allowed_types_with_post_process_fn
        .get(&simple_field_type_name_str.as_str())
        .unwrap();
    let post_process_fn: proc_macro2::TokenStream = post_process_fn_str.parse().unwrap();

    let default_options_override_fn_names: Vec<proc_macro2::TokenStream> = derived_table_options
        .iter()
        .map(|q| {
            let GeneralTableOptions::OverrideFunction(fn_name) = q;
            fn_name.parse().unwrap()
        })
        .collect();

    let generics_bounds =
        "std::fmt::Debug + serde::Serialize + for<'de> serde::de::Deserialize<'de>";
    let generics_bounds_token: proc_macro2::TokenStream = generics_bounds.parse().unwrap();

    let config_struct_name_str = format!("{name}Configurator");
    let config_struct_name: proc_macro2::TokenStream = config_struct_name_str.parse().unwrap();

    let intermediate_db_map_struct_name_str = format!("{name}IntermediateDBMapStructPrimary");
    let intermediate_db_map_struct_name: proc_macro2::TokenStream =
        intermediate_db_map_struct_name_str.parse().unwrap();

    let secondary_db_map_struct_name_str = format!("{name}ReadOnly");
    let secondary_db_map_struct_name: proc_macro2::TokenStream =
        secondary_db_map_struct_name_str.parse().unwrap();

    TokenStream::from(quote! {

        // <----------- This section generates the configurator struct -------------->

        /// Create config structs for configuring DBMap tables
        pub struct #config_struct_name {
            #(
                pub #field_names : typed_store::rocks::DBOptions,
            )*
        }

        impl #config_struct_name {
            /// Initialize to defaults
            pub fn init() -> Self {
                Self {
                    #(
                        #field_names : typed_store::rocks::default_db_options(),
                    )*
                }
            }

            /// Build a config
            pub fn build(&self) -> typed_store::rocks::DBMapTableConfigMap {
                typed_store::rocks::DBMapTableConfigMap::new([
                    #(
                        (stringify!(#field_names).to_owned(), self.#field_names.clone()),
                    )*
                ].into_iter().collect())
            }
        }

        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > #name #generics {

                pub fn configurator() -> #config_struct_name {
                    #config_struct_name::init()
                }
        }

        // <----------- This section generates the core open logic for opening DBMaps -------------->

        /// Create an intermediate struct used to open the DBMap tables in primary mode
        /// This is only used internally
        struct #intermediate_db_map_struct_name #generics {
                #(
                    pub #field_names : DBMap #inner_types,
                )*
        }


        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > #intermediate_db_map_struct_name #generics {
            /// Opens a set of tables in read-write mode
            /// If as_secondary_with_path is set, the DB is opened in read only mode with the path specified
            pub fn open_tables_impl(
                path: std::path::PathBuf,
                as_secondary_with_path: Option<std::path::PathBuf>,
                is_transaction: bool,
                metric_conf: typed_store::rocks::MetricConf,
                global_db_options_override: Option<typed_store::rocksdb::Options>,
                tables_db_options_override: Option<typed_store::rocks::DBMapTableConfigMap>,
                remove_deprecated_tables: bool,
            ) -> Self {
                let path = &path;
                let (db, rwopt_cfs) = {
                    let opt_cfs = match tables_db_options_override {
                        None => [
                            #(
                                (stringify!(#cf_names).to_owned(), #default_options_override_fn_names()),
                            )*
                        ],
                        Some(o) => [
                            #(
                                (stringify!(#cf_names).to_owned(), o.to_map().get(stringify!(#cf_names)).unwrap().clone()),
                            )*
                        ]
                    };
                    // Safe to call unwrap because we will have at least one field_name entry in the struct
                    let rwopt_cfs: std::collections::HashMap<String, typed_store::rocks::ReadWriteOptions> = opt_cfs.iter().map(|q| (q.0.as_str().to_string(), q.1.rw_options.clone())).collect();
                    let opt_cfs: Vec<_> = opt_cfs.iter().map(|q| (q.0.as_str(), q.1.options.clone())).collect();
                    let db = match (as_secondary_with_path.clone(), is_transaction) {
                        (Some(p), _) => typed_store::rocks::open_cf_opts_secondary(path, Some(&p), global_db_options_override, metric_conf, &opt_cfs),
                        (_, true) => typed_store::rocks::open_cf_opts_transactional(path, global_db_options_override, metric_conf, &opt_cfs),
                        _ => typed_store::rocks::open_cf_opts(path, global_db_options_override, metric_conf, &opt_cfs)
                    };
                    db.map(|d| (d, rwopt_cfs))
                }.expect(&format!("Cannot open DB at {:?}", path));
                let deprecated_tables = vec![#(stringify!(#deprecated_cfs),)*];
                let (
                        #(
                            #field_names
                        ),*
                ) = (#(
                        DBMap::#inner_types::reopen(&db, Some(stringify!(#cf_names)), rwopt_cfs.get(stringify!(#cf_names)).unwrap_or(&typed_store::rocks::ReadWriteOptions::default()), remove_deprecated_tables && deprecated_tables.contains(&stringify!(#cf_names))).expect(&format!("Cannot open {} CF.", stringify!(#cf_names))[..])
                    ),*);

                if as_secondary_with_path.is_none() && remove_deprecated_tables {
                    #(
                        db.drop_cf(stringify!(#deprecated_cfs)).expect("failed to drop a deprecated cf");
                    )*
                }
                Self {
                    #(
                        #field_names,
                    )*
                }
            }
        }


        // <----------- This section generates the read-write open logic and other common utils -------------->

        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > #name #generics {
            /// Opens a set of tables in read-write mode
            /// Only one process is allowed to do this at a time
            /// `global_db_options_override` apply to the whole DB
            /// `tables_db_options_override` apply to each table. If `None`, the attributes from `default_options_override_fn` are used if any
            #[allow(unused_parens)]
            pub fn open_tables_read_write(
                path: std::path::PathBuf,
                metric_conf: typed_store::rocks::MetricConf,
                global_db_options_override: Option<typed_store::rocksdb::Options>,
                tables_db_options_override: Option<typed_store::rocks::DBMapTableConfigMap>
            ) -> Self {
                let inner = #intermediate_db_map_struct_name::open_tables_impl(path, None, false, metric_conf, global_db_options_override, tables_db_options_override, false);
                Self {
                    #(
                        #field_names: #post_process_fn(inner.#field_names),
                    )*
                }
            }

            #[allow(unused_parens)]
            pub fn open_tables_read_write_with_deprecation_option(
                path: std::path::PathBuf,
                metric_conf: typed_store::rocks::MetricConf,
                global_db_options_override: Option<typed_store::rocksdb::Options>,
                tables_db_options_override: Option<typed_store::rocks::DBMapTableConfigMap>,
                remove_deprecated_tables: bool,
            ) -> Self {
                let inner = #intermediate_db_map_struct_name::open_tables_impl(path, None, false, metric_conf, global_db_options_override, tables_db_options_override, remove_deprecated_tables);
                Self {
                    #(
                        #field_names: #post_process_fn(inner.#field_names),
                    )*
                }
            }

            /// Opens a set of tables in transactional read-write mode
            /// Only one process is allowed to do this at a time
            /// `global_db_options_override` apply to the whole DB
            /// `tables_db_options_override` apply to each table. If `None`, the attributes from `default_options_override_fn` are used if any
            #[allow(unused_parens)]
            pub fn open_tables_transactional(
                path: std::path::PathBuf,
                metric_conf: typed_store::rocks::MetricConf,
                global_db_options_override: Option<typed_store::rocksdb::Options>,
                tables_db_options_override: Option<typed_store::rocks::DBMapTableConfigMap>
            ) -> Self {
                let inner = #intermediate_db_map_struct_name::open_tables_impl(path, None, true, metric_conf, global_db_options_override, tables_db_options_override, false);
                Self {
                    #(
                        #field_names: #post_process_fn(inner.#field_names),
                    )*
                }
            }

            /// Returns a list of the tables name and type pairs
            pub fn describe_tables() -> std::collections::BTreeMap<String, (String, String)> {
                vec![#(
                    (stringify!(#field_names).to_owned(), (stringify!(#key_names).to_owned(), stringify!(#value_names).to_owned())),
                )*].into_iter().collect()
            }

            /// This opens the DB in read only mode and returns a struct which exposes debug features
            pub fn get_read_only_handle (
                primary_path: std::path::PathBuf,
                with_secondary_path: Option<std::path::PathBuf>,
                global_db_options_override: Option<typed_store::rocksdb::Options>,
                metric_conf: typed_store::rocks::MetricConf,
                ) -> #secondary_db_map_struct_name #generics {
                #secondary_db_map_struct_name::open_tables_read_only(primary_path, with_secondary_path, metric_conf, global_db_options_override)
            }
        }


        // <----------- This section generates the features that use read-only open logic -------------->
        /// Create an intermediate struct used to open the DBMap tables in secondary mode
        /// This is only used internally
        pub struct #secondary_db_map_struct_name #generics {
            #(
                pub #field_names : DBMap #inner_types,
            )*
        }

        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > #secondary_db_map_struct_name #generics {
            /// Open in read only mode. No limitation on number of processes to do this
            pub fn open_tables_read_only(
                primary_path: std::path::PathBuf,
                with_secondary_path: Option<std::path::PathBuf>,
                metric_conf: typed_store::rocks::MetricConf,
                global_db_options_override: Option<typed_store::rocksdb::Options>,
            ) -> Self {
                let inner = match with_secondary_path {
                    Some(q) => #intermediate_db_map_struct_name::open_tables_impl(primary_path, Some(q), false, metric_conf, global_db_options_override, None, false),
                    None => {
                        let p: std::path::PathBuf = tempfile::tempdir()
                        .expect("Failed to open temporary directory")
                        .into_path();
                        #intermediate_db_map_struct_name::open_tables_impl(primary_path, Some(p), false, metric_conf, global_db_options_override, None, false)
                    }
                };
                Self {
                    #(
                        #field_names: inner.#field_names,
                    )*
                }
            }

            fn cf_name_to_table_name(cf_name: &str) -> eyre::Result<&'static str> {
                Ok(match cf_name {
                    #(
                        stringify!(#cf_names) => stringify!(#field_names),
                    )*
                    _ => eyre::bail!("No such cf name: {}", cf_name),
                })
            }

            /// Dump all key-value pairs in the page at the given table name
            /// Tables must be opened in read only mode using `open_tables_read_only`
            pub fn dump(&self, cf_name: &str, page_size: u16, page_number: usize) -> eyre::Result<std::collections::BTreeMap<String, String>> {
                let table_name = Self::cf_name_to_table_name(cf_name)?;

                Ok(match table_name {
                    #(
                        stringify!(#field_names) => {
                            typed_store::traits::Map::try_catch_up_with_primary(&self.#field_names)?;
                            typed_store::traits::Map::unbounded_iter(&self.#field_names)
                                .skip((page_number * (page_size) as usize))
                                .take(page_size as usize)
                                .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                                .collect::<std::collections::BTreeMap<_, _>>()
                        }
                    )*

                    _ => eyre::bail!("No such table name: {}", table_name),
                })
            }

            /// Get key value sizes from the db
            /// Tables must be opened in read only mode using `open_tables_read_only`
            pub fn table_summary(&self, table_name: &str) -> eyre::Result<typed_store::traits::TableSummary> {
                let mut count = 0;
                let mut key_bytes = 0;
                let mut value_bytes = 0;
                match table_name {
                    #(
                        stringify!(#field_names) => {
                            typed_store::traits::Map::try_catch_up_with_primary(&self.#field_names)?;
                            self.#field_names.table_summary()
                        }
                    )*

                    _ => eyre::bail!("No such table name: {}", table_name),
                }
            }

            /// Count the keys in this table
            /// Tables must be opened in read only mode using `open_tables_read_only`
            pub fn count_keys(&self, table_name: &str) -> eyre::Result<usize> {
                Ok(match table_name {
                    #(
                        stringify!(#field_names) => {
                            typed_store::traits::Map::try_catch_up_with_primary(&self.#field_names)?;
                            typed_store::traits::Map::unbounded_iter(&self.#field_names).count()
                        }
                    )*

                    _ => eyre::bail!("No such table name: {}", table_name),
                })
            }

            pub fn describe_tables() -> std::collections::BTreeMap<String, (String, String)> {
                vec![#(
                    (stringify!(#field_names).to_owned(), (stringify!(#key_names).to_owned(), stringify!(#value_names).to_owned())),
                )*].into_iter().collect()
            }

            /// Try catch up with primary for all tables. This can be a slow operation
            /// Tables must be opened in read only mode using `open_tables_read_only`
            pub fn try_catch_up_with_primary_all(&self) -> eyre::Result<()> {
                #(
                    typed_store::traits::Map::try_catch_up_with_primary(&self.#field_names)?;
                )*
                Ok(())
            }
        }

        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > TypedStoreDebug for #secondary_db_map_struct_name #generics {
                fn dump_table(
                    &self,
                    table_name: String,
                    page_size: u16,
                    page_number: usize,
                ) -> eyre::Result<std::collections::BTreeMap<String, String>> {
                    self.dump(table_name.as_str(), page_size, page_number)
                }

                fn primary_db_name(&self) -> String {
                    stringify!(#name).to_owned()
                }

                fn describe_all_tables(&self) -> std::collections::BTreeMap<String, (String, String)> {
                    Self::describe_tables()
                }

                fn count_table_keys(&self, table_name: String) -> eyre::Result<usize> {
                    self.count_keys(table_name.as_str())
                }

                fn table_summary(&self, table_name: String) -> eyre::Result<TableSummary> {
                    self.table_summary(table_name.as_str())
                }


        }

    })
}

#[proc_macro_derive(SallyDB, attributes(default_options_override_fn))]
pub fn derive_sallydb_general(input: TokenStream) -> TokenStream {
    //log_syntax!("here");
    let input = parse_macro_input!(input as ItemStruct);
    let name = &input.ident;
    let generics = &input.generics;
    let generics_names = extract_generics_names(generics);

    let allowed_types_with_post_process_fn: BTreeMap<_, _> =
        [("SallyColumn", "")].into_iter().collect();
    let allowed_strs = allowed_types_with_post_process_fn
        .keys()
        .map(|s| s.to_string())
        .collect();

    // TODO: use `parse_quote` over `parse()`
    // TODO: Eventually this should return a Vec<Vec<GeneralTableOptions>> to capture default table options for each column type i.e. RockDB, TestDB, etc
    let ExtractedStructInfo {
        field_names,
        inner_types,
        derived_table_options,
        simple_field_type_name_str,
        ..
    } = extract_struct_info(input.clone(), allowed_strs);

    let (key_names, value_names): (Vec<_>, Vec<_>) = inner_types
        .iter()
        .map(|q| (q.args.first().unwrap(), q.args.last().unwrap()))
        .unzip();

    // This is the actual name of the type which was found
    let post_process_fn_str = allowed_types_with_post_process_fn
        .get(&simple_field_type_name_str.as_str())
        .unwrap();
    let post_process_fn: proc_macro2::TokenStream = post_process_fn_str.parse().unwrap();

    let default_options_override_fn_names: Vec<proc_macro2::TokenStream> = derived_table_options
        .iter()
        .map(|q| {
            let GeneralTableOptions::OverrideFunction(fn_name) = q;
            fn_name.parse().unwrap()
        })
        .collect();

    let generics_bounds =
        "std::fmt::Debug + serde::Serialize + for<'de> serde::de::Deserialize<'de>";
    let generics_bounds_token: proc_macro2::TokenStream = generics_bounds.parse().unwrap();

    let config_struct_name_str = format!("{name}SallyConfigurator");
    let sally_config_struct_name: proc_macro2::TokenStream =
        config_struct_name_str.parse().unwrap();

    let intermediate_db_map_struct_name_str = format!("{name}Primary");
    let intermediate_db_map_struct_name: proc_macro2::TokenStream =
        intermediate_db_map_struct_name_str.parse().unwrap();

    let secondary_db_map_struct_name_str = format!("{name}ReadOnly");
    let secondary_db_map_struct_name: proc_macro2::TokenStream =
        secondary_db_map_struct_name_str.parse().unwrap();

    TokenStream::from(quote! {

        // <----------- This section generates the configurator struct -------------->

        /// Create config structs for configuring SallyColumns
        pub struct #sally_config_struct_name {
            #(
                pub #field_names : typed_store::sally::SallyColumnOptions,
            )*
        }

        impl #sally_config_struct_name {
            /// Initialize to defaults
            pub fn init() -> Self {
                Self {
                    #(
                        #field_names : typed_store::sally::default_column_options(),
                    )*
                }
            }

            /// Build a config
            pub fn build(&self) -> typed_store::sally::SallyDBConfigMap {
                typed_store::sally::SallyDBConfigMap::new([
                    #(
                        (stringify!(#field_names).to_owned(), self.#field_names.clone()),
                    )*
                ].into_iter().collect())
            }
        }


        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > #name #generics {

                pub fn configurator() -> #sally_config_struct_name {
                    #sally_config_struct_name::init()
                }
        }


        // <----------- This section generates the core open logic for opening sally columns -------------->

        /// Create an intermediate struct used to open the DBMap tables in primary mode
        /// This is only used internally
        struct #intermediate_db_map_struct_name #generics {
                #(
                    pub #field_names : SallyColumn #inner_types,
                )*
        }


        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > #intermediate_db_map_struct_name #generics {
            /// Opens a set of tables in read-write mode
            /// If as_secondary_with_path is set, the DB is opened in read only mode with the path specified
            pub fn init(db_options: typed_store::sally::SallyDBOptions) -> Self {
                match db_options {
                    typed_store::sally::SallyDBOptions::TestDB => {
                        let (
                            #(
                                #field_names
                            ),*
                        ) = (#(
                            SallyColumn::TestDB((typed_store::test_db::TestDB::#inner_types::open(), typed_store::sally::SallyConfig::default()))
                            ),*);

                        Self {
                            #(
                                #field_names,
                            )*
                        }
                    },
                    typed_store::sally::SallyDBOptions::RocksDB((path, metric_conf, access_type, global_db_options_override, tables_db_options_override)) => {
                        let path = &path;
                        let (db, rwopt_cfs) = {
                            let opt_cfs = match tables_db_options_override {
                                None => [
                                    #(
                                        (stringify!(#field_names).to_owned(), #default_options_override_fn_names().clone()),
                                    )*
                                ],
                                Some(o) => [
                                    #(
                                        (stringify!(#field_names).to_owned(), o.to_map().get(stringify!(#field_names)).unwrap().clone()),
                                    )*
                                ]
                            };
                            // Safe to call unwrap because we will have at least one field_name entry in the struct
                            let rwopt_cfs: std::collections::HashMap<String, typed_store::rocks::ReadWriteOptions> = opt_cfs.iter().map(|q| (q.0.as_str().to_string(), q.1.rw_options.clone())).collect();
                            let opt_cfs: Vec<_> = opt_cfs.iter().map(|q| (q.0.as_str(), q.1.options.clone())).collect();
                            let db = match access_type {
                                RocksDBAccessType::Secondary(Some(p)) => typed_store::rocks::open_cf_opts_secondary(path, Some(&p), global_db_options_override, metric_conf, &opt_cfs),
                                _ => typed_store::rocks::open_cf_opts(path, global_db_options_override, metric_conf, &opt_cfs)
                            };
                            db.map(|d| (d, rwopt_cfs))
                        }.expect(&format!("Cannot open DB at {:?}", path));
                        let (
                            #(
                                #field_names
                            ),*
                        ) = (#(
                            SallyColumn::RocksDB((DBMap::#inner_types::reopen(&db, Some(stringify!(#field_names)), rwopt_cfs.get(stringify!(#field_names)).unwrap_or(&typed_store::rocks::ReadWriteOptions::default()), false).expect(&format!("Cannot open {} CF.", stringify!(#field_names))[..]), typed_store::sally::SallyConfig::default()))
                            ),*);

                        Self {
                            #(
                                #field_names,
                            )*
                        }
                    }
                }
            }
        }


        // <----------- This section generates the read-write open logic and other common utils -------------->
        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > #name #generics {
            /// Opens a set of tables in read-write mode
            /// Only one process is allowed to do this at a time
            /// `global_db_options_override` apply to the whole DB
            /// `tables_db_options_override` apply to each table. If `None`, the attributes from `default_options_override_fn` are used if any
            #[allow(unused_parens)]
            pub fn init(
                db_options: typed_store::sally::SallyDBOptions
            ) -> Self {
                let inner = #intermediate_db_map_struct_name::init(db_options);
                Self {
                    #(
                        #field_names: #post_process_fn(inner.#field_names),
                    )*
                }
            }

            /// Returns a list of the tables name and type pairs
            pub fn describe_tables() -> std::collections::BTreeMap<String, (String, String)> {
                vec![#(
                    (stringify!(#field_names).to_owned(), (stringify!(#key_names).to_owned(), stringify!(#value_names).to_owned())),
                )*].into_iter().collect()
            }

            /// This opens the DB in read only mode and returns a struct which exposes debug features
            pub fn get_read_only_handle (
                db_options: typed_store::sally::SallyReadOnlyDBOptions
                ) -> #secondary_db_map_struct_name #generics {
                #secondary_db_map_struct_name::init_read_only(db_options)
            }
        }

        // <----------- This section generates the features that use read-only open logic -------------->
        /// Create an intermediate struct used to open the DBMap tables in secondary mode
        /// This is only used internally
        pub struct #secondary_db_map_struct_name #generics {
            #(
                pub #field_names : SallyColumn #inner_types,
            )*
        }

        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > #secondary_db_map_struct_name #generics {
            /// Open in read only mode. No limitation on number of processes to do this
            pub fn init_read_only(
                db_options: typed_store::sally::SallyReadOnlyDBOptions,
            ) -> Self {
                match db_options {
                    typed_store::sally::SallyReadOnlyDBOptions::TestDB => {
                        let inner = #intermediate_db_map_struct_name::init(SallyDBOptions::TestDB);
                        Self {
                            #(
                                #field_names: inner.#field_names,
                            )*
                        }
                    },
                    typed_store::sally::SallyReadOnlyDBOptions::RocksDB(b) => {
                        let inner = match b.2 {
                            Some(q) => #intermediate_db_map_struct_name::init(SallyDBOptions::RocksDB((b.0, b.1, RocksDBAccessType::Secondary(Some(q)), b.3, None))),
                            None => {
                                let p: std::path::PathBuf = tempfile::tempdir()
                                    .expect("Failed to open temporary directory")
                                    .into_path();
                                #intermediate_db_map_struct_name::init(SallyDBOptions::RocksDB((b.0, b.1, RocksDBAccessType::Secondary(Some(p)), b.3, None)))
                            }
                        };
                        Self {
                            #(
                                #field_names: inner.#field_names,
                            )*
                        }
                    }
                }
            }

            /// Dump all key-value pairs in the page at the given table name
            /// Tables must be opened in read only mode using `open_tables_read_only`
            pub fn dump(&self, table_name: &str, page_size: u16,
                page_number: usize) -> eyre::Result<std::collections::BTreeMap<String, String>> {
                Ok(match table_name {
                    #(
                        stringify!(#field_names) => {
                            match &self.#field_names {
                                SallyColumn::RocksDB((db_map, typed_store::sally::SallyConfig { mode: typed_store::sally::SallyRunMode::FallbackToDB })) => {
                                    typed_store::traits::Map::try_catch_up_with_primary(db_map)?;
                                    typed_store::traits::Map::unbounded_iter(db_map)
                                        .skip((page_number * (page_size) as usize))
                                        .take(page_size as usize)
                                        .map(|(k, v)| (format!("{:?}", k), format!("{:?}", v)))
                                        .collect::<std::collections::BTreeMap<_, _>>()
                                }
                                _ => unimplemented!(),
                            }
                        }
                    )*
                    _ => eyre::bail!("No such table name: {}", table_name),
                })
            }

            pub fn table_summary(&self, table_name: &str) -> eyre::Result<typed_store::traits::TableSummary> {
                let mut count = 0;
                let mut key_bytes = 0;
                let mut value_bytes = 0;
                match table_name {
                    #(
                        stringify!(#field_names) => {
                            match &self.#field_names {
                                SallyColumn::RocksDB((db_map, typed_store::sally::SallyConfig { mode: typed_store::sally::SallyRunMode::FallbackToDB })) => {
                                    typed_store::traits::Map::try_catch_up_with_primary(db_map)?;
                                    db_map.table_summary()
                                }
                                _ => unimplemented!(),
                            }
                        }
                    )*

                    _ => eyre::bail!("No such table name: {}", table_name),
                }
            }

            /// Count the keys in this table
            /// Tables must be opened in read only mode using `open_tables_read_only`
            pub fn count_keys(&self, table_name: &str) -> eyre::Result<usize> {
                Ok(match table_name {
                    #(
                        stringify!(#field_names) => {
                            match &self.#field_names {
                                SallyColumn::RocksDB((db_map, typed_store::sally::SallyConfig { mode: typed_store::sally::SallyRunMode::FallbackToDB })) => {
                                    typed_store::traits::Map::try_catch_up_with_primary(db_map)?;
                                    typed_store::traits::Map::unbounded_iter(db_map).count()
                                }
                                _ => unimplemented!(),
                            }
                        }
                    )*

                    _ => eyre::bail!("No such table name: {}", table_name),
                })
            }

            pub fn describe_tables() -> std::collections::BTreeMap<String, (String, String)> {
                vec![#(
                    (stringify!(#field_names).to_owned(), (stringify!(#key_names).to_owned(), stringify!(#value_names).to_owned())),
                )*].into_iter().collect()
            }
        }


        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > TypedStoreDebug for #secondary_db_map_struct_name #generics {
                fn dump_table(
                    &self,
                    table_name: String,
                    page_size: u16,
                    page_number: usize,
                ) -> eyre::Result<std::collections::BTreeMap<String, String>> {
                    self.dump(table_name.as_str(), page_size, page_number)
                }

                fn primary_db_name(&self) -> String {
                    stringify!(#name).to_owned()
                }

                fn describe_all_tables(&self) -> std::collections::BTreeMap<String, (String, String)> {
                    Self::describe_tables()
                }

                fn count_table_keys(&self, table_name: String) -> eyre::Result<usize> {
                    self.count_keys(table_name.as_str())
                }
                fn table_summary(&self, table_name: String) -> eyre::Result<TableSummary> {
                    self.table_summary(table_name.as_str())
                }

        }

    })
}

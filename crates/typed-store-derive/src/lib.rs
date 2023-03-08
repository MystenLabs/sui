// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashSet};

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use syn::Type;
use syn::{
    parse_macro_input, AngleBracketedGenericArguments, Attribute, Generics, ItemStruct, Lit, Meta,
    PathArguments,
};

// This is used as default when none is specified
const DEFAULT_DB_OPTIONS_CUSTOM_FN: &str = "typed_store::rocks::default_db_options";
// Custom function which returns the option and overrides the defaults for this table
const DB_OPTIONS_CUSTOM_FUNCTION: &str = "default_options_override_fn";

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
) -> (
    Vec<Ident>,
    Vec<AngleBracketedGenericArguments>,
    Vec<GeneralTableOptions>,
    String,
) {
    // There must only be one map type used for all entries
    let allowed_strs: Vec<_> = allowed_map_type_names
        .iter()
        .map(|s| format!("{s}<K, V>"))
        .collect();
    let allowed_strs = allowed_strs.join(" or ");

    let info = input.fields.iter().map(|f| {
        let attrs: Vec<_> = f
            .attrs
            .iter()
            .filter(|a| a.path.is_ident(DB_OPTIONS_CUSTOM_FUNCTION))
            .collect();
        let options = if attrs.is_empty() {
            GeneralTableOptions::default()
        } else {
            GeneralTableOptions::OverrideFunction(
                get_options_override_function(attrs.get(0).unwrap()).unwrap(),
            )
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
                return (
                    (f.ident.as_ref().unwrap().clone(), type_str),
                    (inner_type, options),
                );
            } else {
                panic!("All struct members must be of type {allowed_strs}");
            }
        }
        panic!("All struct members must be of type {allowed_strs}");
    });

    let (field_info, inner_types_with_opts): (Vec<_>, Vec<_>) = info.unzip();
    let (field_names, simple_field_type_names): (Vec<_>, Vec<_>) = field_info.into_iter().unzip();

    // Check for homogeneous types
    if let Some(first) = simple_field_type_names.get(0) {
        simple_field_type_names.iter().for_each(|q| {
            if q != first {
                panic!("All struct members must be of same type");
            }
        })
    } else {
        panic!("Cannot derive on empty struct");
    };

    let (inner_types, options): (Vec<_>, Vec<_>) = inner_types_with_opts.into_iter().unzip();

    (
        field_names,
        inner_types,
        options,
        simple_field_type_names.get(0).unwrap().clone(),
    )
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

/// A helper macro to simplify common operations for opening and debugging TypedStore (currently internally structs of DBMaps)
/// It operates on a struct where all the members are of Store<K, V> or DBMap<K, V>
/// `TypedStoreDebug` traits are then derived
/// The main features are:
/// 1. Flexible configuration of each table (column family) via defaults and overrides
/// 2. Auto-generated `open` routine
/// 3. Auto-generated `read_only_mode` handle
/// 4. Auto-generated memory stats method
/// 5. Other convenience features
///
/// 1. Flexible configuration:
/// a. Static options specified at struct definition
/// The definer of the struct can specify the default options for each table using annotations
/// We can also supply column family options on the default ones
/// A user defined function of signature () -> Options can be provided for each table
/// If a an override function is not specified, the default in `typed_store::rocks::default_db_options` is used
/// ```
/// use typed_store::rocks::DBOptions;
/// use typed_store::rocks::DBMap;
/// use typed_store::Store;
/// use typed_store_derive::DBMapUtils;
/// use typed_store::traits::TypedStoreDebug;
/// use typed_store::traits::TableSummary;
/// use core::fmt::Error;
/// /// Define a struct with all members having type DBMap<K, V>
///
/// fn custom_fn_name1() -> DBOptions {DBOptions::default()}
/// fn custom_fn_name2() -> DBOptions {
///     let mut op = custom_fn_name1();
///     op.options.set_write_buffer_size(123456);
///     op
/// }
/// #[derive(DBMapUtils)]
/// struct Tables {
///     /// Specify custom options function `custom_fn_name1`
///     #[default_options_override_fn = "custom_fn_name1"]
///     table1: DBMap<String, String>,
///     #[default_options_override_fn = "custom_fn_name2"]
///     table2: DBMap<i32, String>,
///     // Nothing specified so `typed_store::rocks::default_db_options` is used
///     table3: DBMap<i32, String>,
///     #[default_options_override_fn = "custom_fn_name1"]
///     table4: DBMap<i32, String>,
/// }
///
/// // b. Options specified by DB opener
/// // For finer control, we also allow the opener of the DB to specify their own options which override the defaults set by the definer
/// // This is done via a configurator which gives one a struct with field similarly named as that of the DB, but of type Options
///
/// #[tokio::main]
/// async fn main() -> Result<(), Error> {
/// // Get a configurator for this table
/// let mut config = Tables::configurator();
/// // Config table 1
/// config.table1 = DBOptions::default();
/// config.table1.options.create_if_missing(true);
/// config.table1.options.set_write_buffer_size(123456);
///
/// let primary_path = tempfile::tempdir().expect("Failed to open temporary directory").into_path();
///
/// // We can then open the DB with the configs
/// let _ = Tables::open_tables_read_write(primary_path, None, Some(config.build()));
/// Ok(())
/// }
///
///```
///
/// 2. Auto-generated `open` routine
/// The function `open_tables_read_write` is generated which allows for specifying DB wide options and custom table configs as mentioned above
///
/// 3. Auto-generated `read_only_mode` handle
/// This mode provides handle struct which opens the DB in read only mode and has certain features like dumping and counting the keys in the tables
///
/// Use the function `Tables::get_read_only_handle` which returns a handle that only allows read only features
///```
/// use typed_store::rocks::DBOptions;
/// use typed_store::rocks::DBMap;
/// use typed_store_derive::DBMapUtils;
/// use typed_store::traits::TypedStoreDebug;
/// use core::fmt::Error;
/// use typed_store::traits::TableSummary;
/// /// Define a struct with all members having type DBMap<K, V>
///
/// fn custom_fn_name1() -> DBOptions {DBOptions::default()}
/// fn custom_fn_name2() -> DBOptions {
///     let mut op = custom_fn_name1();
///     op.options.set_write_buffer_size(123456);
///     op
/// }
/// #[derive(DBMapUtils)]
/// struct Tables {
///     /// Specify custom options function `custom_fn_name1`
///     #[default_options_override_fn = "custom_fn_name1"]
///     table1: DBMap<String, String>,
///     #[default_options_override_fn = "custom_fn_name2"]
///     table2: DBMap<i32, String>,
///     // Nothing specified so `typed_store::rocks::default_db_options` is used
///     table3: DBMap<i32, String>,
///     #[default_options_override_fn = "custom_fn_name1"]
///     table4: DBMap<i32, String>,
/// }
/// #[tokio::main]
/// async fn main() -> Result<(), Error> {
///
/// let primary_path = tempfile::tempdir().expect("Failed to open temporary directory").into_path();
/// let _ = Tables::open_tables_read_write(primary_path.clone(), None, None);
///
/// // Get the read only handle
/// let read_only_handle = Tables::get_read_only_handle(primary_path, None, None);
/// // Use this handle for dumping
/// let ret = read_only_handle.dump("table2", 100, 0).unwrap();
/// let key_count = read_only_handle.count_keys("table1").unwrap();
/// Ok(())
/// }
/// ```
/// 4. Auto-generated memory stats method
/// `self.get_memory_usage` is derived to provide memory and cache usage
///
/// 5. Other convenience features
/// `Tables::describe_tables` is used to get a list of the table names and key-value types as string in a BTreeMap
///
/// // Bad usage example
/// // Structs fields most only be of type Store<K, V> or DMBap<K, V>
/// // This will fail to compile with error `All struct members must be of type Store<K, V> or DMBap<K, V>`
/// // #[derive(DBMapUtils)]
/// // struct BadTables {
/// //     table1: Store<String, String>,
/// //     bad_field: u32,
/// // #}

#[proc_macro_derive(DBMapUtils, attributes(default_options_override_fn))]
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
    let (field_names, inner_types, derived_table_options, simple_field_type_name_str) =
        extract_struct_info(input.clone(), allowed_strs);

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
                global_db_options_override: Option<rocksdb::Options>,
                tables_db_options_override: Option<typed_store::rocks::DBMapTableConfigMap>
            ) -> Self {
                let path = &path;
                let (db, rwopt_cfs) = {
                    let opt_cfs = match tables_db_options_override {
                        None => [
                            #(
                                (stringify!(#field_names).to_owned(), #default_options_override_fn_names()),
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
                    let opt_cfs: Vec<_> = opt_cfs.iter().map(|q| (q.0.as_str(), &q.1.options)).collect();
                    let db = match (as_secondary_with_path, is_transaction) {
                        (Some(p), _) => typed_store::rocks::open_cf_opts_secondary(path, Some(&p), global_db_options_override, &opt_cfs),
                        (_, true) => typed_store::rocks::open_cf_opts_transactional(path, global_db_options_override, &opt_cfs),
                        _ => typed_store::rocks::open_cf_opts(path, global_db_options_override, &opt_cfs)
                    };
                    db.map(|d| (d, rwopt_cfs))
                }.expect("Cannot open DB.");
                let (
                        #(
                            #field_names
                        ),*
                ) = (#(
                        DBMap::#inner_types::reopen(&db, Some(stringify!(#field_names)), rwopt_cfs.get(stringify!(#field_names)).unwrap_or(&typed_store::rocks::ReadWriteOptions::default())).expect(&format!("Cannot open {} CF.", stringify!(#field_names))[..])
                    ),*);

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
                global_db_options_override: Option<rocksdb::Options>,
                tables_db_options_override: Option<typed_store::rocks::DBMapTableConfigMap>
            ) -> Self {
                let inner = #intermediate_db_map_struct_name::open_tables_impl(path, None, false, global_db_options_override, tables_db_options_override);
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
                global_db_options_override: Option<rocksdb::Options>,
                tables_db_options_override: Option<typed_store::rocks::DBMapTableConfigMap>
            ) -> Self {
                let inner = #intermediate_db_map_struct_name::open_tables_impl(path, None, true, global_db_options_override, tables_db_options_override);
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
                global_db_options_override: Option<rocksdb::Options>,
                ) -> #secondary_db_map_struct_name #generics {
                #secondary_db_map_struct_name::open_tables_read_only(primary_path, with_secondary_path, global_db_options_override)
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
                global_db_options_override: Option<rocksdb::Options>,
            ) -> Self {
                let inner = match with_secondary_path {
                    Some(q) => #intermediate_db_map_struct_name::open_tables_impl(primary_path, Some(q), false, global_db_options_override, None),
                    None => {
                        let p: std::path::PathBuf = tempfile::tempdir()
                        .expect("Failed to open temporary directory")
                        .into_path();
                        #intermediate_db_map_struct_name::open_tables_impl(primary_path, Some(p), false, global_db_options_override, None)
                    }
                };
                Self {
                    #(
                        #field_names: inner.#field_names,
                    )*
                }
            }

            /// Dump all key-value pairs in the page at the given table name
            /// Tables must be opened in read only mode using `open_tables_read_only`
            pub fn dump(&self, table_name: &str, page_size: u16,
                page_number: usize) -> eyre::Result<std::collections::BTreeMap<String, String>> {
                Ok(match table_name {
                    #(
                        stringify!(#field_names) => {
                            typed_store::traits::Map::try_catch_up_with_primary(&self.#field_names)?;
                            typed_store::traits::Map::iter(&self.#field_names)
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
                            typed_store::traits::Map::iter(&self.#field_names).count()
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
    let (field_names, inner_types, derived_table_options, simple_field_type_name_str) =
        extract_struct_info(input.clone(), allowed_strs);

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
                    typed_store::sally::SallyDBOptions::RocksDB((path, access_type, global_db_options_override, tables_db_options_override)) => {
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
                            let opt_cfs: Vec<_> = opt_cfs.iter().map(|q| (q.0.as_str(), &q.1.options)).collect();
                            let db = match access_type {
                                RocksDBAccessType::Secondary(Some(p)) => typed_store::rocks::open_cf_opts_secondary(path, Some(&p), global_db_options_override, &opt_cfs),
                                _ => typed_store::rocks::open_cf_opts(path, global_db_options_override, &opt_cfs)
                            };
                            db.map(|d| (d, rwopt_cfs))
                        }.expect("Cannot open DB.");
                        let (
                            #(
                                #field_names
                            ),*
                        ) = (#(
                            SallyColumn::RocksDB((DBMap::#inner_types::reopen(&db, Some(stringify!(#field_names)), rwopt_cfs.get(stringify!(#field_names)).unwrap_or(&typed_store::rocks::ReadWriteOptions::default())).expect(&format!("Cannot open {} CF.", stringify!(#field_names))[..]), typed_store::sally::SallyConfig::default()))
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
                        let inner = match b.1 {
                            Some(q) => #intermediate_db_map_struct_name::init(SallyDBOptions::RocksDB((b.0, RocksDBAccessType::Secondary(Some(q)), b.2, None))),
                            None => {
                                let p: std::path::PathBuf = tempfile::tempdir()
                                    .expect("Failed to open temporary directory")
                                    .into_path();
                                #intermediate_db_map_struct_name::init(SallyDBOptions::RocksDB((b.0, RocksDBAccessType::Secondary(Some(p)), b.2, None)))
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
                                    typed_store::traits::Map::iter(db_map)
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
                                    typed_store::traits::Map::iter(db_map).count()
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

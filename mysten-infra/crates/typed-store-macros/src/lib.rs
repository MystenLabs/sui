// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use syn::Type::{self};
use syn::{
    parse_macro_input, AngleBracketedGenericArguments, Attribute, Generics, ItemStruct, Lit, Meta,
    PathArguments,
};

// This is used as default when none is specified
const DEFAULT_DB_OPTIONS_CUSTOM_FN: &str = "typed_store::rocks::default_rocksdb_options";
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

/// A helper macro to simplify common operations for opening and dumping structs of DBMaps
/// It operates on a struct where all the members are DBMap<K, V>
/// `DBMapTableUtil` traits are then derived
/// We can also supply column family options on the default ones
/// A user defined function of signature () -> Options can be provided for each table
/// If a an override function is not specified, the default in `typed_store::rocks::default_rocksdb_options` is used
/// The old way creating tables is to define a struct of DBMap tables, create the column families, then reopen
/// If dumping is needed, there's an additional step of implementing a way to match and dump each table
///
/// We remove the need for all these steps by auto deriving the member functions for opening, confguring, dumping, etc.
///
/// # Examples
///
/// Well formed struct of tables
/// ```
/// use rocksdb::Options;
/// use typed_store::rocks::DBMap;
/// use typed_store_macros::DBMapUtils;
/// use typed_store::traits::DBMapTableUtil;
///
/// /// Define a struct with all members having type DBMap<K, V>
///
/// fn custom_fn_name1() -> Options {Options::default()}
/// fn custom_fn_name2() -> Options {
///     let mut op = custom_fn_name1();
///     op.set_write_buffer_size(123456);
///     op
/// }
/// #[derive(DBMapUtils)]
/// struct Tables {
///     /// Specify custom options function `custom_fn_name1`
///     #[default_options_override_fn = "custom_fn_name1"]
///     table1: DBMap<String, String>,
///     #[default_options_override_fn = "custom_fn_name2"]
///     table2: DBMap<i32, String>,
///     // Nothing specifed so `typed_store::rocks::default_rocksdb_options` is used
///     table3: DBMap<i32, String>,
///     #[default_options_override_fn = "custom_fn_name1"]
///     table4: DBMap<i32, String>,
/// }
///
/// /// All traits in `DBMapTableUtil` are automatically derived
/// /// Use the struct like normal
/// let primary_path = tempfile::tempdir().expect("Failed to open temporary directory").into_path();
/// /// This is auto derived
/// let tbls_primary = Tables::open_tables_read_write(primary_path.clone(), None, None);
///
/// /// Do some stuff with the DB
///
/// /// We must open as secondary (read only) before using debug features
/// /// Open in secondary mode for dumping and other debug features
/// let tbls_secondary = Tables::open_tables_read_only(primary_path.clone(), None, None);
/// /// Table dump is auto derived
/// let entries = tbls_secondary.dump("table1", 100, 0).unwrap();
/// /// Key counting fn is auto derived
/// let key_count = tbls_secondary.count_keys("table1").unwrap();
/// /// Listing all tables is auto derived
/// let table_names = Tables::list_tables(primary_path).unwrap();
///
/// // Bad usage example
/// // Structs fields most only be of type DBMap<K, V>
/// // This will fail to compile with error `All struct members must be of type DMBap<K, V>`
/// // #[derive(DBMapUtils)]
/// // struct BadTables {
/// //     table1: DBMap<String, String>,
/// //     bad_field: u32,
/// // #}
/// ```
#[proc_macro_derive(DBMapUtils, attributes(options, default_options_override_fn))]
pub fn derive_dbmap_utils(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    let name = &input.ident;
    let generics = &input.generics;
    let generics_names = extract_generics_names(generics);

    let (field_names, inner_types, derived_table_options, simple_field_type_name) =
        extract_struct_info(input.clone());
    let default_options_override_fn_names: Vec<proc_macro2::TokenStream> = derived_table_options
        .iter()
        .map(|q| {
            let GeneralTableOptions::OverrideFunction(fn_name) = q;
            fn_name.parse().unwrap()
        })
        .collect();

    let precondition_str = "#[pre(\"Must be called only after `open_tables_read_only`\")]";
    let _precondition_str_tok: proc_macro2::TokenStream = precondition_str.parse().unwrap();
    let generics_bounds =
        "std::fmt::Debug + serde::Serialize + for<'de> serde::de::Deserialize<'de>";
    let generics_bounds_token: proc_macro2::TokenStream = generics_bounds.parse().unwrap();

    let config_struct_name_str = format!("{}Configurator", name);
    let config_struct_name: proc_macro2::TokenStream = config_struct_name_str.parse().unwrap();

    let first_field_name = field_names
        .get(0)
        .expect("Expected at least one field")
        .clone();

    // TODO: use this to disambiguate Store from DBMap when unifying both
    let _simple_field_type_name: proc_macro2::TokenStream = simple_field_type_name.parse().unwrap();

    TokenStream::from(quote! {
        /// Create config structs for configuring DBMap tables
        pub struct #config_struct_name {
            #(
                pub #field_names : rocksdb::Options,
            )*
        }

        impl #config_struct_name {
            /// Initialize to defaults
            pub fn init() -> Self {
                Self {
                    #(
                        #field_names : typed_store::rocks::default_rocksdb_options(),
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
        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > DBMapTableUtil for #name #generics{
            /// Opens a set of tables in read-write mode
            /// Only one process is allowed to do this at a time
            /// `global_db_options_override` apply to the whole DB
            /// `tables_db_options_override` apply to each table. If `None`, the attributes from `default_options_override_fn` are used if any
            fn open_tables_read_write(
                path: std::path::PathBuf,
                global_db_options_override: Option<rocksdb::Options>,
                tables_db_options_override: Option<typed_store::rocks::DBMapTableConfigMap>
            ) -> Self {
                Self::open_tables_impl(path, None, global_db_options_override, tables_db_options_override)
            }

            /// Open in read only mode. No limitation on number of processes to do this
            fn open_tables_read_only(
                path: std::path::PathBuf,
                with_secondary_path: Option<std::path::PathBuf>,
                global_db_options_override: Option<rocksdb::Options>,
            ) -> Self {
                match with_secondary_path {
                    Some(q) => Self::open_tables_impl(path, Some(q), global_db_options_override, None),
                    None => {
                        let p: std::path::PathBuf = tempfile::tempdir()
                        .expect("Failed to open temporary directory")
                        .into_path();
                        Self::open_tables_impl(path, Some(p), global_db_options_override, None)
                    }
                }
            }

            /// Opens a set of tables in read-write mode
            /// If with_secondary_path is set, the DB is opened in read only mode with the path specified
            fn open_tables_impl(
                path: std::path::PathBuf,
                with_secondary_path: Option<std::path::PathBuf>,
                global_db_options_override: Option<rocksdb::Options>,
                tables_db_options_override: Option<typed_store::rocks::DBMapTableConfigMap>
            ) -> Self {
                let path = &path;
                let db = {
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

                    let opt_cfs: Vec<_> = opt_cfs.iter().map(|q| (q.0.as_str(), &q.1)).collect();

                    let res = match with_secondary_path {
                        Some(p) => typed_store::rocks::open_cf_opts_secondary(path, Some(&p), global_db_options_override, &opt_cfs),
                        None    => typed_store::rocks::open_cf_opts(path, global_db_options_override, &opt_cfs)
                    };
                    res
                }.expect("Cannot open DB.");

                let (
                        #(
                            #field_names
                        ),*
                ) = (#(
                        DBMap::#inner_types::reopen(&db, Some(stringify!(#field_names))).expect(&format!("Cannot open {} CF.", stringify!(#field_names))[..])
                    ),*);

                Self {
                    #(
                        #field_names,
                    )*
                }
            }

            /// Dump all key-value pairs in the page at the given table name
            /// Tables must be opened in read only mode using `open_tables_read_only`
            /// TODO: use preconditions to ensure call after `open_tables_read_only`
            // #_precondition_str_tok
            fn dump(&self, table_name: &str, page_size: u16,
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

            /// Count the keys in this table
            /// Tables must be opened in read only mode using `open_tables_read_only`
            /// TODO: use preconditions to ensure call after `open_tables_read_only`
            // #_precondition_str_tok
            fn count_keys(&self, table_name: &str) -> eyre::Result<usize> {
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

            /// This gives info about memory usage and returns a tuple of total table memory usage and cache memory usage
            fn get_memory_usage(&self) -> Result<(u64, u64), typed_store::rocks::TypedStoreError> {
                let stats = rocksdb::perf::get_memory_usage_stats(Some(&[&self.#first_field_name.rocksdb]), None)
                    .map_err(|e| typed_store::rocks::TypedStoreError::RocksDBError(e.to_string()))?;
                Ok((stats.mem_table_total, stats.cache_total))
            }
        }
    })
}

// Extracts the field names, field types, inner types (K,V in DBMap<K, V>), and the options attrs
fn extract_struct_info(
    input: ItemStruct,
) -> (
    Vec<Ident>,
    Vec<AngleBracketedGenericArguments>,
    Vec<GeneralTableOptions>,
    String,
) {
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
                    panic!("All struct members must be of type DMBap<K, V>");
                };

            let type_str = format!("{}", &type_info.ident);
            // Rough way to check that this is DBMap
            if type_str == "DBMap" {
                return (
                    (f.ident.as_ref().unwrap().clone(), type_str),
                    (inner_type, options),
                );
            } else {
                panic!("All struct members must be of type DMBap<K, V>");
            }
        }
        panic!("All struct members must be of type DMBap<K, V>");
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
            _ => panic!("Unspoorted generic type"),
        })
        .collect()
}

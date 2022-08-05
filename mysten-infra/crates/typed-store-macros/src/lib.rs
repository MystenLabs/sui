// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use syn::Type::{self};
use syn::{
    parse_macro_input, AngleBracketedGenericArguments, Attribute, Generics, ItemStruct, Lit, Meta,
    NestedMeta, PathArguments,
};

const DEFAULT_CACHE_CAPACITY: usize = 300_000;
const DB_OPTIONS_ATTR_NAME: &str = "options";
const DB_OPTIONS_OPTIMIZATION_ATTR_NAME: &str = "optimization";
const DB_OPTIONS_CACHE_CAPACITY_ATTR_NAME: &str = "cache_capacity";
const DB_OPTIONS_POINT_LOOKUP_ATTR_NAME: &str = "point_lookup";

/// Commonly used options
struct TableOptions {
    pub point_lookup: bool,
    pub cache_capacity: usize,
}

impl Default for TableOptions {
    fn default() -> Self {
        TableOptions {
            point_lookup: false,
            cache_capacity: DEFAULT_CACHE_CAPACITY,
        }
    }
}

/// A helper macro to simplify common operations for opening and dumping structs of DBMaps
/// It operates on a struct where all the members are DBMap<K, V>
/// `DBMapTableUtil` traits are then derived
/// We can also supply column family options on the default ones
///  #[options(optimization = "point_lookup", cache_capacity = "1000000")]
///
/// The typical flow for creating tables is to define a struct of DBMap tables, create the column families, then reopen
/// If dumping is needed, there's an additional step of implementing a way to match and dump each table
///
/// We remove the need for all these steps by auto deriving the member functions.
///
/// # Examples
///
/// Well formed struct of tables
/// ```
/// use typed_store::rocks::DBMap;
/// use typed_store_macros::DBMapUtils;
/// use typed_store::traits::DBMapTableUtil;
///
/// /// Define a struct with all members having type DBMap<K, V>
/// #[derive(DBMapUtils)]
/// struct Tables {
///     /// Specify some or no options for each field
///     /// Valid options are currently `optimization = "point_lookup"` and `cache_capacity = <Uint>`
///     #[options(optimization = "point_lookup", cache_capacity = 100000)]
///     table1: DBMap<String, String>,
///     #[options(optimization = "point_lookup")]
///     table2: DBMap<i32, String>,
///     table3: DBMap<i32, String>,
///     #[options()]
///     table4: DBMap<i32, String>,
/// }
///
/// /// All traits in `DBMapTableUtil` are automatically derived
/// /// Use the struct like normal
/// let primary_path = tempfile::tempdir().expect("Failed to open temporary directory").into_path();
/// /// This is auto derived
/// let tbls_primary = Tables::open_tables_read_write(primary_path.clone(), None);
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
#[proc_macro_derive(DBMapUtils, attributes(options))]
pub fn derive_dbmap_utils(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemStruct);
    let name = &input.ident;
    let generics = &input.generics;
    let generics_names = extract_generics_names(generics);

    let (field_names, _field_types, inner_types, derived_table_options) =
        extract_struct_info(input.clone());
    let (cache_capacity, point_lookup): (Vec<_>, Vec<_>) = derived_table_options
        .iter()
        .map(|q| (q.cache_capacity, q.point_lookup))
        .unzip();

    let precondition_str = "#[pre(\"Must be called only after `open_tables_read_only`\")]";
    let _precondition_str_tok: proc_macro2::TokenStream = precondition_str.parse().unwrap();
    let generics_bounds =
        "std::fmt::Debug + serde::Serialize + for<'de> serde::de::Deserialize<'de>";
    let generics_bounds_token: proc_macro2::TokenStream = generics_bounds.parse().unwrap();

    TokenStream::from(quote! {
        impl <
                #(
                    #generics_names: #generics_bounds_token,
                )*
            > DBMapTableUtil for #name #generics{
            /// Opens a set of tables in read-write mode
            /// Only one process is allowed to do this at a time
            fn open_tables_read_write(
                path: std::path::PathBuf,
                db_options: Option<rocksdb::Options>,
            ) -> Self {
                Self::open_tables_impl(path, None, db_options)
            }

            /// Open in read only mode. No limitation on number of processes to do this
            fn open_tables_read_only(
                path: std::path::PathBuf,
                with_secondary_path: Option<std::path::PathBuf>,
                db_options: Option<rocksdb::Options>,
            ) -> Self {
                match with_secondary_path {
                    Some(q) => Self::open_tables_impl(path, Some(q), db_options),
                    None => {
                        let p: std::path::PathBuf = tempfile::tempdir()
                        .expect("Failed to open temporary directory")
                        .into_path();
                        Self::open_tables_impl(path, Some(p), db_options)
                    }
                }
            }

            /// Opens a set of tables in read-write mode
            /// If with_secondary_path is set, the DB is opened in read only mode with the path specified
            fn open_tables_impl(
                path: std::path::PathBuf,
                with_secondary_path: Option<std::path::PathBuf>,
                db_options: Option<rocksdb::Options>,
            ) -> Self {
                let path = &path;
                let db = {
                    let opt_cfs: &[(&str, &rocksdb::Options)] = &[
                        #(
                            (stringify!(#field_names), &Self::adjusted_db_options(None, #cache_capacity, #point_lookup).clone()),
                        )*
                    ];

                    if let Some(p) = with_secondary_path {
                        typed_store::rocks::open_cf_opts_secondary(path, Some(&p), db_options, opt_cfs)
                    } else {
                        typed_store::rocks::open_cf_opts(path, db_options, opt_cfs)
                    }
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
                page_number: usize) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
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

                    _ => anyhow::bail!("No such table name: {}", table_name),
                })
            }

            /// Count the keys in this table
            /// Tables must be opened in read only mode using `open_tables_read_only`
            /// TODO: use preconditions to ensure call after `open_tables_read_only`
            // #_precondition_str_tok
            fn count_keys(&self, table_name: &str) -> anyhow::Result<usize> {
                Ok(match table_name {
                    #(
                        stringify!(#field_names) => {
                            typed_store::traits::Map::try_catch_up_with_primary(&self.#field_names)?;
                            typed_store::traits::Map::iter(&self.#field_names).count()
                        }
                    )*

                    _ => anyhow::bail!("No such table name: {}", table_name),
                })
            }
        }
    })
}

// Extracts the field names, field types, inner types (K,V in DBMap<K, V>), and the options attrs
fn extract_struct_info(
    input: ItemStruct,
) -> (
    Vec<Ident>,
    Vec<Type>,
    Vec<AngleBracketedGenericArguments>,
    Vec<TableOptions>,
) {
    let info = input.fields.iter().map(|f| {
        let attrs: Vec<_> = f
            .attrs
            .iter()
            .filter(|a| a.path.is_ident(DB_OPTIONS_ATTR_NAME))
            .collect();

        let options = if attrs.is_empty() {
            TableOptions::default()
        } else {
            get_options(attrs.get(0).unwrap()).unwrap()
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
                    (f.ident.as_ref().unwrap().clone(), f.ty.clone()),
                    (inner_type, options),
                );
            } else {
                panic!("All struct members must be of type DMBap<K, V>");
            }
        }
        panic!("All struct members must be of type DMBap<K, V>");
    });

    let (field_info, inner_types_with_opts): (Vec<_>, Vec<_>) = info.unzip();
    let (field_names, _field_types): (Vec<_>, Vec<_>) = field_info.into_iter().unzip();
    let (inner_types, options): (Vec<_>, Vec<_>) = inner_types_with_opts.into_iter().unzip();

    (field_names, _field_types, inner_types, options)
}

/// Extracts the options from attribute
/// Valid options are `optimization = "point_lookup"` and `cache_capacity = <usize>
fn get_options(attr: &Attribute) -> syn::Result<TableOptions> {
    let meta = attr.parse_meta()?;

    let meta_list = match meta {
        Meta::List(list) => list,
        _ => {
            return Err(syn::Error::new_spanned(
                meta,
                "Expected attribute list of options",
            ))
        }
    };

    let tokens = match meta_list.nested.len() {
        0 => {
            return Ok(TableOptions {
                point_lookup: false,
                cache_capacity: DEFAULT_CACHE_CAPACITY,
            })
        }
        1 | 2 => &meta_list.nested,
        _ => {
            return Err(syn::Error::new_spanned(
                meta_list.nested,
                format!("At most 2 attributes allowed: `{DB_OPTIONS_OPTIMIZATION_ATTR_NAME}` and/or `{DB_OPTIONS_CACHE_CAPACITY_ATTR_NAME}`"),
            ));
        }
    };

    let mut point_lookup = None;
    let mut cache_capacity: Option<usize> = None;

    for t in tokens {
        let name_val = match t {
            NestedMeta::Meta(Meta::NameValue(nv)) => nv,
            _ => return Err(syn::Error::new_spanned(t, "Expected `<opt> = \"<value>\"`")),
        };

        if name_val.path.is_ident(DB_OPTIONS_CACHE_CAPACITY_ATTR_NAME) {
            if cache_capacity.is_some() {
                return Err(syn::Error::new_spanned(
                    name_val,
                    format!("Duplicate entry for `{DB_OPTIONS_CACHE_CAPACITY_ATTR_NAME}`"),
                ));
            }
            cache_capacity = match &name_val.lit {
                Lit::Int(i) => Some(i.base10_parse().unwrap()),
                _ => {
                    return Err(syn::Error::new_spanned(
                        t,
                        format!(
                            "Expected unsigned integer for `{DB_OPTIONS_CACHE_CAPACITY_ATTR_NAME}`"
                        ),
                    ))
                }
            };
        } else if name_val.path.is_ident(DB_OPTIONS_OPTIMIZATION_ATTR_NAME) {
            if point_lookup.is_some() {
                return Err(syn::Error::new_spanned(
                    name_val,
                    format!("Duplicate entry for `{DB_OPTIONS_OPTIMIZATION_ATTR_NAME}`"),
                ));
            }
            let opt = match &name_val.lit {
                Lit::Str(s) => s.value(),
                _ => {
                    return Err(syn::Error::new_spanned(
                        t,
                        format!("Expected string for `{DB_OPTIONS_OPTIMIZATION_ATTR_NAME}`"),
                    ))
                }
            };

            if opt != DB_OPTIONS_POINT_LOOKUP_ATTR_NAME {
                return Err(syn::Error::new_spanned(
                    t,
                    format!("Only `{DB_OPTIONS_POINT_LOOKUP_ATTR_NAME}`  supported for `{DB_OPTIONS_OPTIMIZATION_ATTR_NAME}`"),
                ));
            }
            point_lookup = Some(true);
        } else {
            return Err(syn::Error::new_spanned(
                t,
                format!("Only `{DB_OPTIONS_OPTIMIZATION_ATTR_NAME}` and `{DB_OPTIONS_CACHE_CAPACITY_ATTR_NAME}` are valid options"),
            ));
        }
    }

    Ok(TableOptions {
        point_lookup: point_lookup.is_some(),
        cache_capacity: cache_capacity.unwrap_or(DEFAULT_CACHE_CAPACITY),
    })
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

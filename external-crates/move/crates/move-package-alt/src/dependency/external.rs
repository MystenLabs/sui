// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods for external dependencies (of the form `{ r.<res> = data }`).

use std::{
    collections::BTreeMap,
    fmt::Debug,
    iter::once,
    ops::Range,
    path::{Path, PathBuf},
};

use futures::future::try_join_all;
use serde::{
    Deserialize, Serialize,
    de::{MapAccess, Visitor},
};
use serde_spanned::Spanned;
use tracing::warn;

use crate::{
    errors::{
        self, FileHandle, Located, ManifestError, ManifestErrorKind, PackageError, PackageResult,
        ResolverError,
    },
    flavor::MoveFlavor,
    package::{EnvironmentName, PackageName},
};

use super::{
    DependencySet, ManifestDependencyInfo, PinnedDependencyInfo,
    external_protocol::{Query, QueryID, QueryResult, Request, Response},
    pin,
};

type ResolverName = String;

/// An external dependency has the form `{ r.<res> = <data> }`. External
/// dependencies are resolved by external resolvers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(try_from = "RField", into = "RField")]
pub struct ExternalDependency {
    /// The `<res>` in `{ r.<res> = <data> }`
    pub resolver: ResolverName,

    /// the `<data>` in `{ r.<res> = <data> }`
    data: Located<toml::Value>,
}

/// Convenience type for serializing/deserializing external deps
#[derive(Serialize, Deserialize)]
struct RField {
    r: Located<BTreeMap<String, toml::Value>>,
}

impl ExternalDependency {
    /// Invoke the external binaries to resolve all [deps] in all [envs]; deserialize their outputs
    /// as dependencies.
    ///
    /// Note that the return value may not have entries for all of the environments in [envs]; some
    /// may be removed if they are identical to the default resolutions.
    ///
    /// The return value is guaranteed to contain no external dependencies
    pub async fn resolve<F: MoveFlavor>(
        deps: DependencySet<ExternalDependency>,
        envs: &BTreeMap<EnvironmentName, F::EnvironmentID>,
    ) -> PackageResult<DependencySet<ManifestDependencyInfo<F>>> {
        // split by resolver
        let mut sorted: BTreeMap<ResolverName, DependencySet<toml::Value>> = BTreeMap::new();
        for (env, package_name, dep) in deps.into_iter() {
            sorted.entry(dep.resolver).or_default().insert(
                env,
                package_name,
                dep.data.into_inner(),
            );
        }

        // run the resolvers
        let resolved = sorted
            .into_iter()
            .map(move |(resolver, deps)| resolve_single::<F>(resolver, deps, envs));

        let resolved_all = try_join_all(resolved).await?;

        Ok(DependencySet::merge(resolved_all))
    }
}

impl TryFrom<RField> for ExternalDependency {
    type Error = PackageError;

    fn try_from(value: RField) -> Result<Self, Self::Error> {
        if value.r.as_ref().len() != 1 {
            return Err(PackageError::Manifest(ManifestError {
                kind: ManifestErrorKind::BadExternalDependency,
                span: Some(value.r.span()),
                handle: value.r.file(),
            }));
        }

        let (r, file, span) = value.r.destructure();

        let (resolver, data) = r
            .into_iter()
            .next()
            .expect("iterator of length 1 structure is nonempty");
        let data = Located::new(data, file, span);

        Ok(Self { resolver, data })
    }
}

impl From<ExternalDependency> for RField {
    fn from(value: ExternalDependency) -> Self {
        let ExternalDependency { resolver, data } = value;
        let (content, file, span) = data.destructure();

        RField {
            r: Located::new(BTreeMap::from([(resolver, content)]), file, span),
        }
    }
}

impl<F: MoveFlavor> TryFrom<QueryResult> for ManifestDependencyInfo<F> {
    type Error = PackageError;

    fn try_from(value: QueryResult) -> PackageResult<Self> {
        match value {
            // TODO: errors!
            QueryResult::Error { error } => {
                return Err(PackageError::Resolver(ResolverError::resolver_failed(
                    "resolver".to_string(),
                    PackageName::default(),
                    None,
                    error.clone(),
                )));
            }
            // TODO: warnings!
            QueryResult::Success { warnings, resolved } => {
                Self::deserialize(resolved).map_err(|e| {
                    PackageError::Resolver(ResolverError::bad_resolver(
                        &"".to_string(),
                        format!("{e}"),
                    ))
                })
            }
        }
    }
}

/// Resolve the dependencies in [dep_data] with the external resolver [resolver]; requests are
/// performed for all environments in [envs]. Ensures that the returned dependency set contains no
/// externally resolved dependencies.
async fn resolve_single<F: MoveFlavor>(
    resolver: ResolverName,
    dep_data: DependencySet<toml::Value>,
    envs: &BTreeMap<EnvironmentName, F::EnvironmentID>,
) -> PackageResult<DependencySet<ManifestDependencyInfo<F>>> {
    let request = create_request::<F>(&dep_data, envs);

    // invoke the resolver
    let response = request.execute(&resolver).await?;

    // build the result
    process_response(&resolver, &dep_data, envs, response)
}

/// Generate a unique identifier corresponding to [env] and [pkg]
fn query_id(env: &Option<EnvironmentName>, pkg: &PackageName) -> String {
    format!("({env:?}, {pkg})")
}

/// Generate a request for all environments in [envs] and all dependencies in [deps]. The request
/// contains one query for each `(env, dep)` pair where `env` is a key in [envs] (or [None]), and
/// `dep` is a dependency in [deps]. The key for the query is formed by [query_id].
fn create_request<F: MoveFlavor>(
    deps: &DependencySet<toml::Value>,
    envs: &BTreeMap<EnvironmentName, F::EnvironmentID>,
) -> Request {
    let mut request = Request::new(F::name());

    // request default resolution
    let mut default_reqs: BTreeMap<QueryID, Query> = deps
        .default_deps()
        .iter()
        .map(|(pkg_name, data)| create_query::<F>(None, pkg_name.clone(), data.clone()))
        .collect();
    request.queries.append(&mut default_reqs);

    // request env-specific resolutions
    for (env_name, env_id) in envs {
        let mut env_reqs = deps
            .deps_for_env(env_name)
            .into_iter()
            .map(|(pkg_name, data)| {
                create_query::<F>(
                    Some((env_name.clone(), env_id.clone())),
                    pkg_name,
                    data.clone(),
                )
            })
            .collect();
        request.queries.append(&mut env_reqs);
    }

    request
}

/// Output a query for [data] in environment [env]; [pkg] is used to generate the query name
fn create_query<F: MoveFlavor>(
    env: Option<(EnvironmentName, F::EnvironmentID)>,
    pkg: PackageName,
    data: toml::Value,
) -> (QueryID, Query) {
    let (env_name, env_id) = env.unzip();

    (
        query_id(&env_name, &pkg),
        Query {
            argument: data,
            environment_id: env_id.map(|it| it.to_string()),
        },
    )
}

/// Generate a dependency set `r` containing all of the dependencies from [response]. It should be
/// the case that for each environment `e` in [envs] (or [None]) and each dependency `d` in [deps]
/// that `e.deps_for_env(e)` returns the reponse from [response] for the key `(e, d)` (as a
/// precondition, [response] should contain all of these keys)
fn process_response<F: MoveFlavor>(
    resolver: &ResolverName,
    deps: &DependencySet<toml::Value>,
    envs: &BTreeMap<EnvironmentName, F::EnvironmentID>,
    response: Response,
) -> PackageResult<DependencySet<ManifestDependencyInfo<F>>> {
    let mut result: DependencySet<ManifestDependencyInfo<F>> = DependencySet::new();

    for env in once(None).chain(envs.iter().map(|(env_name, _)| Some(env_name))) {
        for (pkg_name, _) in deps.deps_for(env) {
            result.insert(
                env.cloned(),
                pkg_name.clone(),
                extract_query_result(resolver, &response, env.cloned(), pkg_name.clone())?,
            );
        }
    }

    Ok(result)
}

/// Extract the query result corresponding to ([env], [pkg_name]) from [response] and decode it as a
/// [ManifestDependencyInfo]. [resolver] is used for error handling and logging.
fn extract_query_result<F: MoveFlavor>(
    resolver: &ResolverName,
    response: &Response,
    env: Option<EnvironmentName>,
    pkg_name: PackageName,
) -> Result<ManifestDependencyInfo<F>, ResolverError> {
    let result = response
        .responses
        .get(&query_id(&env, &pkg_name))
        .expect("response has all keys");

    match result {
        QueryResult::Error { error } => Err(ResolverError::resolver_failed(
            resolver.clone(),
            pkg_name,
            env,
            error.clone(),
        )),
        QueryResult::Success { warnings, resolved } => {
            // TODO: use diagnostics here
            for warning in warnings {
                warn!("{resolver}: {warning}");
            }

            let result =
                ManifestDependencyInfo::<F>::deserialize(resolved.clone()).map_err(|_| {
                    ResolverError::bad_resolver(resolver, "incorrectly formatted dependency")
                })?;

            if let ManifestDependencyInfo::<F>::External(_) = result {
                Err(ResolverError::bad_resolver(
                    resolver,
                    "resolvers must return resolved dependencies",
                ))
            } else {
                Ok(result)
            }
        }
    }
}

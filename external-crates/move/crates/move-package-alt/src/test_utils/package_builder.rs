use std::{collections::BTreeMap, path::Path};

use heck::CamelCase;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use serde_spanned::Spanned;
use tracing::debug;

use crate::{
    flavor::{
        Vanilla,
        vanilla::{DEFAULT_ENV_ID, DEFAULT_ENV_NAME},
    },
    schema::{
        ImplicitDepMode, OriginalID, PackageMetadata, PackageName, ParsedManifest, Publication,
        PublicationFile, PublishAddresses, PublishedID, RenderToml,
    },
};

use super::ProjectBuilder;

/// A convenience type for building packages
pub struct PackageBuilder {
    manifest: ParsedManifest,

    /// Additional stuff to add to the manifest
    manifest_tail: String,
    pubs: PublicationFile<Vanilla>,
    local_pubs: PublicationFile<Vanilla>,
}

/// A wrapper for `PackageBuilder` that builds legacy packages
pub struct LegacyPackageBuilder {
    inner: PackageBuilder,
    published_at: Option<PublishedID>,
    self_address: OriginalID,

    /// The `name` field in the package; this is distinct from `self.inner.manifest.metadata.name`
    /// which holds the named address for the package.
    package_name: String,

    /// Additional named addreses for the `[addresses]` section
    extra_named_addresses: BTreeMap<Identifier, AccountAddress>,

    /// Whether to build a modern pubfile or a legacy lockfile
    modern_lockfile: bool,
}

impl LegacyPackageBuilder {
    /// Create a new legacy package; the manifest will have `package.name` set to a CamelCased
    /// version of `name`, and there will be a named address with value `name`. Published addresses
    /// will be in the automated address section `Move.lock`, while
    pub fn new(name: impl AsRef<str>) -> Self {
        Self {
            inner: PackageBuilder::new(&name),
            published_at: None,
            self_address: 0.into(),
            package_name: name.as_ref().to_camel_case(),
            extra_named_addresses: BTreeMap::new(),
            modern_lockfile: false,
        }
    }

    /// Set the `published-at` field in the manifest
    pub fn published_at(mut self, published_at: impl AsRef<str>) -> Self {
        self.published_at = Some(published_at.as_ref().try_into().unwrap());
        self
    }

    /// Set the value of the named address for this package
    pub fn original_id(mut self, original_id: impl AsRef<str>) -> Self {
        self.self_address = original_id.as_ref().try_into().unwrap();
        self
    }

    /// Set the `published-at` and the self address (does not touch the lockfile)
    pub fn publish(mut self, original_id: impl AsRef<str>, published_at: impl AsRef<str>) -> Self {
        self.published_at(published_at).original_id(original_id)
    }

    /// Add `<name> = <addr>` to the `[addresses]` table
    pub fn add_named_address(mut self, name: impl AsRef<str>, addr: impl AsRef<str>) -> Self {
        let old = self.extra_named_addresses.insert(
            Identifier::new(name.as_ref()).expect("valid identifier"),
            AccountAddress::from_hex(addr.as_ref()).expect("valid hex address"),
        );
        self
    }

    /// Add a publication to the lockfile or pubfile in the default environment
    pub fn publish_in_default_env(
        mut self,
        original_id: impl AsRef<str>,
        published_at: impl AsRef<str>,
        version: u16,
    ) -> Self {
        self.inner = self.inner.publish(original_id, published_at, version);
        self
    }

    /// Add a publication to the lockfile or pubfile
    pub fn publish_in_env(mut self, env: impl AsRef<str>, publish: Publication<Vanilla>) -> Self {
        self.inner = self.inner.publish_in_env(env, publish);
        self
    }

    /// Set this to generate a `Move.published` instead of a `Move.lock`. This situation will arise
    /// after building legacy packages using the modern system. Note that whether we have
    /// `modern_lockfile` or not, we always output local publications to `Move.published.local`
    pub fn modern_lockfile(mut self) -> Self {
        self.modern_lockfile = true;
        self
    }

    /// Append `text` to the generated manifest
    pub fn add_to_manifest(mut self, text: impl AsRef<str>) -> Self {
        self.inner = self.inner.add_to_manifest(text);
        self
    }

    /// Produce a legacy `Move.toml`, a modern `Move.published.local`, and either a `Move.lock` or
    /// a `Move.published` depending on `self.modern_lockfile`.
    pub fn generate(self, project: ProjectBuilder, path: impl AsRef<Path>) -> ProjectBuilder {
        todo!()
    }
}

impl PackageBuilder {
    /// Create a new empty package spec
    pub fn new(name: impl AsRef<str>) -> Self {
        let name = span(PackageName::new(name.as_ref()).expect("valid package name"));
        Self {
            manifest: ParsedManifest {
                package: PackageMetadata {
                    name,
                    edition: "2024".to_string(),
                    implicit_deps: ImplicitDepMode::Enabled(None),
                    unrecognized_fields: BTreeMap::new(),
                },
                environments: BTreeMap::new(),
                local_environments: BTreeMap::new(),
                dependencies: BTreeMap::new(),
                dep_replacements: BTreeMap::new(),
            },
            pubs: BTreeMap::new(),
            local_pubs: BTreeMap::new(),
            manifest_tail: String::new(),
        }
    }

    /// Add a published entry at version 0 in the default environment with the given `original_id`
    /// and `published_at` fields
    pub fn publish(
        mut self,
        original_id: impl AsRef<str>,
        published_at: impl AsRef<str>,
        version: u16,
    ) -> Self {
        self.publish_in_env(
            DEFAULT_ENV_NAME,
            make_pub(DEFAULT_ENV_ID, original_id, published_at, version),
        )
    }

    /// Add `publish` to the `[<env>]` section of `Move.published`
    pub fn publish_in_env(mut self, env: impl AsRef<str>, publish: Publication<Vanilla>) -> Self {
        let old = self.pubs.insert(env.as_ref().to_string(), publish);
        assert!(old.is_none(), "previous publication for {}", env.as_ref());
        self
    }

    /// Add `publish` to the `[<env>]` section of `Move.published.local`
    pub fn publish_local(mut self, env: impl AsRef<str>, publish: Publication<Vanilla>) -> Self {
        let old = self.local_pubs.insert(env.as_ref().to_string(), publish);
        assert!(old.is_none(), "previous local pub for {}", env.as_ref());
        self
    }

    /// Update the `name` field in the `[package]` section of the manifest
    pub fn package_name(&mut self, name: impl AsRef<str>) {
        self.manifest.package.name =
            span(PackageName::new(name.as_ref()).expect("valid package name"))
    }

    /// Add `<env> = <chain-id>` to the `[environments]` table
    pub fn add_environment(&mut self, env: impl AsRef<str>, chain: impl AsRef<str>) {
        let old = self.manifest.environments.insert(
            span(env.as_ref().to_string()),
            span(chain.as_ref().to_string()),
        );
        assert!(
            old.is_none(),
            "previous definition of environment {}",
            env.as_ref()
        );
    }

    /// Add `<local_env> = <base_env>` to the `[local-environments]` table
    pub fn add_local_env(mut self, local_env: impl AsRef<str>, base_env: impl AsRef<str>) -> Self {
        let old = self.manifest.local_environments.insert(
            local_env.as_ref().to_string(),
            base_env.as_ref().to_string(),
        );
        assert!(
            old.is_none(),
            "previous definition of local environment {}",
            local_env.as_ref()
        );
        self
    }

    /// Add `str` to the end of the generated manifest
    pub fn add_to_manifest(mut self, text: impl AsRef<str>) -> Self {
        self.manifest_tail.push('\n');
        self.manifest_tail.push_str(text.as_ref());
        self
    }

    /// Output `Move.toml`, `Move.published`, and `Move.published.local` to `path` in `builder`
    pub fn generate(&self, mut project: ProjectBuilder, path: impl AsRef<Path>) -> ProjectBuilder {
        let output = move |project: ProjectBuilder, name: &str, text: String| {
            let path = path.as_ref().join(name);
            debug!("generated {path:?}:\n{text}");
            project.file(path, &text)
        };

        // Move.toml
        let mut manifest = self.manifest.render_as_toml();
        manifest.push_str(&self.manifest_tail);
        project = output(project, "Move.toml", manifest);

        // Move.published
        if !self.pubs.is_empty() {
            project = output(project, "Move.published", self.pubs.render_as_toml());
        }

        // Move.published.local
        if !self.local_pubs.is_empty() {
            project = output(
                project,
                "Move.published.local",
                self.local_pubs.render_as_toml(),
            );
        }

        project
    }

    /// Return the contents of a legacy `Move.toml` file for the legacy package represented by
    /// `node`
    fn format_legacy_manifest(&self) -> String {
        let package = &self.inner[node];
        assert!(package.is_legacy);

        assert!(
            package.pubs.len() <= 1,
            "legacy packages may have at most one publication"
        );
        let publication = package
            .pubs
            .first_key_value()
            .map(|(env, publication)| publication);

        let published_at = publication
            .map(|it| format!("published-at = {}", it.addresses.published_at))
            .unwrap_or_default();

        let mut move_toml = formatdoc!(
            r#"
        [package]
        name = "{}"
        edition = "2024"
        {published_at}
        "#,
            package.id.to_camel_case()
        );

        let mut deps = String::from("\n[dependencies]\n");
        for edge in self.inner.edges(node) {
            let dep_spec = edge.weight();
            let dep_str = self.format_legacy_dep(edge.weight(), &self.inner[edge.target()]);
            deps.push_str(&dep_str);
            deps.push('\n');
        }
        move_toml.push_str(&deps);
        move_toml.push('\n');

        move_toml.push_str(&formatdoc!(
            r#"
        [addresses]
        {} = "{}"
        "#,
            package.name,
            publication
                .map(|it| it.addresses.original_id.to_string())
                .unwrap_or("0x0".to_string())
        ));

        move_toml.push_str(&package.manifest_tail);
        move_toml
    }

    /// Return the contents of a `Move.published` file containing publications `pubs`
    fn format_pub_file(&self, pubs: &PublicationFile<Vanilla>) -> Option<String> {
        if pubs.is_empty() {
            return None;
        }

        let mut pub_file = String::new();

        for (env, publication) in pubs.iter() {
            let PubSpec {
                chain_id,
                addresses:
                    PublishAddresses {
                        original_id,
                        published_at,
                    },
            } = publication;

            pub_file.push_str(&formatdoc!(
                r#"
                [published.{env}]
                published-at = "{published_at}"
                original-id = "{original_id}"
                chain-id = "{DEFAULT_ENV_ID}"
                toolchain-version = "test-0.0.0"
                build-config = {{}}

                "#,
            ));
        }

        Some(pub_file)
    }

    /// Return the contents of the `Move.published` file for the package represented by
    /// `node`.
    fn format_published(&self, node: NodeIndex) -> Option<String> {
        self.format_pub_file(&self.inner[node].pubs)
    }

    /// Return the contents of the `Move.published.local` file for the package represented by
    /// `node`.
    fn format_local_pubs(&self, node: NodeIndex) -> Option<String> {
        self.format_pub_file(&self.inner[node].local_pubs)
    }
}

/// Convenience method for adding a dummy span to something
fn span<T>(value: T) -> Spanned<T> {
    Spanned::new((0..0), value)
}

/// Convenience method to wrap up the fields of a `Publication`
fn make_pub(
    chain_id: impl AsRef<str>,
    original_id: impl AsRef<str>,
    published_at: impl AsRef<str>,
    version: u16,
) -> Publication<Vanilla> {
    Publication {
        addresses: PublishAddresses {
            original_id: original_id.as_ref().try_into().expect("valid hex address"),
            published_at: published_at.as_ref().try_into().expect("valid hex address"),
        },
        chain_id: chain_id.as_ref().to_string(),
        metadata: (),
        version: version.to_string(),
    }
}

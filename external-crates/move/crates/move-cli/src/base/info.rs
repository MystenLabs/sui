// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// use anyhow::Result;
// use clap::Parser;
// use std::path::Path;
//
// use crate::base::{find_env, reroot_path};
//
// use move_core_types::identifier::Identifier;
// use move_package_alt::{
//     flavor::MoveFlavor,
//     package::{Package, RootPackage},
// };
// use move_package_alt_compilation::build_config::BuildConfig;
// use treeline::Tree;
//
// /// Print address information.
// #[derive(Parser)]
// #[clap(name = "info")]
// pub struct Info;
//
// impl Info {
//     pub async fn execute<F: MoveFlavor>(
//         self,
//         path: Option<&Path>,
//         config: BuildConfig,
//     ) -> anyhow::Result<()> {
//         let path = reroot_path(path)?;
//         let env = find_env::<F>(&path, &config)?;
//         let pkg = RootPackage::<F>::load(path, env).await?;
//         print_info(pkg);
//         Ok(())
//     }
// }
//
// fn print_info_dfs<F: MoveFlavor>(
//     pkg: &Package<F>,
//     current_node: &Identifier,
//     tree: &mut Tree<String>,
// ) -> Result<()> {
//     // TODO: fix this with the current package graph.
//
//     // for (name, addr) in &pkg.resolved_table {
//     //     tree.push(Tree::root(format!(
//     //         "{}:0x{}",
//     //         name,
//     //         addr.short_str_lossless()
//     //     )));
//     // }
//     //
//     // for dep in pkg.immediate_dependencies() {
//     //     let mut child = Tree::root(dep.to_string());
//     //     print_info_dfs(root_pkg, &dep, &mut child)?;
//     //     tree.push(child);
//     // }
//
//     Ok(())
// }
//
// fn print_info<F: MoveFlavor>(pkg: RootPackage<F>) -> Result<()> {
//     let root_name = pkg.name();
//     let root_pkg = pkg.package_graph().root_package();
//     let mut tree = Tree::root(root_name.to_string());
//     print_info_dfs(root_pkg, root_name, &mut tree)?;
//     println!("{}", tree);
//     Ok(())
// }

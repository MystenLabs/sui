use crate::{flavor::Vanilla, graph::PackageGraph};

pub struct TestPackageGraph;
pub struct NodeBuilder;
pub struct EdgeBuilder;
pub struct Scenario;

impl TestPackageGraph {
    pub fn new(nodes: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        todo!()
    }

    pub fn add_deps(
        self,
        edges: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>,
    ) -> Self {
        todo!();
        self
    }

    pub fn add_node(
        self,
        node: impl AsRef<str>,
        build: impl FnOnce(NodeBuilder) -> NodeBuilder,
    ) -> Self {
        todo!();
        self
    }

    pub fn add_dep(
        self,
        source: impl AsRef<str>,
        target: impl AsRef<str>,
        build: impl FnOnce(EdgeBuilder) -> EdgeBuilder,
    ) -> Self {
        todo!();
        self
    }

    pub fn build(self) -> Scenario {
        todo!()
    }
}

impl NodeBuilder {
    pub fn original_id(self, addr: u64) -> Self {
        todo!();
        self
    }

    pub fn published_at(self, addr: u64) -> Self {
        todo!();
        self
    }
}

impl EdgeBuilder {
    pub fn set_override(self) -> Self {
        todo!();
        self
    }
}

impl Scenario {
    pub fn graph_for(&self, root: impl AsRef<str>) -> PackageGraph<Vanilla> {
        todo!()
    }
}

use move_binary_format::CompiledModule;

#[derive(Clone, Copy, Debug)]
pub enum BinaryIndexedView<'a> {
    Module(&'a CompiledModule),
}
impl<'a> BinaryIndexedView<'a> {
    pub(crate) fn version(&self) -> u32 {
        match self {
            BinaryIndexedView::Module(module) => module.version(),
        }
    }

    pub(crate) fn module(&self) -> &'a CompiledModule {
        match self {
            BinaryIndexedView::Module(module) => module,
        }
    }
}

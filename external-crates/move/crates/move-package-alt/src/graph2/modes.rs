use crate::{MoveFlavor, graph2::PackageGraph, schema::ModeName};

impl<F: MoveFlavor> PackageGraph<'_, F> {
    pub fn filter_for_mode(self, modes: &Vec<ModeName>) -> Self {
        todo!()
    }
}

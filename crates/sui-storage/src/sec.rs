use typed_store::Map;

use crate::indexes::IndexStoreTables;
use std::path::PathBuf;

pub fn follow_index_table(path: PathBuf) {
    let index_store_read_only_handle =
        IndexStoreTables::get_read_only_handle(db_path, None, None, MetricConf::default());

    index_store_read_only_handle
        .owner_index
        .try_catch_up_with_primary();

    let count = 10;
    let it = index_store_read_only_handle.owner_index.iter();

    while count > 0 {
        println!("{:?}", it.next());
    }
}

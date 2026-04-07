use jj_lib::repo::StoreFactories;

use crate::stores::{RedbBackend, RedbOpHeadsStore, RedbOpStore};

pub mod stores;

pub fn store_factories() -> StoreFactories {
    let mut store_factories = StoreFactories::empty();
    store_factories.add_op_store(
        RedbOpStore::name(),
        Box::new(|_, path, root_data| Ok(Box::new(RedbOpStore::open(path, root_data)))),
    );

    store_factories.add_op_heads_store(
        RedbOpHeadsStore::name(),
        Box::new(|_, path| Ok(Box::new(RedbOpHeadsStore::open(path)))),
    );

    store_factories.add_backend(
        RedbBackend::name(),
        Box::new(|_, path| Ok(Box::new(RedbBackend::open(path)))),
    );

    store_factories
}

use super::*;
use eyre::Result;
use store::{rocks::DBMap, Map};

/// The representation of the DAG in memory.
type Dag = DBMap<KeyAtRound, (Digest, Certificate)>;

pub struct PersistentDag {
    disk_dag: Dag,
    mem_dag: HashMap<KeyAtRound, (Digest, Certificate)>,
}

impl PersistentDag {
    pub fn open<P: AsRef<Path>>(
        path: P,
        db_options: Option<rocksdb::Options>,
        opt_cf: Option<String>,
    ) -> Result<Self> {
        let dag = Dag::open(path, db_options, opt_cf)?;
        Ok(PersistentDag {
            disk_dag: dag,
            mem_dag: HashMap::new(),
        })
    }

    pub(super) fn insert_batch<T: Iterator<Item = (KeyAtRound, (Digest, Certificate))>>(
        &mut self,
        batch: T,
    ) -> Result<()> {
        let mut batch_copy: Vec<(KeyAtRound, (Digest, Certificate))> = Vec::new();
        let side_effect = batch.map(|(key, cert)| {
            batch_copy.push((key.clone(), cert.clone()));
            (key, cert)
        });

        self.disk_dag.batch().insert_batch(side_effect)?.write()?;
        batch_copy.into_iter().for_each(|(key, cert)| {
            self.mem_dag.insert(key, cert);
        });

        Ok(())
    }

    pub(super) fn delete_batch<T: Iterator<Item = KeyAtRound>>(&mut self, batch: T) -> Result<()> {
        let mut batch_copy: Vec<KeyAtRound> = Vec::new();
        let side_effect = batch.map(|key| {
            batch_copy.push(key.clone());
            key
        });

        self.disk_dag.batch().delete_batch(side_effect)?.write()?;
        batch_copy.into_iter().for_each(|key| {
            self.mem_dag.remove(&key);
        });

        Ok(())
    }

    pub(super) fn delete_range(&mut self, from: &KeyAtRound, to: &KeyAtRound) -> Result<()> {
        self.disk_dag.batch().delete_range(from, to)?.write()?;
        self.mem_dag.retain(|k, _v| k < from || k >= to);
        Ok(())
    }

    pub(super) fn insert(&mut self, key: &KeyAtRound, value: &(Digest, Certificate)) -> Result<()> {
        self.disk_dag.insert(key, value)?;
        self.mem_dag.insert(key.clone(), value.clone());
        Ok(())
    }

    // The rest is read-only

    pub(super) fn get(&self, key: &KeyAtRound) -> Option<&(Digest, Certificate)> {
        self.mem_dag.get(key)
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = &KeyAtRound> {
        self.mem_dag.keys()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&KeyAtRound, &(Digest, Certificate))> {
        self.mem_dag.iter()
    }
}

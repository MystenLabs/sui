use sui_pg_db::Db;

struct ObjectStore {
    db: Db,
    object_store: Arc<Box<dyn object_store::ObjectStore>>,
    compression_level: Option<i32>,
}

pub struct ObjectStoreConnection<'c> {
    conn: PgConnection<'c>,
    object_store: Arc<Box<dyn object_store::ObjectStore>>,
    compression_level: Option<i32>,
}

impl ObjectStore {
    pub fn new(
        db: Db,
        object_store: Box<dyn object_store::ObjectStore>,
        compression_level: Option<i32>,
    ) -> Self {
        Self {
            db,
            object_store: Arc::new(object_store),
            compression_level,
        }
    }
}

impl ObjectStoreConnection<'_> {
    pub async fn write(
        &self,
        path: impl Into<object_store::path::Path>,
        data: impl AsRef<[u8]>,
    ) -> anyhow::Result<()> {
        let mut path = path.into();

        let blob: object_store::PutPayload = if let Some(level) = self.compression_level {
            let path_str = format!("{}.zst", path);
            path = path_str.into();

            let compressed = tokio::task::spawn_blocking({
                let data = data.as_ref().to_vec();
                move || zstd::encode_all(&data[..], level)
            })
            .await??;

            Bytes::from(compressed).into()
        } else {
            Bytes::from(data.as_ref().to_vec()).into()
        };

        self.object_store.put(&path, blob.clone()).await?;
        Ok(())
    }
}

impl<'c> std::ops::Deref for ObjectStoreConnection<'c> {
    type Target = PgConnection<'c>;

    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl std::ops::DerefMut for ObjectStoreConnection<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn
    }
}

#[async_trait]
impl Connection for ObjectStoreConnection<'_> {
    async fn committer_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        self.conn.committer_watermark(pipeline).await
    }

    async fn reader_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<ReaderWatermark>> {
        self.conn.reader_watermark(pipeline).await
    }

    async fn pruner_watermark(
        &mut self,
        pipeline: &'static str,
        delay: Duration,
    ) -> anyhow::Result<Option<PrunerWatermark>> {
        self.conn.pruner_watermark(pipeline, delay).await
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
        self.conn.set_committer_watermark(pipeline, watermark).await
    }

    async fn set_reader_watermark(
        &mut self,
        pipeline: &'static str,
        reader_lo: u64,
    ) -> anyhow::Result<bool> {
        self.conn.set_reader_watermark(pipeline, reader_lo).await
    }

    async fn set_pruner_watermark(
        &mut self,
        pipeline: &'static str,
        pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        self.conn.set_pruner_watermark(pipeline, pruner_hi).await
    }
}

#[async_trait]
impl Store for ObjectStore {
    type Connection<'c> = ObjectStoreConnection<'c>;

    async fn connect<'c>(&'c self) -> anyhow::Result<Self::Connection<'c>> {
        Ok(ObjectStoreConnection {
            conn: self.db.connect().await?,
            object_store: self.object_store.clone(),
            compression_level: self.compression_level,
        })
    }
}

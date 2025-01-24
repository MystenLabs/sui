use crate::{BoxExEx, BoxedLaunchExEx, ExExContext};
use futures::future::BoxFuture;

// Helper function to create a boxed ExEx
pub fn box_exex<F, Fut>(f: F) -> Box<dyn BoxedLaunchExEx>
where
    F: FnOnce(ExExContext) -> Fut + Send + Sync + 'static,
    Fut: futures::Future<Output = anyhow::Result<()>> + Send + 'static,
{
    Box::new(move |ctx| {
        Box::pin(async move { Ok(Box::pin(f(ctx)) as BoxExEx) })
            as BoxFuture<'static, anyhow::Result<BoxExEx>>
    })
}

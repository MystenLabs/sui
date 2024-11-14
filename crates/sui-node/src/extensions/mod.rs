pub mod hello;

use futures::future::BoxFuture;
use sui_exex::{BoxExEx, BoxedLaunchExEx, ExExContext};

use hello::exex_hello;

// Helper function to create a boxed ExEx
fn box_exex<F, Fut>(f: F) -> Box<dyn BoxedLaunchExEx>
where
    F: FnOnce(ExExContext) -> Fut + Send + Sync + 'static,
    Fut: futures::Future<Output = anyhow::Result<()>> + Send + 'static,
{
    Box::new(move |ctx| {
        Box::pin(async move { Ok(Box::pin(f(ctx)) as BoxExEx) })
            as BoxFuture<'static, anyhow::Result<BoxExEx>>
    })
}

/// List of all ExEx that will be ran.
pub fn sui_exexes() -> Vec<(String, Box<dyn BoxedLaunchExEx>)> {
    vec![(String::from("Hello"), box_exex(exex_hello))]
}

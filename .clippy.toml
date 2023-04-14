too-many-arguments-threshold = 20
disallowed-methods = [
    # we use tracing with the log feature instead of the log crate.
    { path = "log::info", reason = "use tracing::info instead" },
    { path = "log::debug", reason = "use tracing::debug instead" },
    { path = "log::error", reason = "use tracing::error instead" },
    { path = "log::warn", reason = "use tracing::warn instead" },
    # unbounded channels are for expert use only
    { path = "tokio::sync::mpsc::unbounded_channel", reason = "use a bounded channel instead" },
    { path = "futures::channel::mpsc::unbounded", reason = "use a bounded channel instead" },
    { path = "futures_channel::mpsc::unbounded", reason = "use a bounded channel instead" },
    # known to cause blocking issues
    { path = "futures::executor::block_on", reason = "use tokio::runtime::runtime::Runtime::block_on instead"},
    # bincode::deserialize_from is easy to shoot your foot with
    { path = "bincode::deserialize_from", reason = "use bincode::deserialize instead" },
]

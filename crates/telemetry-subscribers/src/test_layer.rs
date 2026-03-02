// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::Mutex;
use tracing::field;
use tracing_subscriber::Layer;
use tracing_subscriber::layer;

// Captures log events for tests.
#[derive(Clone, Default)]
pub struct TestLayer {
    events: Arc<Mutex<Vec<String>>>,
}

impl TestLayer {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn get_events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

impl<S> Layer<S> for TestLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: layer::Context<'_, S>) {
        let mut visitor = TestVisitor::default();
        event.record(&mut visitor);

        let level = event.metadata().level();
        let message = format!("[{}] {}", level, visitor.message);

        self.events.lock().unwrap().push(message);
    }
}

#[derive(Default)]
struct TestVisitor {
    message: String,
}

impl field::Visit for TestVisitor {
    fn record_debug(&mut self, field: &field::Field, value: &dyn std::fmt::Debug) {
        let space = if self.message.is_empty() { "" } else { " " };
        if field.name() == "message" {
            self.message += &format!("{space}{value:?}");
        } else {
            self.message += &format!("{space}{field}={value:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use tracing::error;
    use tracing::info;
    use tracing::warn;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    #[test]
    fn test_layer() {
        let test_layer = TestLayer::new();

        // Set up the subscriber with our test layer
        let subscriber = tracing_subscriber::registry().with(test_layer.clone());

        tracing::subscriber::with_default(subscriber, || {
            info!("info-message");
            info!(field1=%"value1", "info-message");
            info!(field1=%"value1", field2=%"value2", "info-message");
            warn!("warn-message");
            error!("error-message");
        });

        let events = test_layer.get_events();

        assert_eq!(events.len(), 5);
        assert_eq!(events[0], "[INFO] info-message");
        assert_eq!(events[1], "[INFO] info-message field1=value1");
        assert_eq!(events[2], "[INFO] info-message field1=value1 field2=value2");
        assert_eq!(events[3], "[WARN] warn-message");
        assert_eq!(events[4], "[ERROR] error-message");
    }
}

use std::collections::HashMap;
use tracing::warn;

pub fn initialize_storage_failpoints() {
    let mut failpoints: HashMap<String, String> = HashMap::new();
    failpoints.insert(
        String::from("certificate-store"),
        String::from(".05%return"),
    );

    if fail::has_failpoints() {
        warn!("Failpoints are enabled");
        for (point, actions) in failpoints {
            fail::cfg(point, &actions).expect("failed to set actions for storage failpoints");
        }
    } else {
        warn!("Storage failpoints are not enabled");
    }
}

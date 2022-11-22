#[macro_export]
macro_rules! await_and_retry {
    ($future: expr, $retrial_number: expr, $start_interval: expr, $err_msg: expr) => {
        let mut retrial_count = 0usize;
        let mut interval = $start_interval;

        loop {
            if retrial_count == $retrial_number {
                return Err(anyhow::Error(
                    $err_msg.into(),
                ))
            }

            match $future.await {
                Ok(res) => break Ok(res),
                Err(err) => {
                    // TODOggao: format with err_msg + err + count
                    warn!("Err happened executing future, retrying...");
                    retrial_count += 1;
                    interval *= 2;
                }
            }
        }
    }
}
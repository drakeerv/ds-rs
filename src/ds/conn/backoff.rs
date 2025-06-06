use std::future::Future;
use std::time::Duration;
use tokio::time;

pub struct ExponentialBackoff {
    attempt: u32,
    max_timeout: Duration,
    use_max: bool,
    timeout: Option<Duration>,
}

impl ExponentialBackoff {
    #[inline(always)]
    pub const fn new(max_timeout: Duration) -> ExponentialBackoff {
        ExponentialBackoff {
            attempt: 0,
            max_timeout,
            use_max: false,
            timeout: None,
        }
    }

    pub async fn run<O, E>(
        &mut self,
        fut: impl Future<Output = Result<O, E>>,
    ) -> Result<O, (E, bool)> {
        if let Some(timeout) = self.timeout {
            println!("Backoff: waiting {:?}", timeout);
            time::sleep(timeout).await;
        }
        match fut.await {
            Ok(out) => {
                self.reset();
                Ok(out)
            }
            Err(e) => {
                let disconnected = self.attempt == 0;
                self.calculate_wait();
                self.attempt += 1;
                Err((e, disconnected))
            }
        }
    }

    pub fn reset(&mut self) {
        self.use_max = false;
        self.attempt = 0;
        self.timeout = None;
    }

    fn calculate_wait(&mut self) {
        // Short circuit path if we're already at the point of reaching the max timeout
        if self.use_max {
            return;
        }

        let backoff_millis = 20u64.pow(self.attempt);

        let delay = Duration::from_millis(backoff_millis).min(self.max_timeout);
        if delay == self.max_timeout {
            self.use_max = true;
        }

        self.timeout = Some(delay);
    }
}

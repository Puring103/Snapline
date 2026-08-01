use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct LoginLimiter {
    attempts: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    max_attempts: usize,
    window: Duration,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::new(5, Duration::from_secs(15 * 60))
    }
}

impl LoginLimiter {
    pub fn new(max_attempts: usize, window: Duration) -> Self {
        Self {
            attempts: Arc::new(Mutex::new(HashMap::new())),
            max_attempts,
            window,
        }
    }

    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut attempts = self.attempts.lock().expect("login limiter mutex poisoned");
        let entries = attempts.entry(key.to_owned()).or_default();
        entries.retain(|attempt| now.duration_since(*attempt) < self.window);
        entries.len() < self.max_attempts
    }

    pub fn failure(&self, key: &str) {
        self.attempts
            .lock()
            .expect("login limiter mutex poisoned")
            .entry(key.to_owned())
            .or_default()
            .push(Instant::now());
    }

    pub fn success(&self, key: &str) {
        self.attempts
            .lock()
            .expect("login limiter mutex poisoned")
            .remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_blocks_after_threshold_and_success_resets() {
        let limiter = LoginLimiter::new(2, Duration::from_secs(60));
        assert!(limiter.check("person@example.com"));
        limiter.failure("person@example.com");
        assert!(limiter.check("person@example.com"));
        limiter.failure("person@example.com");
        assert!(!limiter.check("person@example.com"));
        limiter.success("person@example.com");
        assert!(limiter.check("person@example.com"));
    }
}

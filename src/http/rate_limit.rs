//! A fixed-window rate limiter keyed by client address.
//!
//! Each address may record `limit` events per window; the count resets when
//! the window ends. Coarser than a token bucket, but simple to reason about
//! and plenty for keeping one client from hogging a small server.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Above this many tracked addresses, expired windows are pruned.
const MAX_TRACKED: usize = 10_000;

const MINUTE: Duration = Duration::from_secs(60);

pub struct RateLimiter {
    /// Events allowed per window; 0 means unlimited.
    limit: u32,
    window: Duration,
    windows: Mutex<HashMap<IpAddr, Window>>,
}

struct Window {
    started: Instant,
    count: u32,
}

impl RateLimiter {
    pub fn per_minute(limit: u32) -> Self {
        Self {
            limit,
            window: MINUTE,
            windows: Mutex::new(HashMap::new()),
        }
    }

    /// Records an event for `ip`. Returns `false`, without recording, if
    /// `ip` has already used up its limit for the current window.
    pub fn hit(&self, ip: IpAddr) -> bool {
        if self.limit == 0 {
            return true;
        }
        let now = Instant::now();
        let mut windows = self
            .windows
            .lock()
            .expect("rate limiter lock is never poisoned");
        if windows.len() >= MAX_TRACKED {
            windows.retain(|_, window| now.duration_since(window.started) < self.window);
        }
        let window = windows.entry(ip).or_insert(Window {
            started: now,
            count: 0,
        });
        if now.duration_since(window.started) >= self.window {
            *window = Window {
                started: now,
                count: 0,
            };
        }
        if window.count >= self.limit {
            return false;
        }
        window.count += 1;
        true
    }

    /// Whether `ip` has used up its limit for the current window.
    pub fn is_limited(&self, ip: IpAddr) -> bool {
        if self.limit == 0 {
            return false;
        }
        let windows = self
            .windows
            .lock()
            .expect("rate limiter lock is never poisoned");
        windows.get(&ip).is_some_and(|window| {
            window.started.elapsed() < self.window && window.count >= self.limit
        })
    }
}

/// `429 Too Many Requests`, telling the client to wait out the window.
pub fn too_many_requests() -> Response {
    let mut response = (StatusCode::TOO_MANY_REQUESTS, "too many requests").into_response();
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from(MINUTE.as_secs()));
    response
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    const A: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    const B: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

    #[test]
    fn allows_the_limit_per_address() {
        let limiter = RateLimiter::per_minute(2);
        assert!(!limiter.is_limited(A));
        assert!(limiter.hit(A));
        assert!(limiter.hit(A));
        assert!(limiter.is_limited(A));
        assert!(!limiter.hit(A));
        assert!(limiter.hit(B));
    }

    #[test]
    fn zero_means_unlimited() {
        let limiter = RateLimiter::per_minute(0);
        for _ in 0..100 {
            assert!(limiter.hit(A));
        }
        assert!(!limiter.is_limited(A));
    }

    #[test]
    fn a_new_window_resets_the_count() {
        let limiter = RateLimiter {
            limit: 1,
            window: Duration::from_millis(20),
            windows: Mutex::new(HashMap::new()),
        };
        assert!(limiter.hit(A));
        assert!(!limiter.hit(A));
        std::thread::sleep(Duration::from_millis(25));
        assert!(!limiter.is_limited(A));
        assert!(limiter.hit(A));
    }
}

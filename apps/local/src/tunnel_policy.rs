//! Pure terminal-response policy shared by the tunnel runtime and fuzz targets.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PostClass {
    Accepted,
    Terminal,
    Retry,
    Auth,
    Fail,
}

pub(crate) fn classify_post(status: u16) -> PostClass {
    match status {
        200 => PostClass::Accepted,
        404 => PostClass::Terminal,
        401 | 403 => PostClass::Auth,
        408 | 429 | 502 | 503 | 504 => PostClass::Retry,
        _ => PostClass::Fail,
    }
}

pub(crate) fn backoff_base_ms(attempt: u32) -> u64 {
    let shift = attempt.saturating_sub(1).min(6);
    200_u64.saturating_mul(1_u64 << shift).min(10_000)
}

pub(crate) fn jittered_backoff_ms(attempt: u32, entropy: u64) -> u64 {
    let base = backoff_base_ms(attempt);
    let jitter = if base <= 200 {
        0
    } else {
        entropy % (base / 4 + 1)
    };
    base.saturating_add(jitter).min(10_000)
}

pub(crate) fn retry_after_ms(value: &str, now_unix_seconds: i64) -> Option<u64> {
    const MAX_RETRY_AFTER_MS: u64 = 60_000;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    if value.bytes().all(|byte| byte.is_ascii_digit()) {
        let seconds = value.parse::<u64>().ok()?;
        return Some(seconds.saturating_mul(1_000).min(MAX_RETRY_AFTER_MS));
    }

    let target = chrono::DateTime::parse_from_rfc2822(value)
        .ok()?
        .timestamp();
    let seconds = target.checked_sub(now_unix_seconds)?;
    if seconds <= 0 {
        return None;
    }
    Some(
        u64::try_from(seconds)
            .ok()?
            .saturating_mul(1_000)
            .min(MAX_RETRY_AFTER_MS),
    )
}

pub(crate) fn retry_delay_ms(attempt: u32, entropy: u64, retry_after: Option<u64>) -> u64 {
    jittered_backoff_ms(attempt, entropy)
        .max(retry_after.unwrap_or(0))
        .min(60_000)
}

#[cfg(test)]
// Test fixtures intentionally fail fast and use compact tuple snapshots.
#[allow(clippy::unwrap_used, clippy::type_complexity)]
mod tests {
    use super::*;

    #[test]
    fn post_classes_match_protocol() {
        assert_eq!(classify_post(200), PostClass::Accepted);
        assert_eq!(classify_post(404), PostClass::Terminal);
        for status in [408, 429, 502, 503, 504] {
            assert_eq!(classify_post(status), PostClass::Retry);
        }
        for status in [401, 403] {
            assert_eq!(classify_post(status), PostClass::Auth);
        }
        for status in [0, 201, 400, 500, u16::MAX] {
            assert_eq!(classify_post(status), PostClass::Fail);
        }
    }

    #[test]
    fn backoff_base_is_bounded_and_saturates() {
        assert_eq!(backoff_base_ms(0), 200);
        assert_eq!(backoff_base_ms(1), 200);
        assert_eq!(backoff_base_ms(2), 400);
        assert_eq!(backoff_base_ms(6), 6_400);
        assert_eq!(backoff_base_ms(7), 10_000);
        assert_eq!(backoff_base_ms(u32::MAX), 10_000);
    }

    #[test]
    fn jitter_is_bounded_and_retry_after_is_a_minimum() {
        assert_eq!(jittered_backoff_ms(1, 999), 200);
        let jittered = jittered_backoff_ms(3, 17);
        assert!((800..=1_000).contains(&jittered));
        assert_eq!(retry_delay_ms(1, 0, Some(5_000)), 5_000);
        assert_eq!(retry_delay_ms(u32::MAX, u64::MAX, Some(60_000)), 60_000);
    }

    #[test]
    fn retry_after_accepts_delta_and_http_date_with_cap() {
        assert_eq!(retry_after_ms("5", 0), Some(5_000));
        assert_eq!(retry_after_ms("999999", 0), Some(60_000));
        assert_eq!(
            retry_after_ms("Sun, 06 Nov 1994 08:49:37 GMT", 784_111_767),
            Some(10_000)
        );
        assert_eq!(
            retry_after_ms("Sun, 06 Nov 1994 08:49:37 GMT", 784_111_777),
            None
        );
        for invalid in ["", "-1", "1.5", "tomorrow"] {
            assert_eq!(retry_after_ms(invalid, 0), None);
        }
    }
}

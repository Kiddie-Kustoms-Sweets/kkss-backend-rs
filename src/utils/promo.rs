use chrono::{DateTime, Utc};

/// 判断当前时间是否在 5/4-5/11 活动期内
pub fn is_in_promo_period(now: DateTime<Utc>) -> bool {
    let start = chrono::DateTime::parse_from_rfc3339("2026-05-04T00:00:00-04:00")
        .unwrap()
        .with_timezone(&Utc);
    let end = chrono::DateTime::parse_from_rfc3339("2026-05-11T23:59:59-04:00")
        .unwrap()
        .with_timezone(&Utc);
    now >= start && now <= end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_in_promo_period() {
        // 活动期间内
        let during = chrono::DateTime::parse_from_rfc3339("2026-05-06T12:00:00-04:00")
            .unwrap()
            .with_timezone(&Utc);
        assert!(is_in_promo_period(during));

        // 活动开始当天
        let start = chrono::DateTime::parse_from_rfc3339("2026-05-04T00:00:00-04:00")
            .unwrap()
            .with_timezone(&Utc);
        assert!(is_in_promo_period(start));

        // 活动结束当天
        let end = chrono::DateTime::parse_from_rfc3339("2026-05-11T23:59:59-04:00")
            .unwrap()
            .with_timezone(&Utc);
        assert!(is_in_promo_period(end));

        // 活动前
        let before = chrono::DateTime::parse_from_rfc3339("2026-05-03T23:59:59-04:00")
            .unwrap()
            .with_timezone(&Utc);
        assert!(!is_in_promo_period(before));

        // 活动后
        let after = chrono::DateTime::parse_from_rfc3339("2026-05-12T00:00:00-04:00")
            .unwrap()
            .with_timezone(&Utc);
        assert!(!is_in_promo_period(after));
    }
}

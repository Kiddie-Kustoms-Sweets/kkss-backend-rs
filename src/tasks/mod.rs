//! Background scheduled tasks for the application.
//!
//! This module centralizes all recurring background jobs (syncing orders/discount codes,
//! membership expiration checks, birthday rewards, and monthly card coupons).
//! Call `spawn_all` once during startup to launch them.

use crate::services::{BirthdayRewardService, DiscountCodeService, MembershipService, MonthlyCardService, SyncService};

/// Spawn all background tasks.
///
/// Notes
/// - Each task is idempotent as implemented in its service and runs on its own schedule.
/// - This function detaches tasks via `tokio::spawn`; it does not block.
pub fn spawn_all(
    sync_service: SyncService,
    membership_service: MembershipService,
    birthday_reward_service: BirthdayRewardService,
    monthly_card_service: MonthlyCardService,
    discount_code_service: DiscountCodeService,
) {
    // 每分钟同步最近一月订单与优惠码
    {
        let sync_service_clone = sync_service.clone();
        tokio::spawn(async move {
            use chrono::{Duration, Utc};
            loop {
                let now = Utc::now();
                let start = now - Duration::days(30);
                let start_date = start.format("%Y-%m-%d %H:%M:%S").to_string();
                let end_date = format!("{} 23:59:59", now.format("%Y-%m-%d"));

                log::debug!("Start syncing orders and discount codes: {start_date} ~ {end_date}");
                if let Err(e) = sync_service_clone.sync_orders(&start_date, &end_date).await {
                    log::error!("Failed to sync orders: {e:?}");
                }
                if let Err(e) = sync_service_clone.sync_discount_codes().await {
                    log::error!("Failed to sync discount codes: {e:?}");
                }
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
        });
    }

    // 会员过期检查（每 6 小时）
    {
        let svc = membership_service.clone();
        tokio::spawn(async move {
            loop {
                match svc.expire_memberships().await {
                    Ok(n) if n > 0 => log::info!("Expired memberships processed: {n}"),
                    Ok(_) => {}
                    Err(e) => log::error!("Failed to expire memberships: {e:?}"),
                }
                tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
            }
        });
    }

    // 生日福利发放（每小时）
    {
        let svc = birthday_reward_service.clone();
        tokio::spawn(async move {
            loop {
                match svc.grant_today_birthdays().await {
                    Ok(n) if n > 0 => log::info!("Birthday rewards granted: {n}"),
                    Ok(_) => {}
                    Err(e) => log::error!("Failed to grant birthday rewards: {e:?}"),
                }
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });
    }

    // 月卡每日优惠券发放（每天一次）
    {
        let svc = monthly_card_service.clone();
        tokio::spawn(async move {
            loop {
                match svc.grant_daily_coupons().await {
                    Ok(n) if n > 0 => log::info!("Monthly card daily coupons granted: {n}"),
                    Ok(_) => {}
                    Err(e) => log::error!("Failed to grant monthly card daily coupons: {e:?}"),
                }
                tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
            }
        });
    }

    // 清理过期的注册活动百分比优惠券（每 6 小时）
    {
        let svc = discount_code_service.clone();
        tokio::spawn(async move {
            loop {
                match svc.cleanup_expired_registration_rewards().await {
                    Ok(n) if n > 0 => log::info!("Cleaned up expired registration rewards: {n}"),
                    Ok(_) => {}
                    Err(e) => log::error!("Failed to cleanup expired registration rewards: {e:?}"),
                }
                tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
            }
        });
    }
}

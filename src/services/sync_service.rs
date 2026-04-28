use crate::entities::{
    MemberType, discount_code_entity as discount_codes, lucky_draw_chance_entity as chances,
    order_entity as orders, sweet_cash_transaction_entity as sct, user_entity as users,
};
use crate::error::AppResult;
use crate::external::*;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    Set, TransactionTrait,
};

#[derive(Clone)]
pub struct SyncService {
    pool: DatabaseConnection,
    sevencloud_api: std::sync::Arc<tokio::sync::Mutex<SevenCloudAPI>>,
}

impl SyncService {
    pub fn new(
        pool: DatabaseConnection,
        sevencloud_api: std::sync::Arc<tokio::sync::Mutex<SevenCloudAPI>>,
    ) -> Self {
        Self {
            pool,
            sevencloud_api,
        }
    }

    /// 同步七云订单到本地
    pub async fn sync_orders(&self, start_date: &str, end_date: &str) -> AppResult<usize> {
        let mut api = self.sevencloud_api.lock().await;
        let orders = api.get_orders(start_date, end_date).await?;

        let mut processed_count = 0;

        for order_record in orders {
            if let Err(e) = self.process_order(order_record).await {
                log::error!("Failed to process order: {e:?}");
                continue;
            }
            processed_count += 1;
        }

        log::debug!("Synchronization complete, processed orders: {processed_count}");
        Ok(processed_count)
    }

    /// 处理七云订单
    async fn process_order(&self, order_record: OrderRecord) -> AppResult<()> {
        // 检查订单是否已存在
        let existing = orders::Entity::find_by_id(order_record.id)
            .one(&self.pool)
            .await?;
        if existing.is_some() {
            log::debug!("Order already exists, skipping: {}", order_record.id);
            return Ok(());
        }

        // 根据会员号查找用户
        let user_opt = if let Some(member_code) = &order_record.member_code {
            users::Entity::find()
                .filter(users::Column::MemberCode.eq(member_code.clone()))
                .one(&self.pool)
                .await?
        } else {
            None
        };

        if let Some(user_model) = user_opt {
            let user_id_db: i64 = user_model.id;
            let referrer_id_opt: Option<i64> = user_model.referrer_id;
            // 开始事务
            let txn = self.pool.begin().await?;

            // 插入订单记录
            let created_at = chrono::DateTime::from_timestamp_millis(order_record.create_date)
                .unwrap_or_default();
            let price_cents: i64 = (order_record.price.unwrap_or(0.0) * 100.0) as i64;
            // 每满 $5.5 美元获得 1 次抽奖机会（按向下取整计算）
            let spins_awarded: i64 = if price_cents > 0 {
                price_cents / 550
            } else {
                0
            };

            let _inserted_order = orders::ActiveModel {
                id: Set(order_record.id),
                user_id: Set(user_id_db),
                member_code: Set(order_record.member_code.clone()),
                price: Set(price_cents),
                product_name: Set(order_record.product_name.clone()),
                product_no: Set(order_record.product_no.clone()),
                order_status: Set(order_record.status),
                pay_type: Set(Some(order_record.pay_type.unwrap_or_default())),
                stamps_earned: Set(Some(1)),
                external_created_at: Set(created_at),
                ..Default::default()
            }
            .insert(&txn)
            .await?;

            // 新订单 +1 个 stamp
            if let Some(user_model_in_txn) = users::Entity::find_by_id(user_id_db).one(&txn).await?
            {
                let new_stamps = user_model_in_txn.stamps.unwrap_or(0) + 1;
                let mut user_active = user_model_in_txn.into_active_model();
                user_active.stamps = Set(Some(new_stamps));
                user_active.update(&txn).await?;
            } else {
                log::warn!("User {user_id_db} not found inside txn when updating stamps");
            }

            // 订单返利（Sweet/Super 会员按自身等级返利；Fan 或已过期不返）
            if price_cents > 0 {
                // 查询当前下单用户（事务内）以获取最新余额/会员信息
                if let Some(buyer) = users::Entity::find_by_id(user_id_db).one(&txn).await? {
                    let now = chrono::Utc::now();

                    // 判断是否为有效付费会员（非 Fan 且未过期）
                    let is_active_paid = |u: &users::Model| -> bool {
                        matches!(
                            u.member_type,
                            MemberType::SweetShareholder | MemberType::SuperShareholder
                        ) && u.membership_expires_at.map(|t| t > now).unwrap_or(false)
                    };

                    // 买家返利
                    if is_active_paid(&buyer) {
                        let buyer_member_type = buyer.member_type.clone();
                        let buyer_rebate = match buyer_member_type {
                            MemberType::SweetShareholder => (price_cents * 5) / 100,
                            MemberType::SuperShareholder => price_cents / 10,
                            MemberType::Fan => 0,
                        };
                        if buyer_rebate > 0 {
                            let buyer_new_balance = buyer.balance.unwrap_or(0) + buyer_rebate;
                            let mut buyer_am = buyer.into_active_model();
                            buyer_am.balance = Set(Some(buyer_new_balance));
                            buyer_am.update(&txn).await?;

                            sct::ActiveModel {
                                user_id: Set(user_id_db),
                                transaction_type: Set(sct::TransactionType::Earn),
                                amount: Set(buyer_rebate),
                                balance_after: Set(buyer_new_balance),
                                related_order_id: Set(Some(order_record.id)),
                                description: Set(Some(format!(
                                    "Order cashback {}% for order {}",
                                    match buyer_member_type {
                                        MemberType::SweetShareholder => 5,
                                        MemberType::SuperShareholder => 10,
                                        MemberType::Fan => 0,
                                    },
                                    order_record.id
                                ))),
                                ..Default::default()
                            }
                            .insert(&txn)
                            .await?;
                        }
                    }

                    // 推荐人返利（好友下单时，推荐人若为有效付费会员则获得返利）
                    if let Some(referrer_id) = referrer_id_opt {
                        if let Some(referrer) =
                            users::Entity::find_by_id(referrer_id).one(&txn).await?
                        {
                            if is_active_paid(&referrer) {
                                let ref_member_type = referrer.member_type.clone();
                                let ref_rebate = match ref_member_type {
                                    MemberType::SweetShareholder => (price_cents * 5) / 100,
                                    MemberType::SuperShareholder => price_cents / 10,
                                    MemberType::Fan => 0,
                                };
                                if ref_rebate > 0 {
                                    let ref_new_balance =
                                        referrer.balance.unwrap_or(0) + ref_rebate;
                                    let mut ref_am = referrer.into_active_model();
                                    ref_am.balance = Set(Some(ref_new_balance));
                                    ref_am.update(&txn).await?;

                                    sct::ActiveModel {
                                        user_id: Set(referrer_id),
                                        transaction_type: Set(sct::TransactionType::Earn),
                                        amount: Set(ref_rebate),
                                        balance_after: Set(ref_new_balance),
                                        related_order_id: Set(Some(order_record.id)),
                                        description: Set(Some(format!(
                                            "Referral cashback {}% from user {} order {}",
                                            match ref_member_type {
                                                MemberType::SweetShareholder => 5,
                                                MemberType::SuperShareholder => 10,
                                                MemberType::Fan => 0,
                                            },
                                            user_id_db,
                                            order_record.id
                                        ))),
                                        ..Default::default()
                                    }
                                    .insert(&txn)
                                    .await?;
                                }
                            }
                        } else {
                            log::warn!(
                                "Referrer {referrer_id} not found inside txn when applying cashback"
                            );
                        }
                    }
                } else {
                    log::warn!("User {user_id_db} not found inside txn when applying cashback");
                }
            }

            // 按消费金额发放抽奖机会（$5.5/次）
            if spins_awarded > 0 {
                if let Some(ldc) = chances::Entity::find()
                    .filter(chances::Column::UserId.eq(user_id_db))
                    .one(&txn)
                    .await?
                {
                    let current_total = ldc.total_awarded;
                    let mut am = ldc.into_active_model();
                    am.total_awarded = Set(current_total + spins_awarded);
                    am.updated_at = Set(Some(Utc::now()));
                    am.update(&txn).await?;
                } else {
                    chances::ActiveModel {
                        user_id: Set(user_id_db),
                        total_awarded: Set(spins_awarded),
                        total_used: Set(0),
                        ..Default::default()
                    }
                    .insert(&txn)
                    .await?;
                }
            }

            txn.commit().await?;

            log::info!(
                "Successfully processed order: {}, User: {}, Stamps reward: {}, Spins awarded: {}",
                order_record.id,
                user_id_db,
                1,
                spins_awarded
            );
        } else {
            log::debug!(
                "Order has no associated user, skipping: {}",
                order_record.id
            );
        }

        Ok(())
    }

    /// 同步七云优惠码
    pub async fn sync_discount_codes(&self) -> AppResult<usize> {
        let mut api = self.sevencloud_api.lock().await;
        let coupons = api.get_discount_codes(None).await?;

        let mut processed_count = 0;

        for coupon_record in coupons {
            if let Err(e) = self.process_discount_code(coupon_record).await {
                log::error!("Failed to process discount code: {e:?}");
                continue;
            }
            processed_count += 1;
        }

        log::debug!("Synchronization complete, processed discount codes: {processed_count}");
        Ok(processed_count)
    }

    /// 处理七云优惠码
    async fn process_discount_code(&self, coupon_record: CouponRecord) -> AppResult<()> {
        // 同步逻辑：依据外部优惠码 code 字段（不使用 external_id），更新本地 is_used/used_at
        // _coupon_record.is_use: "0" 未使用, "1" 已使用
        let code_str = coupon_record.code.to_string();

        // 查询本地是否存在该优惠码
        let local = discount_codes::Entity::find()
            .filter(discount_codes::Column::Code.eq(code_str.clone()))
            .one(&self.pool)
            .await?;

        if local.is_none() {
            log::debug!(
                "Discount code not found locally, skipping sync: external_code={}",
                coupon_record.code
            );
            return Ok(());
        }
        let local = local.unwrap();
        let local_id: i64 = local.id;
        let local_is_used: bool = local.is_used.unwrap_or(false);

        let external_used = match coupon_record.is_use.as_str() {
            "0" => false,
            "1" => true,
            other => {
                log::warn!(
                    "Unknown is_use value from external coupon: code={}, value={}",
                    coupon_record.code,
                    other
                );
                false
            }
        };

        // 若外部已使用而本地未标记，则更新
        if external_used && !local_is_used {
            // 转换 use_date (七云时间戳假定为毫秒)；若不存在则使用当前时间
            let used_at = coupon_record
                .use_date
                .and_then(chrono::DateTime::from_timestamp_millis)
                .unwrap_or_else(chrono::Utc::now);

            // 保存 external_id 状态，避免 move 后无法访问
            let has_external_id = local.external_id.is_some();
            let mut active = local.into_active_model();
            active.is_used = Set(Some(true));
            active.used_at = Set(Some(used_at));
            // 同步 external_id（如果本地还没有）
            if !has_external_id {
                active.external_id = Set(Some(coupon_record.id));
            }
            active.updated_at = Set(Some(Utc::now()));
            active.update(&self.pool).await?;

            log::info!(
                "Discount code marked as used via sync: code={}, id={:?}",
                coupon_record.code,
                local_id
            );
        } else {
            // 外部未使用且本地也未使用：同步 external_id（如果本地还没有）
            if local.external_id.is_none() {
                let mut active = local.into_active_model();
                active.external_id = Set(Some(coupon_record.id));
                active.updated_at = Set(Some(Utc::now()));
                active.update(&self.pool).await?;
                log::info!(
                    "Discount code external_id synced: code={}, external_id={}",
                    coupon_record.code,
                    coupon_record.id
                );
            }
        }

        if !external_used && local_is_used {
            // 外部显示未使用但本地已使用——通常不回滚，记录冲突
            log::warn!(
                "Usage state mismatch (local used, external unused), keeping local: code={}, id={:?}",
                coupon_record.code,
                local_id
            );
        } else {
            log::debug!(
                "Discount code already in sync: code={}, used={}",
                coupon_record.code,
                external_used
            );
        }

        Ok(())
    }
}

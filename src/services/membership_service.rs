use crate::entities::StripeTransactionCategory;
use crate::entities::{
    CodeType, MemberType, MembershipPurchaseStatus, discount_code_entity as discount_codes,
    membership_purchase_entity as mp,
    user_entity as users,
};
use crate::error::{AppError, AppResult};
use crate::external::StripeService;
use crate::models::*;
use chrono::Utc;
use crate::services::{DiscountCodeService, DiscountValue, StripeTransactionService};
use crate::utils::is_in_promo_period;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, PaginatorTrait,
    QueryFilter, QueryOrder, Set, TransactionTrait,
};
use stripe::PaymentIntentStatus;

#[derive(Clone)]
pub struct MembershipService {
    pool: DatabaseConnection,
    stripe_service: StripeService,
    discount_code_service: DiscountCodeService,
    stx_service: StripeTransactionService,
}

impl MembershipService {
    pub fn new(
        pool: DatabaseConnection,
        stripe_service: StripeService,
        discount_code_service: DiscountCodeService,
    ) -> Self {
        let stx_service = StripeTransactionService::new(pool.clone());
        Self {
            pool,
            stripe_service,
            discount_code_service,
            stx_service,
        }
    }

    fn membership_price_cents(target: &MemberType) -> Option<i64> {
        match target {
            MemberType::SweetShareholder => Some(800),  // $8
            MemberType::SuperShareholder => Some(3000), // $30
            MemberType::Fan => None,                    // 不允许购买回Fan
        }
    }

    fn format_member_type(member_type: &MemberType) -> String {
        match member_type {
            MemberType::Fan => "Fan".to_string(),
            MemberType::SweetShareholder => "Sweets Shareholder".to_string(),
            MemberType::SuperShareholder => "Super Shareholder".to_string(),
        }
    }

    pub async fn create_membership_intent(
        &self,
        user_id: i64,
        req: CreateMembershipIntentRequest,
    ) -> AppResult<CreateMembershipIntentResponse> {
        // 查询当前用户会员类型和用户名
        let user = users::Entity::find_by_id(user_id)
            .one(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;

        let current = user.member_type.clone();
        let username = user.username.clone();

        // 不允许降级或重复购买同级
        if current == req.target_member_type {
            return Err(AppError::ValidationError("Already this membership".into()));
        }
        // 只能从 fan 升级到 sweet 或 super；从 sweet 升到 super
        if current == MemberType::SuperShareholder {
            return Err(AppError::ValidationError(
                "Already highest membership".into(),
            ));
        }
        if current == MemberType::SweetShareholder
            && req.target_member_type == MemberType::SweetShareholder
        {
            return Err(AppError::ValidationError(
                "Already sweet shareholder".into(),
            ));
        }
        if current == MemberType::Fan && req.target_member_type == MemberType::Fan {
            return Err(AppError::ValidationError(
                "Invalid target membership".into(),
            ));
        }
        if current == MemberType::SweetShareholder && req.target_member_type == MemberType::Fan {
            return Err(AppError::ValidationError("Cannot downgrade".into()));
        }

        let target_type = req.target_member_type.clone();
        let amount = Self::membership_price_cents(&target_type)
            .ok_or_else(|| AppError::ValidationError("Unsupported target member type".into()))?;

        let formatted_member_type = Self::format_member_type(&target_type);
        let description = format!("{} upgrade to {}", username, formatted_member_type);

        let payment_intent = self
            .stripe_service
            .create_payment_intent_with_category(
                amount,
                user_id,
                "membership",
                Some("usd".to_string()),
                Some(description.clone()),
                None,
            )
            .await?;

        // Checkout URL（官方支付页）
        let checkout = self
            .stripe_service
            .create_checkout_session_for_amount(
                amount,
                Some("usd".to_string()),
                user_id,
                "membership",
                Some(description.clone()),
                None,
            )
            .await?;

        let status = MembershipPurchaseStatus::Pending;
        let payment_intent_id = checkout
            .payment_intent_id
            .clone()
            .unwrap_or_else(|| payment_intent.id.to_string());
        // upsert-like: try insert, ignore unique conflict
        let _ = mp::ActiveModel {
            user_id: Set(user_id),
            stripe_payment_intent_id: Set(payment_intent_id.clone()),
            target_member_type: Set(req.target_member_type.clone()),
            amount: Set(amount),
            status: Set(status),
            ..Default::default()
        }
        .insert(&self.pool)
        .await
        .ok();

        // 记录 unified stripe transaction（创建阶段）
        let _ = self
            .stx_service
            .record_payment_intent(
                user_id,
                StripeTransactionCategory::Membership,
                &payment_intent_id,
                Some(amount),
                Some("usd".to_string()),
                Some(format!("{:?}", payment_intent.status)),
                payment_intent.description.clone(),
            )
            .await;

        Ok(CreateMembershipIntentResponse {
            payment_intent_id,
            client_secret: checkout
                .client_secret
                .unwrap_or_else(|| payment_intent.client_secret.unwrap_or_default()),
            checkout_url: checkout.url,
            amount,
            target_member_type: target_type,
        })
    }

    pub async fn confirm_membership(
        &self,
        user_id: i64,
        req: ConfirmMembershipRequest,
    ) -> AppResult<ConfirmMembershipResponse> {
        // 查询 intent
        let payment_intent = self
            .stripe_service
            .retrieve_payment_intent(&req.payment_intent_id)
            .await?;
        if payment_intent.status != PaymentIntentStatus::Succeeded {
            return Err(AppError::ValidationError("Payment not successful".into()));
        }

        let txn = self.pool.begin().await?;
        // 读取记录：优先按 payment_intent_id 精确匹配；若找不到，回退到按用户+金额+pending 匹配，并修正记录中的 PIID
        let rec = match mp::Entity::find()
            .filter(mp::Column::StripePaymentIntentId.eq(req.payment_intent_id.clone()))
            .filter(mp::Column::UserId.eq(user_id))
            .one(&txn)
            .await?
        {
            Some(r) => r,
            None => {
                // 回退：查找该用户最近一条金额相同且仍为 pending 的记录
                let alt = mp::Entity::find()
                    .filter(mp::Column::UserId.eq(user_id))
                    .filter(mp::Column::Status.eq(MembershipPurchaseStatus::Pending))
                    .filter(mp::Column::Amount.eq(payment_intent.amount))
                    .order_by_desc(mp::Column::CreatedAt)
                    .one(&txn)
                    .await?;
                let Some(alt_rec) = alt else {
                    return Err(AppError::NotFound(
                        "Membership purchase record not found".into(),
                    ));
                };
                // 更新其 payment_intent_id 为实际支付成功的 PI，避免后续再次不匹配
                let mut am = alt_rec.clone().into_active_model();
                am.stripe_payment_intent_id = Set(req.payment_intent_id.clone());
                am.update(&txn).await?;
                alt_rec
            }
        };
        let mut rec = rec;

        if rec.status == MembershipPurchaseStatus::Succeeded {
            // 已经处理，直接返回用户当前会员类型
            let mt = users::Entity::find_by_id(user_id)
                .one(&txn)
                .await?
                .map(|u| u.member_type)
                .unwrap_or(MemberType::Fan);
            let resp = MembershipPurchaseRecordResponse::from(rec);
            return Ok(ConfirmMembershipResponse {
                membership_record: resp,
                new_member_type: mt,
            });
        }

        // 升级用户会员类型并设置到期时间为NOW() + 1 year
        let new_member_type = rec.target_member_type.clone();
        if let Some(u) = users::Entity::find_by_id(user_id).one(&txn).await? {
            let mut am = u.into_active_model();
            am.member_type = Set(new_member_type.clone());
            let next = chrono::Utc::now() + chrono::Duration::days(365);
            am.membership_expires_at = Set(Some(next));
            am.update(&txn).await?;
        }

        // 更新记录状态
        let success = MembershipPurchaseStatus::Succeeded;
        if let Some(m) = mp::Entity::find_by_id(rec.id).one(&txn).await? {
            let mut am = m.into_active_model();
            am.status = Set(success);
            am.stripe_status = Set(Some(format!("{:?}", payment_intent.status)));
            am.update(&txn).await?;
        }

        // 提交事务后再进行外部福利发放，避免长事务或潜在锁冲突
        txn.commit().await?;

        // 异步后台发放福利（不阻塞 webhook 返回）
        let svc = self.discount_code_service.clone();
        let pool = self.pool.clone();
        let mt_for_task = new_member_type.clone();
        tokio::spawn(async move {
            match mt_for_task {
                MemberType::SweetShareholder => {
                    if let Err(e) = svc
                        .create_user_discount_code(user_id, DiscountValue::FixedAmount(800), CodeType::ShareholderReward, 1)
                        .await
                    {
                        log::error!(
                            "Failed to create shareholder reward code for user {user_id}: {e:?}"
                        );
                    }
                }
                MemberType::SuperShareholder => {
                    let mut handles = Vec::with_capacity(10);
                    for _ in 0..10 {
                        let svc_in = svc.clone();
                        handles.push(tokio::spawn(async move {
                            svc_in
                                .create_user_discount_code(
                                    user_id,
                                    DiscountValue::FixedAmount(300),
                                    CodeType::SuperShareholderReward,
                                    1,
                                )
                                .await
                        }));
                    }
                    for h in handles {
                        match h.await {
                            Ok(Ok(_)) => {}
                            Ok(Err(e)) => {
                                log::error!(
                                    "Failed to create one of super shareholder codes for user {user_id}: {e:?}"
                                );
                            }
                            Err(join_err) => {
                                log::error!(
                                    "Join error creating super shareholder codes for user {user_id}: {join_err}"
                                );
                            }
                        }
                    }
                }
                MemberType::Fan => {}
            }

            // 活动期（5/4-5/11）额外发放七五折优惠券
            if is_in_promo_period(Utc::now()) {
                let already_has = discount_codes::Entity::find()
                    .filter(discount_codes::Column::UserId.eq(user_id))
                    .filter(discount_codes::Column::CodeType.eq(CodeType::RegistrationReward))
                    .count(&pool)
                    .await
                    .unwrap_or(1)
                    > 0;

                if !already_has
                    && let Err(e) = svc
                        .create_user_discount_code(
                            user_id,
                            DiscountValue::Percentage(75),
                            CodeType::RegistrationReward,
                            1,
                        )
                        .await
                {
                    log::error!(
                        "Failed to grant promo BOGO50 code for member user {user_id}: {e:?}"
                    );
                }
            }
        });

        // 记录统一交易表
        let _ = self
            .stx_service
            .record_payment_intent(
                user_id,
                StripeTransactionCategory::Membership,
                &req.payment_intent_id,
                Some(rec.amount),
                Some("usd".to_string()),
                Some(format!("{:?}", payment_intent.status)),
                Some(format!("Membership confirmed: {:?}", new_member_type)),
            )
            .await;
        rec.status = MembershipPurchaseStatus::Succeeded;
        let new_type = new_member_type;
        let resp = MembershipPurchaseRecordResponse::from(rec);
        log::info!(
            "Membership confirmed for user_id={}, new_type={:?}",
            user_id,
            new_type
        );
        Ok(ConfirmMembershipResponse {
            membership_record: resp,
            new_member_type: new_type,
        })
    }

    /// 将已过期的会员降级为 Fan，返回处理的用户数量
    pub async fn expire_memberships(&self) -> AppResult<i64> {
        // approximate bulk update by scanning and updating; for simplicity
        let to_downgrade = users::Entity::find()
            .filter(users::Column::MembershipExpiresAt.lte(chrono::Utc::now()))
            .filter(users::Column::MembershipExpiresAt.is_not_null())
            .filter(users::Column::MemberType.ne(MemberType::Fan))
            .all(&self.pool)
            .await?;
        let mut count = 0i64;
        for u in to_downgrade {
            let mut am = u.into_active_model();
            am.member_type = Set(MemberType::Fan);
            am.update(&self.pool).await?;
            count += 1;
        }
        Ok(count)
    }
}

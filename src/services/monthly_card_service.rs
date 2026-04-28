use crate::entities::StripeTransactionCategory;
use crate::entities::{MonthlyCardStatus, monthly_card_entity as mc};
use crate::error::{AppError, AppResult};
use crate::external::StripeService;
use crate::models::*;
use crate::services::{DiscountCodeService, DiscountValue, StripeTransactionService};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};

#[derive(Clone)]
pub struct MonthlyCardService {
    pool: DatabaseConnection,
    stripe_service: StripeService,
    discount_code_service: DiscountCodeService,
    stx_service: StripeTransactionService,
}

impl MonthlyCardService {
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

    pub async fn create_monthly_card_intent(
        &self,
        user_id: i64,
        req: CreateMonthlyCardIntentRequest,
    ) -> AppResult<CreateMonthlyCardIntentResponse> {
        // 优先从配置读取对应 price 的金额，否则退回到本地常量（2000）
        let (_prod, one_time_pid, sub_pid) = self.stripe_service.monthly_card_ids();
        let chosen_price_id = match req.plan_type {
            crate::entities::MonthlyCardPlanType::OneTime => one_time_pid,
            crate::entities::MonthlyCardPlanType::Subscription => sub_pid,
        };
        let amount = if let Some(pid) = chosen_price_id.as_deref() {
            // 如果配置了 price_id，则到 Stripe 查询 unit_amount
            match self.stripe_service.get_price_unit_amount(pid).await {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("Failed to read price {pid} from Stripe: {e:?}");
                    return Err(AppError::ValidationError(
                        "Failed to read price from Stripe".into(),
                    ));
                }
            }
        } else {
            return Err(AppError::ValidationError("No valid price ID found".into()));
        };

        // Create PaymentIntent，附带 plan_type 与（可用时）price_id/product_id 方便审计
        let mut extra = std::collections::HashMap::new();
        extra.insert("plan_type".to_string(), req.plan_type.to_string());
        if let Some(pid) = chosen_price_id {
            extra.insert("price_id".to_string(), pid);
        }
        if let Some(prod) = _prod {
            extra.insert("product_id".to_string(), prod);
        }
        let pi = self
            .stripe_service
            .create_payment_intent_with_category(
                amount,
                user_id,
                "monthly_card",
                Some("usd".to_string()),
                Some(format!(
                    "User {user_id} buys monthly card ({})",
                    req.plan_type
                )),
                Some(extra),
            )
            .await?;

        // Checkout URL
        let checkout = self
            .stripe_service
            .create_checkout_session_for_amount(
                amount,
                Some("usd".to_string()),
                user_id,
                "monthly_card",
                Some(format!(
                    "User {user_id} buys monthly card ({})",
                    req.plan_type
                )),
                None,
            )
            .await?;

        let status = MonthlyCardStatus::Pending;
        let _ = mc::ActiveModel {
            user_id: Set(user_id),
            plan_type: Set(req.plan_type.clone()),
            status: Set(status),
            ..Default::default()
        }
        .insert(&self.pool)
        .await?;

        // record stripe tx
        let _ = self
            .stx_service
            .record_payment_intent(
                user_id,
                StripeTransactionCategory::MonthlyCard,
                checkout
                    .payment_intent_id
                    .as_deref()
                    .unwrap_or_else(|| pi.id.as_ref()),
                Some(amount),
                Some("usd".to_string()),
                Some(format!("{:?}", pi.status)),
                pi.description.clone(),
            )
            .await;

        Ok(CreateMonthlyCardIntentResponse {
            payment_intent_id: checkout
                .payment_intent_id
                .clone()
                .unwrap_or_else(|| pi.id.to_string()),
            client_secret: checkout
                .client_secret
                .clone()
                .unwrap_or_else(|| pi.client_secret.clone().unwrap_or_default()),
            checkout_url: checkout.url,
            amount,
            plan_type: req.plan_type,
        })
    }

    pub async fn confirm_monthly_card(
        &self,
        user_id: i64,
        req: ConfirmMonthlyCardRequest,
    ) -> AppResult<ConfirmMonthlyCardResponse> {
        let pi = self
            .stripe_service
            .retrieve_payment_intent(&req.payment_intent_id)
            .await?;
        if pi.status != stripe::PaymentIntentStatus::Succeeded {
            return Err(AppError::ValidationError("Payment not successful".into()));
        }
        let txn = self.pool.begin().await?;
        // pick the latest pending record for user
        let rec = mc::Entity::find()
            .filter(mc::Column::UserId.eq(user_id))
            .order_by_desc(mc::Column::CreatedAt)
            .one(&txn)
            .await?
            .ok_or_else(|| AppError::NotFound("Monthly card record not found".into()))?;
        if rec.status == MonthlyCardStatus::Active {
            let resp = MonthlyCardRecordResponse::from(rec);
            return Ok(ConfirmMonthlyCardResponse { monthly_card: resp });
        }
        let mut am = rec.into_active_model();
        am.status = Set(MonthlyCardStatus::Active);
        am.starts_at = Set(Some(Utc::now()));
        am.ends_at = Set(Some(Utc::now() + Duration::days(30)));
        am.update(&txn).await?;
        txn.commit().await?;
        let rec = mc::Entity::find()
            .filter(mc::Column::UserId.eq(user_id))
            .order_by_desc(mc::Column::CreatedAt)
            .one(&self.pool)
            .await?
            .unwrap();
        Ok(ConfirmMonthlyCardResponse {
            monthly_card: MonthlyCardRecordResponse::from(rec),
        })
    }

    /// 每日为活跃月卡用户发放 $5.5 优惠码，保证一天 1 次。
    pub async fn grant_daily_coupons(&self) -> AppResult<i64> {
        let today = Utc::now().date_naive();
        let active_cards = mc::Entity::find()
            .filter(mc::Column::Status.eq(MonthlyCardStatus::Active))
            .filter(mc::Column::EndsAt.gte(Utc::now()))
            .all(&self.pool)
            .await?;
        let mut granted = 0i64;
        for card in active_cards {
            if card.last_coupon_granted_on == Some(today) {
                continue;
            }
            // 发放 550 cents 优惠码，有效期 1 个月
            self.discount_code_service
                .create_user_discount_code(
                    card.user_id,
                    DiscountValue::FixedAmount(550),
                    crate::entities::CodeType::SweetsCreditsReward,
                    1,
                )
                .await?;
            let mut am = card.into_active_model();
            am.last_coupon_granted_on = Set(Some(today));
            am.update(&self.pool).await?;
            granted += 1;
        }
        Ok(granted)
    }

    /// 订阅续费成功，延长有效期 30 天
    pub async fn renew_by_subscription(&self, subscription_id: &str) -> AppResult<()> {
        if let Some(card) = mc::Entity::find()
            .filter(mc::Column::StripeSubscriptionId.eq(subscription_id.to_string()))
            .one(&self.pool)
            .await?
        {
            let mut am = card.clone().into_active_model();
            let base = card.ends_at.unwrap_or(Utc::now());
            am.ends_at = Set(Some(base + Duration::days(30)));
            am.status = Set(MonthlyCardStatus::Active);
            am.update(&self.pool).await?;
        }
        Ok(())
    }
}

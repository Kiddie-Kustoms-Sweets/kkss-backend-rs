use crate::entities::StripeTransactionCategory;
use crate::entities::stripe_transaction_entity as stx;
use crate::error::AppResult;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

#[derive(Clone)]
pub struct StripeTransactionService {
    pool: DatabaseConnection,
}

impl StripeTransactionService {
    pub fn new(pool: DatabaseConnection) -> Self {
        Self { pool }
    }

    /// 记录一条与 PaymentIntent 相关的交易
    #[allow(clippy::too_many_arguments)]
    pub async fn record_payment_intent(
        &self,
        user_id: i64,
        category: StripeTransactionCategory,
        payment_intent_id: &str,
        amount: Option<i64>,
        currency: Option<String>,
        status: Option<String>,
        description: Option<String>,
    ) -> AppResult<i64> {
        let model = stx::ActiveModel {
            user_id: Set(user_id),
            category: Set(category),
            payment_intent_id: Set(Some(payment_intent_id.to_string())),
            amount: Set(amount),
            currency: Set(currency),
            status: Set(status),
            description: Set(description),
            created_at: Set(Some(Utc::now())),
            ..Default::default()
        };
        let inserted = model.insert(&self.pool).await?;
        Ok(inserted.id)
    }

    /// 记录退款
    #[allow(clippy::too_many_arguments)]
    pub async fn record_refund(
        &self,
        user_id: i64,
        category: StripeTransactionCategory,
        refund_id: &str,
        charge_id: Option<String>,
        amount: Option<i64>,
        currency: Option<String>,
        status: Option<String>,
        description: Option<String>,
    ) -> AppResult<i64> {
        let model = stx::ActiveModel {
            user_id: Set(user_id),
            category: Set(category),
            refund_id: Set(Some(refund_id.to_string())),
            charge_id: Set(charge_id),
            amount: Set(amount),
            currency: Set(currency),
            status: Set(status),
            description: Set(description),
            created_at: Set(Some(Utc::now())),
            ..Default::default()
        };
        let inserted = model.insert(&self.pool).await?;
        Ok(inserted.id)
    }
}

use crate::entities::{CodeType, DiscountType};
use crate::entities::discount_code_entity;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DiscountCodeResponse {
    pub id: i64,
    pub code: String,
    pub discount_amount: i64,
    pub discount_type: DiscountType,
    pub code_type: CodeType,
    pub is_used: bool,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DiscountCodeQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub status: Option<String>,    // available/used/expired
    pub code_type: Option<String>, // shareholder_reward/super_shareholder_reward/sweets_credits_reward/free_topping
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RedeemDiscountCodeRequest {
    pub discount_amount: i64, // 要兑换的优惠码金额(美分)
    pub expire_months: u32,   // 有效期(月)，1-3
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RedeemDiscountCodeResponse {
    pub discount_code: DiscountCodeResponse,
    pub stamps_used: i64,
    pub remaining_stamps: i64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RedeemBalanceDiscountCodeRequest {
    pub discount_amount: i64, // 要兑换的优惠码金额(美分)，与 balance 1:1 扣减
    pub expire_months: u32,   // 有效期(月)，1-3
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RedeemBalanceDiscountCodeResponse {
    pub discount_code: DiscountCodeResponse,
    pub balance_used: i64,
    pub remaining_balance: i64,
}
// Convert from entity Model to API response
impl From<discount_code_entity::Model> for DiscountCodeResponse {
    fn from(m: discount_code_entity::Model) -> Self {
        Self {
            id: m.id,
            code: m.code,
            discount_amount: m.discount_amount,
            discount_type: m.discount_type,
            code_type: m.code_type,
            is_used: m.is_used.unwrap_or(false),
            expires_at: m.expires_at,
            created_at: m.created_at.unwrap_or_else(Utc::now),
        }
    }
}

use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use sea_orm::{DeriveActiveEnum, EnumIter};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema, DeriveActiveEnum, EnumIter,
)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "discount_type")]
#[serde(rename_all = "snake_case")]
pub enum DiscountType {
    #[sea_orm(string_value = "fixed_amount")]
    FixedAmount,
    #[sea_orm(string_value = "percentage")]
    Percentage,
}

impl std::fmt::Display for DiscountType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscountType::FixedAmount => write!(f, "fixed_amount"),
            DiscountType::Percentage => write!(f, "percentage"),
        }
    }
}

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema, DeriveActiveEnum, EnumIter,
)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "code_type")]
#[serde(rename_all = "snake_case")]
pub enum CodeType {
    #[sea_orm(string_value = "shareholder_reward")]
    ShareholderReward,
    #[sea_orm(string_value = "super_shareholder_reward")]
    SuperShareholderReward,
    #[sea_orm(string_value = "sweets_credits_reward")]
    SweetsCreditsReward,
    #[sea_orm(string_value = "free_topping")]
    FreeTopping,
    #[sea_orm(string_value = "registration_reward")]
    RegistrationReward,
}

impl std::fmt::Display for CodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodeType::ShareholderReward => write!(f, "shareholder_reward"),
            CodeType::SuperShareholderReward => write!(f, "super_shareholder_reward"),
            CodeType::SweetsCreditsReward => write!(f, "sweets_credits_reward"),
            CodeType::FreeTopping => write!(f, "free_topping"),
            CodeType::RegistrationReward => write!(f, "registration_reward"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "discount_codes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub code: String,
    pub discount_amount: i64,
    pub discount_type: DiscountType,
    pub code_type: CodeType,
    pub is_used: Option<bool>,
    pub used_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub external_id: Option<i64>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

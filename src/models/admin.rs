use crate::entities::{CodeType, DiscountType};
use crate::models::{UserResponse, UserStatistics};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminLoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// 管理看板总览统计
#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct AdminOverviewStats {
    pub total_users: i64,
    pub new_users_today: i64,
    pub new_users_7d: i64,
    pub new_users_30d: i64,
    pub fans: i64,
    pub sweet_shareholders: i64,
    pub super_shareholders: i64,
    /// 会员未过期（membership_expires_at > now）的用户数
    pub active_memberships: i64,
    /// 状态为 active 且未过期的月卡数
    pub active_monthly_cards: i64,
    pub total_orders: i64,
    /// 订单总额（美分）
    pub total_order_revenue: i64,
    pub orders_30d: i64,
    pub order_revenue_30d: i64,
    /// 充值成功总额（美分，不含赠送）
    pub total_recharge: i64,
    pub recharge_30d: i64,
    /// 未使用且未过期
    pub coupons_available: i64,
    pub coupons_used: i64,
    /// 全平台用户余额总和（美分）
    pub sweet_cash_outstanding: i64,
    /// 有邀请人的用户总数
    pub total_referred_users: i64,
}

/// 按 code_type 分组的优惠券统计
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminCouponStatItem {
    pub code_type: CodeType,
    /// 未使用且未过期
    pub available: i64,
    pub used: i64,
    /// 已过期但未删除（理论上清理任务会删除，残留则为异常）
    pub expired_unused: i64,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AdminUserQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    /// 手机号 / 会员号 / 用户名模糊搜索
    pub search: Option<String>,
    /// fan / sweet_shareholder / super_shareholder
    pub member_type: Option<String>,
}

/// 仅分页参数（邀请列表等）
#[derive(Debug, Deserialize, IntoParams)]
pub struct AdminPageQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

/// 用户详情：资料 + 统计
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminUserDetailResponse {
    pub user: UserResponse,
    pub statistics: UserStatistics,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdjustBalanceRequest {
    /// 调整金额（美分），正数增加、负数扣减
    pub amount: i64,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdjustStampsRequest {
    /// 调整印花数，正数增加、负数扣减
    pub delta: i64,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminGrantCouponRequest {
    pub discount_type: DiscountType,
    /// fixed_amount: 金额（美分）；percentage: 折数的 10 倍（75 = 7.5 折）
    pub value: i64,
    pub code_type: CodeType,
    /// 有效期（月），1-3
    pub expire_months: u32,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AdminCouponQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub user_id: Option<i64>,
    /// shareholder_reward / super_shareholder_reward / sweets_credits_reward / free_topping / registration_reward
    pub code_type: Option<String>,
    /// available / used / expired
    pub status: Option<String>,
}

/// 管理端优惠券列表项（附带归属用户信息）
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminCouponResponse {
    pub id: i64,
    pub user_id: i64,
    pub user_member_code: Option<String>,
    pub user_phone: Option<String>,
    pub code: String,
    pub discount_amount: i64,
    pub discount_type: DiscountType,
    pub code_type: CodeType,
    pub is_used: bool,
    pub used_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub external_id: Option<i64>,
    pub created_at: Option<DateTime<Utc>>,
}

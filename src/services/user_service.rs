use crate::entities::{
    discount_code_entity as discount_codes, monthly_card_entity as monthly_cards,
    order_entity as orders, sweet_cash_transaction_entity as sct, user_entity as users,
};
use crate::error::{AppError, AppResult};
use crate::models::*;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, ExprTrait, IntoActiveModel,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};

#[derive(Clone)]
pub struct UserService {
    pool: DatabaseConnection,
}

impl UserService {
    pub fn new(pool: DatabaseConnection) -> Self {
        Self { pool }
    }

    /// 获取用户个人资料和统计信息
    pub async fn get_user_profile(
        &self,
        user_id: i64,
    ) -> AppResult<(UserResponse, UserStatistics)> {
        let u = users::Entity::find_by_id(user_id).one(&self.pool).await?;
        let user = u.ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        // 获取推荐人数
        let total_referrals = users::Entity::find()
            .filter(users::Column::ReferrerId.eq(user_id))
            .count(&self.pool)
            .await? as i64;

        // 获取用户统计信息
        let statistics = self.get_user_statistics(user_id).await?;

        let mut user_response = UserResponse::from(user);
        user_response.total_referrals = total_referrals;

        // 查询月卡状态与过期时间
        let mc = monthly_cards::Entity::find()
            .filter(monthly_cards::Column::UserId.eq(user_id))
            .filter(monthly_cards::Column::Status.eq(monthly_cards::MonthlyCardStatus::Active))
            .filter(monthly_cards::Column::EndsAt.gt(chrono::Utc::now()))
            .order_by_desc(monthly_cards::Column::EndsAt)
            .one(&self.pool)
            .await?;
        user_response.is_monthly_card = mc.is_some();
        user_response.monthly_card_expires_at = mc.as_ref().and_then(|m| m.ends_at);

        Ok((user_response, statistics))
    }

    /// 更新用户Profile
    pub async fn update_user_profile(
        &self,
        user_id: i64,
        request: UpdateUserRequest,
    ) -> AppResult<UserResponse> {
        // 验证输入
        if let Some(username) = &request.username
            && (username.len() < 2 || username.len() > 20)
        {
            return Err(AppError::ValidationError(
                "Username length must be between 2 and 20 characters".to_string(),
            ));
        }

        let birthday = if let Some(birthday_str) = &request.birthday {
            Some(
                chrono::NaiveDate::parse_from_str(birthday_str, "%Y-%m-%d").map_err(|_| {
                    AppError::ValidationError("Invalid birthday format".to_string())
                })?,
            )
        } else {
            None
        };

        // 检查是否有需要更新的字段
        if request.username.is_none() && request.birthday.is_none() {
            return Err(AppError::ValidationError("No fields to update".to_string()));
        }

        // 根据提供的字段执行相应的更新
        let mut model = users::Entity::find_by_id(user_id)
            .one(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?
            .into_active_model();
        if let Some(username) = &request.username {
            model.username = Set(username.clone());
        }
        if let Some(b) = &birthday {
            model.birthday = Set(*b);
            use chrono::Datelike;
            model.birthday_month = Set(b.month() as i16);
            model.birthday_day = Set(b.day() as i16);
        }
        let _updated = model.update(&self.pool).await?;

        // 返回更新后的用户信息
        let (user_response, _) = self.get_user_profile(user_id).await?;
        Ok(user_response)
    }

    /// 获取用户推荐列表
    pub async fn get_user_referrals(
        &self,
        user_id: i64,
        params: &PaginationParams,
    ) -> AppResult<PaginatedResponse<UserResponse>> {
        let offset = params.get_offset();
        let limit = params.get_limit();

        // 获取总数
        let total = users::Entity::find()
            .filter(users::Column::ReferrerId.eq(user_id))
            .count(&self.pool)
            .await? as i64;

        // 获取推荐用户列表
        let models = users::Entity::find()
            .filter(users::Column::ReferrerId.eq(user_id))
            .order_by_desc(users::Column::CreatedAt)
            .limit(limit as u64)
            .offset(offset as u64)
            .all(&self.pool)
            .await?;
        let items: Vec<UserResponse> = models.into_iter().map(UserResponse::from).collect();

        Ok(PaginatedResponse::new(
            items,
            params.page.unwrap_or(1),
            params.page_size.unwrap_or(20),
            total,
        ))
    }

    /// 获取用户统计信息
    async fn get_user_statistics(&self, user_id: i64) -> AppResult<UserStatistics> {
        // 获取订单统计
        #[derive(Debug, sea_orm::FromQueryResult)]
        struct OrderStatsRow {
            total_orders: i64,
            // SUM 在无记录时会返回 NULL，这里使用 Option 以避免解码错误；并将 SUM(price) 显式 cast 为 BIGINT 以避免 NUMERIC -> i64 解码问题
            total_spent: Option<i64>,
            total_earned_stamps: Option<i64>,
        }
        let order_stats_row: Option<OrderStatsRow> = orders::Entity::find()
            .filter(orders::Column::UserId.eq(user_id))
            .select_only()
            .column_as(Expr::val(1).count(), "total_orders")
            .column_as(Expr::cust("SUM(price)::BIGINT"), "total_spent")
            .column_as(
                Expr::cust("SUM(stamps_earned)::BIGINT"),
                "total_earned_stamps",
            )
            .into_model::<OrderStatsRow>()
            .one(&self.pool)
            .await?;

        // 获取可用优惠码数量
        let available_codes = discount_codes::Entity::find()
            .filter(discount_codes::Column::UserId.eq(user_id))
            .filter(discount_codes::Column::IsUsed.eq(false))
            .filter(discount_codes::Column::ExpiresAt.gt(chrono::Utc::now()))
            .count(&self.pool)
            .await? as i64;

        Ok(UserStatistics {
            total_orders: order_stats_row
                .as_ref()
                .map(|r| r.total_orders)
                .unwrap_or(0),
            total_spent: order_stats_row
                .as_ref()
                .and_then(|r| r.total_spent)
                .unwrap_or(0),
            total_earned_stamps: order_stats_row
                .as_ref()
                .and_then(|r| r.total_earned_stamps)
                .unwrap_or(0),
            available_discount_codes: available_codes,
        })
    }

    /// 获取用户钱包流水：充值(成功)、生日奖励(Earn)、兑换(Redeem)
    pub async fn get_user_wallet_transactions(
        &self,
        user_id: i64,
        params: &PaginationParams,
    ) -> AppResult<PaginatedResponse<WalletTransactionResponse>> {
        let offset = params.get_offset();
        let limit = params.get_limit();

        // 统计总数（所有钱包流水均来自 sweet_cash_transactions）
        let total = sct::Entity::find()
            .filter(sct::Column::UserId.eq(user_id))
            .count(&self.pool)
            .await? as i64;

        // 拉取当前页数据
        let rows = sct::Entity::find()
            .filter(sct::Column::UserId.eq(user_id))
            .order_by_desc(sct::Column::CreatedAt)
            .limit(limit as u64)
            .offset(offset as u64)
            .all(&self.pool)
            .await?;

        let items: Vec<WalletTransactionResponse> = rows
            .into_iter()
            .map(|t| {
                let kind = match t.transaction_type {
                    sct::TransactionType::Redeem => WalletTransactionKind::Redeem,
                    sct::TransactionType::Earn => {
                        let is_birthday = t
                            .description
                            .as_deref()
                            .map(|d| d.contains("Birthday"))
                            .unwrap_or(false);
                        if is_birthday {
                            WalletTransactionKind::BirthdayReward
                        } else {
                            WalletTransactionKind::Recharge
                        }
                    }
                };
                WalletTransactionResponse {
                    id: t.id,
                    kind,
                    amount: t.amount,
                    balance_after: Some(t.balance_after),
                    description: t.description.clone(),
                    created_at: t.created_at.unwrap_or_else(chrono::Utc::now),
                }
            })
            .collect();

        Ok(PaginatedResponse::new(
            items,
            params.page.unwrap_or(1),
            params.page_size.unwrap_or(20),
            total,
        ))
    }
}

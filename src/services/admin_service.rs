use crate::config::AdminConfig;
use crate::entities::{
    CodeType, DiscountType, MemberType, MonthlyCardStatus, RechargeStatus,
    discount_code_entity as discount_codes, monthly_card_entity as monthly_cards,
    order_entity as orders, recharge_record_entity as recharge_records,
    sweet_cash_transaction_entity as sct, user_entity as users,
};
use crate::error::{AppError, AppResult};
use crate::external::SevenCloudAPI;
use crate::models::*;
use crate::services::{DiscountCodeService, DiscountValue, UserService};
use crate::utils::JwtService;
use chrono::{Duration, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, ExprTrait,
    IntoActiveModel, Iterable, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait,
};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct AdminService {
    pool: DatabaseConnection,
    jwt_service: JwtService,
    admin_config: AdminConfig,
    user_service: UserService,
    discount_code_service: DiscountCodeService,
    sevencloud_api: Arc<Mutex<SevenCloudAPI>>,
}

fn parse_member_type(s: &str) -> AppResult<MemberType> {
    match s {
        "fan" => Ok(MemberType::Fan),
        "sweet_shareholder" => Ok(MemberType::SweetShareholder),
        "super_shareholder" => Ok(MemberType::SuperShareholder),
        other => Err(AppError::ValidationError(format!(
            "Invalid member_type: {other}. Expected fan/sweet_shareholder/super_shareholder"
        ))),
    }
}

fn parse_code_type(s: &str) -> AppResult<CodeType> {
    match s {
        "shareholder_reward" => Ok(CodeType::ShareholderReward),
        "super_shareholder_reward" => Ok(CodeType::SuperShareholderReward),
        "sweets_credits_reward" => Ok(CodeType::SweetsCreditsReward),
        "free_topping" => Ok(CodeType::FreeTopping),
        "registration_reward" => Ok(CodeType::RegistrationReward),
        other => Err(AppError::ValidationError(format!(
            "Invalid code_type: {other}"
        ))),
    }
}

impl AdminService {
    pub fn new(
        pool: DatabaseConnection,
        jwt_service: JwtService,
        admin_config: AdminConfig,
        user_service: UserService,
        discount_code_service: DiscountCodeService,
        sevencloud_api: Arc<Mutex<SevenCloudAPI>>,
    ) -> Self {
        Self {
            pool,
            jwt_service,
            admin_config,
            user_service,
            discount_code_service,
            sevencloud_api,
        }
    }

    /// 管理员登录（账号密码来自环境变量 ADMIN_USERNAME / ADMIN_PASSWORD）
    pub async fn login(&self, request: AdminLoginRequest) -> AppResult<AdminLoginResponse> {
        if self.admin_config.username.is_empty() || self.admin_config.password.is_empty() {
            return Err(AppError::ConfigError(
                "Admin account is not configured".to_string(),
            ));
        }
        if request.username != self.admin_config.username
            || request.password != self.admin_config.password
        {
            return Err(AppError::AuthError("Invalid admin credentials".to_string()));
        }

        let token = self.jwt_service.generate_admin_token(&request.username)?;
        Ok(AdminLoginResponse {
            access_token: token,
            token_type: "Bearer".to_string(),
            expires_in: self.jwt_service.get_access_token_expires_in(),
        })
    }

    /// 看板总览统计
    pub async fn get_overview(&self) -> AppResult<AdminOverviewStats> {
        let now = Utc::now();
        let today_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap_or_default()
            .and_utc();
        let d7 = now - Duration::days(7);
        let d30 = now - Duration::days(30);

        #[derive(Debug, sea_orm::FromQueryResult)]
        struct OrderAgg {
            total: i64,
            revenue: Option<i64>,
        }
        let order_agg = |days: Option<chrono::DateTime<Utc>>| {
            let mut q = orders::Entity::find()
                .select_only()
                .column_as(Expr::val(1).count(), "total")
                .column_as(Expr::cust("SUM(price)::BIGINT"), "revenue");
            if let Some(since) = days {
                q = q.filter(orders::Column::ExternalCreatedAt.gte(since));
            }
            q.into_model::<OrderAgg>().one(&self.pool)
        };

        #[derive(Debug, sea_orm::FromQueryResult)]
        struct SumRow {
            total: Option<i64>,
        }

        let mut stats = AdminOverviewStats {
            total_users: users::Entity::find().count(&self.pool).await? as i64,
            new_users_today: users::Entity::find()
                .filter(users::Column::CreatedAt.gte(today_start))
                .count(&self.pool)
                .await? as i64,
            new_users_7d: users::Entity::find()
                .filter(users::Column::CreatedAt.gte(d7))
                .count(&self.pool)
                .await? as i64,
            new_users_30d: users::Entity::find()
                .filter(users::Column::CreatedAt.gte(d30))
                .count(&self.pool)
                .await? as i64,
            fans: users::Entity::find()
                .filter(users::Column::MemberType.eq(MemberType::Fan))
                .count(&self.pool)
                .await? as i64,
            sweet_shareholders: users::Entity::find()
                .filter(users::Column::MemberType.eq(MemberType::SweetShareholder))
                .count(&self.pool)
                .await? as i64,
            super_shareholders: users::Entity::find()
                .filter(users::Column::MemberType.eq(MemberType::SuperShareholder))
                .count(&self.pool)
                .await? as i64,
            active_memberships: users::Entity::find()
                .filter(users::Column::MembershipExpiresAt.gt(now))
                .count(&self.pool)
                .await? as i64,
            active_monthly_cards: monthly_cards::Entity::find()
                .filter(monthly_cards::Column::Status.eq(MonthlyCardStatus::Active))
                .filter(monthly_cards::Column::EndsAt.gt(now))
                .count(&self.pool)
                .await? as i64,
            coupons_available: discount_codes::Entity::find()
                .filter(discount_codes::Column::IsUsed.eq(false))
                .filter(discount_codes::Column::ExpiresAt.gt(now))
                .count(&self.pool)
                .await? as i64,
            coupons_used: discount_codes::Entity::find()
                .filter(discount_codes::Column::IsUsed.eq(true))
                .count(&self.pool)
                .await? as i64,
            total_referred_users: users::Entity::find()
                .filter(users::Column::ReferrerId.is_not_null())
                .count(&self.pool)
                .await? as i64,
            ..Default::default()
        };

        // 订单汇总（全部 / 近 30 天）
        if let Some(row) = order_agg(None).await? {
            stats.total_orders = row.total;
            stats.total_order_revenue = row.revenue.unwrap_or(0);
        }
        if let Some(row) = order_agg(Some(d30)).await? {
            stats.orders_30d = row.total;
            stats.order_revenue_30d = row.revenue.unwrap_or(0);
        }

        // 充值成功金额（全部 / 近 30 天）
        let recharge_sum = |since: Option<chrono::DateTime<Utc>>| {
            let mut q = recharge_records::Entity::find()
                .filter(recharge_records::Column::Status.eq(RechargeStatus::Succeeded))
                .select_only()
                .column_as(Expr::cust("SUM(amount)::BIGINT"), "total");
            if let Some(since) = since {
                q = q.filter(recharge_records::Column::CreatedAt.gte(since));
            }
            q.into_model::<SumRow>().one(&self.pool)
        };
        stats.total_recharge = recharge_sum(None)
            .await?
            .and_then(|r| r.total)
            .unwrap_or(0);
        stats.recharge_30d = recharge_sum(Some(d30))
            .await?
            .and_then(|r| r.total)
            .unwrap_or(0);

        // 全平台 Sweet Cash 余额
        stats.sweet_cash_outstanding = users::Entity::find()
            .select_only()
            .column_as(Expr::cust("SUM(balance)::BIGINT"), "total")
            .into_model::<SumRow>()
            .one(&self.pool)
            .await?
            .and_then(|r| r.total)
            .unwrap_or(0);

        Ok(stats)
    }

    /// 按 code_type 分组的优惠券统计
    pub async fn get_coupon_stats(&self) -> AppResult<Vec<AdminCouponStatItem>> {
        let now = Utc::now();
        let mut items = Vec::new();
        for ct in CodeType::iter() {
            let base = || {
                discount_codes::Entity::find()
                    .filter(discount_codes::Column::CodeType.eq(ct.clone()))
            };
            let available = base()
                .filter(discount_codes::Column::IsUsed.eq(false))
                .filter(discount_codes::Column::ExpiresAt.gt(now))
                .count(&self.pool)
                .await? as i64;
            let used = base()
                .filter(discount_codes::Column::IsUsed.eq(true))
                .count(&self.pool)
                .await? as i64;
            let expired_unused = base()
                .filter(discount_codes::Column::IsUsed.eq(false))
                .filter(discount_codes::Column::ExpiresAt.lte(now))
                .count(&self.pool)
                .await? as i64;
            items.push(AdminCouponStatItem {
                code_type: ct,
                available,
                used,
                expired_unused,
            });
        }
        Ok(items)
    }

    /// 用户列表（搜索 + 会员类型过滤 + 分页）
    pub async fn list_users(
        &self,
        query: &AdminUserQuery,
    ) -> AppResult<PaginatedResponse<UserResponse>> {
        let params = PaginationParams::new(query.page, query.per_page);

        let mut cond = Condition::all();
        if let Some(search) = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            cond = cond.add(
                Condition::any()
                    .add(users::Column::Phone.contains(search))
                    .add(users::Column::MemberCode.contains(search))
                    .add(users::Column::Username.contains(search)),
            );
        }
        if let Some(mt) = query.member_type.as_deref() {
            cond = cond.add(users::Column::MemberType.eq(parse_member_type(mt)?));
        }

        let total = users::Entity::find()
            .filter(cond.clone())
            .count(&self.pool)
            .await? as i64;

        let models = users::Entity::find()
            .filter(cond)
            .order_by_desc(users::Column::CreatedAt)
            .limit(params.get_limit() as u64)
            .offset(params.get_offset() as u64)
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

    /// 用户详情（资料 + 统计）
    pub async fn get_user_detail(&self, user_id: i64) -> AppResult<AdminUserDetailResponse> {
        let (user, statistics) = self.user_service.get_user_profile(user_id).await?;
        Ok(AdminUserDetailResponse { user, statistics })
    }

    /// 用户的直接邀请列表
    pub async fn get_user_referrals(
        &self,
        user_id: i64,
        query: &AdminPageQuery,
    ) -> AppResult<PaginatedResponse<UserResponse>> {
        self.user_service
            .get_user_referrals(user_id, &PaginationParams::new(query.page, query.per_page))
            .await
    }

    /// 人工调整用户余额（美分，正负数），并写 sweet_cash_transactions 流水
    pub async fn adjust_balance(
        &self,
        user_id: i64,
        request: AdjustBalanceRequest,
    ) -> AppResult<UserResponse> {
        if request.amount == 0 {
            return Err(AppError::ValidationError("Amount must not be zero".to_string()));
        }
        if request.reason.trim().is_empty() {
            return Err(AppError::ValidationError("Reason is required".to_string()));
        }

        let txn = self.pool.begin().await?;
        let user = users::Entity::find_by_id(user_id)
            .one(&txn)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        let current = user.balance.unwrap_or(0);
        let new_balance = current + request.amount;
        if new_balance < 0 {
            return Err(AppError::ValidationError(format!(
                "Resulting balance cannot be negative (current: {current}, adjustment: {})",
                request.amount
            )));
        }

        let mut am = user.into_active_model();
        am.balance = Set(Some(new_balance));
        am.updated_at = Set(Some(Utc::now()));
        am.update(&txn).await?;

        sct::ActiveModel {
            user_id: Set(user_id),
            transaction_type: Set(if request.amount > 0 {
                sct::TransactionType::Earn
            } else {
                sct::TransactionType::Redeem
            }),
            amount: Set(request.amount.abs()),
            balance_after: Set(new_balance),
            related_order_id: Set(None),
            related_discount_code_id: Set(None),
            description: Set(Some(format!("Admin adjustment: {}", request.reason))),
            ..Default::default()
        }
        .insert(&txn)
        .await?;

        txn.commit().await?;
        log::info!(
            "Admin adjusted balance for user {user_id}: {} -> {} (reason: {})",
            current,
            new_balance,
            request.reason
        );

        let (user, _) = self.user_service.get_user_profile(user_id).await?;
        Ok(user)
    }

    /// 人工调整用户印花（正负数）
    pub async fn adjust_stamps(
        &self,
        user_id: i64,
        request: AdjustStampsRequest,
    ) -> AppResult<UserResponse> {
        if request.delta == 0 {
            return Err(AppError::ValidationError("Delta must not be zero".to_string()));
        }
        if request.reason.trim().is_empty() {
            return Err(AppError::ValidationError("Reason is required".to_string()));
        }

        let user = users::Entity::find_by_id(user_id)
            .one(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        let current = user.stamps.unwrap_or(0);
        let new_stamps = current + request.delta;
        if new_stamps < 0 {
            return Err(AppError::ValidationError(format!(
                "Resulting stamps cannot be negative (current: {current}, adjustment: {})",
                request.delta
            )));
        }

        let mut am = user.into_active_model();
        am.stamps = Set(Some(new_stamps));
        am.updated_at = Set(Some(Utc::now()));
        am.update(&self.pool).await?;

        log::info!(
            "Admin adjusted stamps for user {user_id}: {current} -> {new_stamps} (reason: {})",
            request.reason
        );

        let (user, _) = self.user_service.get_user_profile(user_id).await?;
        Ok(user)
    }

    /// 人工发放优惠券（同步在 SevenCloud 生成）
    pub async fn grant_coupon(
        &self,
        user_id: i64,
        request: AdminGrantCouponRequest,
    ) -> AppResult<DiscountCodeResponse> {
        users::Entity::find_by_id(user_id)
            .one(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

        let value = match request.discount_type {
            DiscountType::FixedAmount => DiscountValue::FixedAmount(request.value),
            DiscountType::Percentage => DiscountValue::Percentage(request.value),
        };
        let id = self
            .discount_code_service
            .create_user_discount_code(user_id, value, request.code_type, request.expire_months)
            .await?;

        let model = discount_codes::Entity::find_by_id(id)
            .one(&self.pool)
            .await?
            .ok_or_else(|| AppError::InternalError("Created coupon not found".to_string()))?;
        Ok(DiscountCodeResponse::from(model))
    }

    /// 全量优惠券列表（过滤 + 分页，附带归属用户信息）
    pub async fn list_coupons(
        &self,
        query: &AdminCouponQuery,
    ) -> AppResult<PaginatedResponse<AdminCouponResponse>> {
        let params = PaginationParams::new(query.page, query.per_page);
        let now = Utc::now();

        let mut cond = Condition::all();
        if let Some(uid) = query.user_id {
            cond = cond.add(discount_codes::Column::UserId.eq(uid));
        }
        if let Some(ct) = query.code_type.as_deref() {
            cond = cond.add(discount_codes::Column::CodeType.eq(parse_code_type(ct)?));
        }
        match query.status.as_deref() {
            Some("available") => {
                cond = cond
                    .add(discount_codes::Column::IsUsed.eq(false))
                    .add(discount_codes::Column::ExpiresAt.gt(now));
            }
            Some("used") => {
                cond = cond.add(discount_codes::Column::IsUsed.eq(true));
            }
            Some("expired") => {
                cond = cond
                    .add(discount_codes::Column::IsUsed.eq(false))
                    .add(discount_codes::Column::ExpiresAt.lte(now));
            }
            Some(other) => {
                return Err(AppError::ValidationError(format!(
                    "Invalid status: {other}. Expected available/used/expired"
                )));
            }
            None => {}
        }

        let total = discount_codes::Entity::find()
            .filter(cond.clone())
            .count(&self.pool)
            .await? as i64;

        let models = discount_codes::Entity::find()
            .filter(cond)
            .order_by_desc(discount_codes::Column::CreatedAt)
            .limit(params.get_limit() as u64)
            .offset(params.get_offset() as u64)
            .all(&self.pool)
            .await?;

        // 批量取归属用户，避免 N+1
        let user_ids: Vec<i64> = {
            let mut ids: Vec<i64> = models.iter().map(|m| m.user_id).collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };
        let owners: std::collections::HashMap<i64, users::Model> = users::Entity::find()
            .filter(users::Column::Id.is_in(user_ids))
            .all(&self.pool)
            .await?
            .into_iter()
            .map(|u| (u.id, u))
            .collect();

        let items = models
            .into_iter()
            .map(|m| {
                let owner = owners.get(&m.user_id);
                AdminCouponResponse {
                    id: m.id,
                    user_id: m.user_id,
                    user_member_code: owner.map(|u| u.member_code.clone()),
                    user_phone: owner.map(|u| u.phone.clone()),
                    code: m.code,
                    discount_amount: m.discount_amount,
                    discount_type: m.discount_type,
                    code_type: m.code_type,
                    is_used: m.is_used.unwrap_or(false),
                    used_at: m.used_at,
                    expires_at: m.expires_at,
                    external_id: m.external_id,
                    created_at: m.created_at,
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

    /// 撤销优惠券（有 external_id 时先删 SevenCloud，失败则保留本地记录并报错）
    pub async fn revoke_coupon(&self, id: i64) -> AppResult<()> {
        let model = discount_codes::Entity::find_by_id(id)
            .one(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Coupon not found".to_string()))?;

        if let Some(ext_id) = model.external_id {
            let mut api = self.sevencloud_api.lock().await;
            api.delete_discount_codes(vec![ext_id]).await?;
        }

        model.into_active_model().delete(&self.pool).await?;
        log::info!("Admin revoked coupon id={id}");
        Ok(())
    }
}

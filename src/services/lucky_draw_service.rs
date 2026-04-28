use crate::entities::{
    CodeType, MonthlyCardPlanType, MonthlyCardStatus, lucky_draw_chance_entity as chances,
    lucky_draw_prize_entity as prizes, lucky_draw_record_entity as records,
    monthly_card_entity as mc,
};
use crate::error::{AppError, AppResult};
use crate::models::{
    LuckyDrawChancesResponse, LuckyDrawPrizeResponse, LuckyDrawRecordPageResponse,
    LuckyDrawRecordQuery, LuckyDrawRecordResponse, LuckyDrawSpinResponse, LuckyDrawWonPrize,
    PaginatedResponse, PaginationParams,
};
use crate::services::{DiscountCodeService, DiscountValue};
use chrono::{Duration, Utc};
use rand::Rng;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, IntoActiveModel,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use sea_orm::{Condition, Order, UpdateResult};

#[derive(Clone)]
pub struct LuckyDrawService {
    pool: DatabaseConnection,
    discount_code_service: DiscountCodeService,
}

impl LuckyDrawService {
    pub fn new(pool: DatabaseConnection, discount_code_service: DiscountCodeService) -> Self {
        Self {
            pool,
            discount_code_service,
        }
    }

    /// 获取用户抽奖次数（不存在则初始化）
    pub async fn get_user_chances(&self, user_id: i64) -> AppResult<LuckyDrawChancesResponse> {
        let model = self.ensure_chances(user_id).await?;
        Ok(model.into())
    }

    /// 获取奖品列表（仅活动的）
    pub async fn list_prizes(&self) -> AppResult<Vec<LuckyDrawPrizeResponse>> {
        let list = prizes::Entity::find()
            .filter(prizes::Column::IsActive.eq(true))
            .order_by_asc(prizes::Column::Id)
            .all(&self.pool)
            .await?;
        Ok(list.into_iter().map(Into::into).collect())
    }

    /// 获取抽奖记录（分页）
    pub async fn list_records(
        &self,
        user_id: i64,
        query: &LuckyDrawRecordQuery,
    ) -> AppResult<LuckyDrawRecordPageResponse> {
        let params = PaginationParams::new(query.page, query.per_page);
        let offset = params.get_offset();
        let limit = params.get_limit();

        let base_query = records::Entity::find().filter(records::Column::UserId.eq(user_id));

        let total = base_query.clone().count(&self.pool).await? as i64;

        let items_models = base_query
            .order_by(records::Column::CreatedAt, Order::Desc)
            .limit(limit as u64)
            .offset(offset as u64)
            .all(&self.pool)
            .await?;

        let items: Vec<LuckyDrawRecordResponse> =
            items_models.into_iter().map(Into::into).collect();

        Ok(PaginatedResponse::new(
            items,
            params.page.unwrap_or(1),
            params.page_size.unwrap_or(20),
            total,
        ))
    }

    /// 抽奖 (Spin)
    ///
    /// 逻辑:
    /// 1. 校验用户剩余次数
    /// 2. 读取启用的奖品并过滤掉已无库存的限量奖品
    /// 3. 按概率 (basis points) 随机抽取
    /// 4. 若命中限量奖品则原子扣减库存 (乐观: update where stock_remaining > 0)
    /// 5. 创建抽奖记录, 更新用户已用次数
    /// 6. 返回奖品信息与剩余次数
    pub async fn spin(&self, user_id: i64) -> AppResult<LuckyDrawSpinResponse> {
        let txn = self.pool.begin().await?;

        // 确保用户抽奖统计存在 (FOR SHARE -> 简单场景可不加锁, 本处直接读取然后更新)
        let user_chances = self.ensure_chances_tx(&txn, user_id).await?;

        let remaining = user_chances.total_awarded - user_chances.total_used;
        if remaining <= 0 {
            return Err(AppError::ValidationError("No remaining chances".into()));
        }

        // 读取可用奖品
        let mut prize_list = prizes::Entity::find()
            .filter(prizes::Column::IsActive.eq(true))
            .order_by_asc(prizes::Column::Id)
            .all(&txn)
            .await?;

        // 过滤掉已无库存奖品
        prize_list.retain(|p| p.is_available());

        if prize_list.is_empty() {
            return Err(AppError::InternalError(
                "No available prizes configured".into(),
            ));
        }

        // 选择奖品（支持在某个限量奖品并发扣减失败后重试）
        let selected_prize = self
            .select_and_secure_prize(&txn, &prize_list)
            .await
            .map_err(|e| {
                AppError::InternalError(format!("Prize selection failed: {}", e))
            })?;

        // 更新已用次数
        {
            let mut am = user_chances.clone().into_active_model();
            am.total_used = Set(user_chances.total_used + 1);
            am.updated_at = Set(Some(Utc::now()));
            am.update(&txn).await?;
        }

        // 写抽奖记录
        records::ActiveModel {
            user_id: Set(user_id),
            prize_id: Set(selected_prize.id),
            prize_name_en: Set(selected_prize.name_en.clone()),
            value_cents: Set(selected_prize.value_cents),
            ..Default::default()
        }
        .insert(&txn)
        .await?;

        // 发放实际奖品（优惠券 / 月卡等）
        // 注意：优惠券创建内部会使用新的事务与外部接口；若失败将返回错误并导致本次 spin 事务回滚
        self.award_prize(user_id, &selected_prize).await?;

        // 计算剩余次数
        let remaining_after = user_chances.total_awarded - (user_chances.total_used + 1);

        txn.commit().await?;

        Ok(LuckyDrawSpinResponse {
            prize: LuckyDrawWonPrize::from(selected_prize),
            remaining_chances: remaining_after,
        })
    }

    /// 为用户增加抽奖次数（任务/充值触发）
    /// 业务方可调用此方法进行发放。
    pub async fn award_chances(
        &self,
        user_id: i64,
        count: i64,
    ) -> AppResult<LuckyDrawChancesResponse> {
        if count <= 0 {
            return Err(AppError::ValidationError(
                "Count to award must be positive".into(),
            ));
        }
        let txn = self.pool.begin().await?;
        let model = self.ensure_chances_tx(&txn, user_id).await?;
        // Model 中字段是 i64（非 Option），直接读取当前值再加上新增次数
        let current_total = model.total_awarded;
        let mut am = model.into_active_model();
        am.total_awarded = Set(current_total + count);
        am.updated_at = Set(Some(Utc::now()));
        let updated = am.update(&txn).await?;
        txn.commit().await?;
        Ok(updated.into())
    }

    // -----------------------------
    // 内部辅助方法
    // -----------------------------

    async fn ensure_chances(&self, user_id: i64) -> Result<chances::Model, DbErr> {
        if let Some(m) = chances::Entity::find()
            .filter(chances::Column::UserId.eq(user_id))
            .one(&self.pool)
            .await?
        {
            return Ok(m);
        }
        chances::ActiveModel {
            user_id: Set(user_id),
            total_awarded: Set(0),
            total_used: Set(0),
            ..Default::default()
        }
        .insert(&self.pool)
        .await
    }

    async fn ensure_chances_tx(
        &self,
        txn: &sea_orm::DatabaseTransaction,
        user_id: i64,
    ) -> Result<chances::Model, DbErr> {
        if let Some(m) = chances::Entity::find()
            .filter(chances::Column::UserId.eq(user_id))
            .one(txn)
            .await?
        {
            return Ok(m);
        }
        chances::ActiveModel {
            user_id: Set(user_id),
            total_awarded: Set(0),
            total_used: Set(0),
            ..Default::default()
        }
        .insert(txn)
        .await
    }

    /// 选择并扣减库存（针对限量奖品），失败自动重试。
    async fn select_and_secure_prize(
        &self,
        txn: &sea_orm::DatabaseTransaction,
        available: &[prizes::Model],
    ) -> Result<prizes::Model, DbErr> {
        // 使用循环以处理限量奖品竞争失败的情况
        let mut attempts = 0;
        let mut filtered: Vec<prizes::Model> = available.to_vec();

        while attempts < 5 {
            attempts += 1;

            // 重新计算总概率 (只使用当前可用 prize)
            let total_bp: i32 = filtered.iter().map(|p| p.probability_bp).sum();
            if total_bp <= 0 {
                // 理论上不应发生
                return Err(DbErr::Custom("Total probability <= 0".into()));
            }

            let mut rng = rand::rng();
            let pick: i32 = rng.random_range(0..total_bp);
            let mut acc = 0;
            let mut chosen = filtered.last().expect("Non-empty vector").clone(); // fallback

            for p in &filtered {
                acc += p.probability_bp;
                if pick < acc {
                    chosen = p.clone();
                    break;
                }
            }

            // 若非限量或无限库存直接返回
            if chosen.stock_limit.is_none() {
                return Ok(chosen);
            }

            // 限量奖品: 尝试原子扣减 (where stock_remaining > 0)
            let update_result: UpdateResult = prizes::Entity::update_many()
                .col_expr(
                    prizes::Column::StockRemaining,
                    Expr::col(prizes::Column::StockRemaining).sub(1),
                )
                .filter(prizes::Column::Id.eq(chosen.id))
                .filter(
                    Condition::all()
                        .add(prizes::Column::StockRemaining.is_not_null())
                        .add(prizes::Column::StockRemaining.gt(0)),
                )
                .exec(txn)
                .await?;

            if update_result.rows_affected == 1 {
                // 重新读取最新数据返回
                if let Some(updated) = prizes::Entity::find_by_id(chosen.id).one(txn).await? {
                    return Ok(updated);
                } else {
                    return Err(DbErr::Custom(
                        "Prize disappeared after successful update".into(),
                    ));
                }
            } else {
                // 扣减失败 - 说明库存已为0，过滤掉该奖品重试
                filtered.retain(|p| p.id != chosen.id && p.is_available());
                if filtered.is_empty() {
                    return Err(DbErr::Custom(
                        "No prize available after stock depletion".into(),
                    ));
                }
                continue;
            }
        }

        Err(DbErr::Custom(
            "Failed to select prize after several attempts".into(),
        ))
    }

    /// 根据选中奖品发放对应奖励:
    /// - Free Topping Coupon -> 50 cents, CodeType::FreeTopping
    /// - Free Original Ice Cream Coupon -> 500 cents, CodeType::SweetsCreditsReward
    /// - Half Price Ice Cream Coupon -> 250 cents, CodeType::SweetsCreditsReward
    /// - Membership Monthly Card -> 创建一条月卡记录（立即生效，30天有效）
    /// - Thank You -> 无发放
    async fn award_prize(&self, user_id: i64, prize: &prizes::Model) -> AppResult<()> {
        match prize.name_en.as_str() {
            "Free Topping Coupon" => {
                // 发放免费小料券 (50 cents)
                self.discount_code_service
                    .create_user_discount_code(
                        user_id,
                        DiscountValue::FixedAmount(50),
                        CodeType::FreeTopping,
                        1, // 有效期 1 个月
                    )
                    .await?;
            }
            "Free Original Ice Cream Coupon" => {
                self.discount_code_service
                    .create_user_discount_code(user_id, DiscountValue::FixedAmount(500), CodeType::SweetsCreditsReward, 1)
                    .await?;
            }
            "Half Price Ice Cream Coupon" => {
                self.discount_code_service
                    .create_user_discount_code(user_id, DiscountValue::FixedAmount(250), CodeType::SweetsCreditsReward, 1)
                    .await?;
            }
            "Membership Monthly Card" => {
                // 月卡叠加策略:
                // 若存在仍在有效期内的 Active 月卡, 将其 ends_at 顺延 30 天
                // 否则创建新的月卡记录 (one_time)
                let now = Utc::now();
                if let Some(existing) = mc::Entity::find()
                    .filter(mc::Column::UserId.eq(user_id))
                    .filter(mc::Column::Status.eq(MonthlyCardStatus::Active))
                    .filter(mc::Column::EndsAt.gte(now))
                    .order_by_desc(mc::Column::EndsAt)
                    .one(&self.pool)
                    .await?
                {
                    // 顺延
                    let base_end = existing.ends_at.unwrap_or(now);
                    let mut am = existing.into_active_model();
                    am.ends_at = Set(Some(base_end + Duration::days(30)));
                    am.update(&self.pool).await?;
                } else {
                    // 创建新月卡
                    mc::ActiveModel {
                        user_id: Set(user_id),
                        plan_type: Set(MonthlyCardPlanType::OneTime),
                        status: Set(MonthlyCardStatus::Active),
                        starts_at: Set(Some(now)),
                        ends_at: Set(Some(now + Duration::days(30))),
                        ..Default::default()
                    }
                    .insert(&self.pool)
                    .await?;
                }
            }
            "Thank You" => {
                // 无奖励发放
            }
            _ => {
                // 未知奖品名称（配置错误）- 记日志但不报错，避免用户丢失一次机会
                log::warn!("Unknown prize name encountered: {}", prize.name_en);
            }
        }
        Ok(())
    }
}

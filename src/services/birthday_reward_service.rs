use crate::entities::{
    MemberType, birthday_reward_entity as br, sweet_cash_transaction_entity as sct,
    user_entity as users,
};
use crate::error::AppResult;
use chrono::{Datelike, Utc};
use sea_orm::sea_query::{OnConflict, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, QueryFilter, Set, TransactionTrait,
};

#[derive(Clone)]
pub struct BirthdayRewardService {
    pool: DatabaseConnection,
}

impl BirthdayRewardService {
    pub fn new(pool: DatabaseConnection) -> Self {
        Self { pool }
    }

    // 给今天生日且今年未领取过的用户发放生日福利；返回发放人数
    pub async fn grant_today_birthdays(&self) -> AppResult<i64> {
        let today = Utc::now().date_naive();
        let month = today.month();
        let day = today.day();
        let year = Utc::now().year();

        // 依赖新增的 birthday_month / birthday_day 字段，用 SeaORM 过滤
        let users_today = users::Entity::find()
            .filter(users::Column::BirthdayMonth.eq(month as i16))
            .filter(users::Column::BirthdayDay.eq(day as i16))
            .all(&self.pool)
            .await?;

        let mut granted = 0i64;
        for u in users_today {
            let amount = match u.member_type {
                MemberType::Fan => 50,               // $0.5
                MemberType::SweetShareholder => 550, // $5.5
                MemberType::SuperShareholder => 800, // $8
            };

            self.grant_single(u, amount, year).await?;
            granted += 1;
        }
        Ok(granted)
    }

    async fn grant_single(&self, user: users::Model, amount: i64, year: i32) -> AppResult<()> {
        let txn = self.pool.begin().await?;
        // 使用 Upsert 语义：插入标记，若已存在则不影响（DO NOTHING）
        let insert = Query::insert()
            .into_table(br::Entity)
            .columns([
                br::Column::UserId,
                br::Column::RewardYear,
                br::Column::Amount,
            ])
            .values_panic([user.id.into(), year.into(), amount.into()])
            .on_conflict(
                OnConflict::columns([br::Column::UserId, br::Column::RewardYear])
                    .do_nothing()
                    .to_owned(),
            )
            .to_owned();
        // SeaORM 2.0: `execute` 直接接收 SeaQuery 语句
        let res = txn.execute(&insert).await?;
        if res.rows_affected() == 0 {
            // 已发放过，跳过
            txn.commit().await?;
            return Ok(());
        }

        // 增加用户余额
        let current = users::Entity::find_by_id(user.id).one(&txn).await?.unwrap();
        let new_balance = current.balance.unwrap_or(0) + amount;
        let mut am = current.into_active_model();
        am.balance = Set(Some(new_balance));
        am.update(&txn).await?;

        // 记 sweet_cash_transactions
        sct::ActiveModel {
            user_id: Set(user.id),
            transaction_type: Set(sct::TransactionType::Earn),
            amount: Set(amount),
            balance_after: Set(new_balance),
            related_order_id: Set(None),
            related_discount_code_id: Set(None),
            description: Set(Some("Birthday reward".to_string())),
            ..Default::default()
        }
        .insert(&txn)
        .await?;

        txn.commit().await?;
        Ok(())
    }
}

use crate::entities::user_entity;
use crate::error::AppResult;
use rand::RngExt;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

/// 生成唯一的六位数字推荐码
pub async fn generate_unique_referral_code(db: &DatabaseConnection) -> AppResult<String> {
    let mut rng = rand::rng();

    loop {
        // 生成100000到999999之间的六位数字
        let referral_code = rng.random_range(100000_u32..=999999_u32).to_string();

        // 检查是否已存在
        let exists = user_entity::Entity::find()
            .filter(user_entity::Column::ReferralCode.eq(referral_code.clone()))
            .one(db)
            .await?
            .is_some();

        if !exists {
            return Ok(referral_code);
        }
    }
}

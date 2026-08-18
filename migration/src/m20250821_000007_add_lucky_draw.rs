use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::Statement;

/// Lucky Draw Chances (用户抽奖次数统计)
#[derive(DeriveIden)]
enum LuckyDrawChances {
    Table,
    Id,
    UserId,
    TotalAwarded,
    TotalUsed,
    CreatedAt,
    UpdatedAt,
}

/// Lucky Draw Prizes (奖品配置表)
#[derive(DeriveIden)]
enum LuckyDrawPrizes {
    Table,
    Id,
    NameEn,
    ValueCents,
    ProbabilityBp,
    StockLimit,
    StockRemaining,
    IsActive,
    CreatedAt,
    UpdatedAt,
}

/// Lucky Draw Records (用户抽奖记录)
#[derive(DeriveIden)]
enum LuckyDrawRecords {
    Table,
    Id,
    UserId,
    PrizeId,
    PrizeNameEn,
    ValueCents,
    CreatedAt,
}

#[derive(DeriveMigrationName)]
pub struct Migration;

/// 概率使用 basis points (bp) 形式，100% = 10000bp
/// 奖品初始配置（英文名称要求，中文仅做注释说明）:
/// - Free Topping Coupon ($0.50) 45% -> 4500
/// - Free Original Ice Cream Coupon ($5.00) 8% -> 800
/// - Membership Monthly Card (月卡) 0.5% -> 50  (限量5)
/// - Half Price Ice Cream Coupon ($2.50) 12% -> 1200
/// - Thank You (谢谢参与) 34.5% -> 3450
///
/// 仅 Membership Monthly Card 有库存限制，其它为无限库存 (stock_limit NULL)
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 抽奖次数统计表
        manager
            .create_table(
                Table::create()
                    .table(LuckyDrawChances::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LuckyDrawChances::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawChances::UserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawChances::TotalAwarded)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawChances::TotalUsed)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawChances::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("NOW()")),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawChances::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("NOW()")),
                    )
                    .to_owned(),
            )
            .await?;

        // user_id 唯一索引（一个用户一条统计记录）
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_lucky_draw_chances_user_unique")
                    .table(LuckyDrawChances::Table)
                    .col(LuckyDrawChances::UserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // 奖品表
        manager
            .create_table(
                Table::create()
                    .table(LuckyDrawPrizes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LuckyDrawPrizes::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawPrizes::NameEn)
                            .string_len(255)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawPrizes::ValueCents)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawPrizes::ProbabilityBp)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawPrizes::StockLimit)
                            .big_integer()
                            .null(), // NULL = 无限库存
                    )
                    .col(
                        ColumnDef::new(LuckyDrawPrizes::StockRemaining)
                            .big_integer()
                            .null(), // 与 StockLimit 对应，NULL 表示不需要
                    )
                    .col(
                        ColumnDef::new(LuckyDrawPrizes::IsActive)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawPrizes::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("NOW()")),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawPrizes::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("NOW()")),
                    )
                    .to_owned(),
            )
            .await?;

        // 奖品英文名唯一
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_lucky_draw_prizes_name_en_unique")
                    .table(LuckyDrawPrizes::Table)
                    .col(LuckyDrawPrizes::NameEn)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // 抽奖记录表
        manager
            .create_table(
                Table::create()
                    .table(LuckyDrawRecords::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LuckyDrawRecords::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawRecords::UserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawRecords::PrizeId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawRecords::PrizeNameEn)
                            .string_len(255)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawRecords::ValueCents)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(LuckyDrawRecords::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::cust("NOW()")),
                    )
                    .to_owned(),
            )
            .await?;

        // 用户查询记录索引
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_lucky_draw_records_user")
                    .table(LuckyDrawRecords::Table)
                    .col(LuckyDrawRecords::UserId)
                    .to_owned(),
            )
            .await?;

        // 奖品外键索引
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_lucky_draw_records_prize")
                    .table(LuckyDrawRecords::Table)
                    .col(LuckyDrawRecords::PrizeId)
                    .to_owned(),
            )
            .await?;

        // 外键（可选，不加 ON DELETE CASCADE，保证历史记录仍然存在）
        manager
            .alter_table(
                Table::alter()
                    .table(LuckyDrawRecords::Table)
                    .add_foreign_key(
                        TableForeignKey::new()
                            .name("fk_lucky_draw_record_prize")
                            .from_tbl(LuckyDrawRecords::Table)
                            .from_col(LuckyDrawRecords::PrizeId)
                            .to_tbl(LuckyDrawPrizes::Table)
                            .to_col(LuckyDrawPrizes::Id),
                    )
                    .to_owned(),
            )
            .await?;

        // 初始化奖品数据
        // 注意：库存限制仅月卡，其它库存无限（stock_limit/stock_remaining = NULL）
        let conn = manager.get_connection();
        let insert_sql = r#"
INSERT INTO lucky_draw_prizes (name_en, value_cents, probability_bp, stock_limit, stock_remaining, is_active)
VALUES
 ('Free Topping Coupon', 50, 4500, NULL, NULL, TRUE),          -- 免费小料券
 ('Free Original Ice Cream Coupon', 500, 800, NULL, NULL, TRUE),-- 免费原味冰激凌券
 ('Membership Monthly Card', 0, 50, 5, 5, TRUE),                -- 会员月卡（限量5）
 ('Half Price Ice Cream Coupon', 250, 1200, NULL, NULL, TRUE),  -- 冰激凌半价券
 ('Thank You', 0, 3450, NULL, NULL, TRUE)                      -- 谢谢参与
ON CONFLICT (name_en) DO NOTHING;
"#;
        conn.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            insert_sql.to_string(),
        ))
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 删除顺序：记录 -> 奖品 -> 次数
        manager
            .drop_table(
                Table::drop()
                    .if_exists()
                    .table(LuckyDrawRecords::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .if_exists()
                    .table(LuckyDrawPrizes::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(
                Table::drop()
                    .if_exists()
                    .table(LuckyDrawChances::Table)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

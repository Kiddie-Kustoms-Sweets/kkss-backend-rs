use sea_orm::{ConnectionTrait, Statement};
use sea_orm_migration::prelude::extension::postgres::Type;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create enum type transaction_type if not exists
        manager
            .create_type(
                Type::create()
                    .as_enum(Alias::new("transaction_type"))
                    .values(vec![Alias::new("earn"), Alias::new("redeem")])
                    .to_owned(),
            )
            .await?;

        // Alter column to enum with explicit USING cast
        let db = manager.get_connection();
        // 1) add temp column
        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "ALTER TABLE \"sweet_cash_transactions\" ADD COLUMN \"transaction_type_new\" transaction_type".to_string(),
        ))
        .await?;

        // 2) copy data with explicit mapping
        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "UPDATE \"sweet_cash_transactions\" SET \"transaction_type_new\" = \
             CASE \"transaction_type\" WHEN 'earn' THEN 'earn'::transaction_type WHEN 'redeem' THEN 'redeem'::transaction_type ELSE 'earn'::transaction_type END"
                .to_string(),
        ))
        .await?;

        // 3) drop index (if exists) on old column
        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "DROP INDEX IF EXISTS idx_sweet_cash_transactions_transaction_type".to_string(),
        ))
        .await?;

        // 4) drop old column
        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "ALTER TABLE \"sweet_cash_transactions\" DROP COLUMN \"transaction_type\"".to_string(),
        ))
        .await?;

        // 5) rename new column
        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "ALTER TABLE \"sweet_cash_transactions\" RENAME COLUMN \"transaction_type_new\" TO \"transaction_type\"".to_string(),
        ))
        .await?;

        // 6) set NOT NULL
        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "ALTER TABLE \"sweet_cash_transactions\" ALTER COLUMN \"transaction_type\" SET NOT NULL".to_string(),
        ))
        .await?;

        // 7) recreate index
        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "CREATE INDEX IF NOT EXISTS idx_sweet_cash_transactions_transaction_type ON \"sweet_cash_transactions\" (\"transaction_type\")".to_string(),
        ))
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

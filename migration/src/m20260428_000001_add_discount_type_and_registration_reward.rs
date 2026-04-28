use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create discount_type enum (if not exists)
        let create_enum = Statement::from_string(
            manager.get_database_backend(),
            "DO $$ BEGIN CREATE TYPE discount_type AS ENUM ('fixed_amount', 'percentage'); EXCEPTION WHEN duplicate_object THEN null; END $$;".to_string(),
        );
        manager.get_connection().execute(create_enum).await?;

        // Add discount_type column to discount_codes table
        let add_col = Statement::from_string(
            manager.get_database_backend(),
            "ALTER TABLE discount_codes ADD COLUMN IF NOT EXISTS discount_type discount_type NOT NULL DEFAULT 'fixed_amount'".to_string(),
        );
        manager.get_connection().execute(add_col).await?;

        // Add registration_reward to code_type enum
        let add_enum = Statement::from_string(
            manager.get_database_backend(),
            "ALTER TYPE code_type ADD VALUE IF NOT EXISTS 'registration_reward'".to_string(),
        );
        manager.get_connection().execute(add_enum).await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // No easy way to drop enum value or remove column with default in PostgreSQL
        Ok(())
    }
}

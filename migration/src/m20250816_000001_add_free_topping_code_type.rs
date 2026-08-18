use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Append new enum value 'free_topping' to code_type
        let stmt = Statement::from_string(
            manager.get_database_backend(),
            "ALTER TYPE code_type ADD VALUE IF NOT EXISTS 'free_topping'".to_string(),
        );
        manager.get_connection().execute_raw(stmt).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // No easy way to drop enum value in PostgreSQL; noop
        Ok(())
    }
}

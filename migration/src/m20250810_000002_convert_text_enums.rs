use sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create Postgres ENUM types (idempotent)
        let db = manager.get_connection();
        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "DO $$ BEGIN \n  CREATE TYPE member_type AS ENUM ('fan','sweet_shareholder','super_shareholder');\nEXCEPTION WHEN duplicate_object THEN NULL; END $$;".to_string(),
        ))
        .await?;

        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "DO $$ BEGIN \n  CREATE TYPE code_type AS ENUM ('welcome','referral','purchase_reward','redeemed');\nEXCEPTION WHEN duplicate_object THEN NULL; END $$;".to_string(),
        ))
        .await?;

        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "DO $$ BEGIN \n  CREATE TYPE recharge_status AS ENUM ('pending','succeeded','failed','canceled');\nEXCEPTION WHEN duplicate_object THEN NULL; END $$;".to_string(),
        ))
        .await?;

        // Alter columns to use enum types with explicit USING casts
        let db = manager.get_connection();

        // users.member_type -> member_type
        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "ALTER TABLE \"users\" \n             ALTER COLUMN \"member_type\" TYPE member_type \n             USING \"member_type\"::member_type".to_string(),
        ))
        .await?;

        // discount_codes.code_type -> code_type
        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "ALTER TABLE \"discount_codes\" \n             ALTER COLUMN \"code_type\" TYPE code_type \n             USING \"code_type\"::code_type".to_string(),
        ))
        .await?;

        // recharge_records.status -> recharge_status
        db.execute_raw(Statement::from_string(
            manager.get_database_backend(),
            "ALTER TABLE \"recharge_records\" \n             ALTER COLUMN \"status\" TYPE recharge_status \n             USING \"status\"::recharge_status".to_string(),
        ))
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

use sea_orm::Statement;
use sea_orm_migration::prelude::extension::postgres::Type;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1) Create a new enum type with the desired final variants
        manager
            .create_type(
                Type::create()
                    .as_enum(Alias::new("code_type_new"))
                    .values(vec![
                        Alias::new("shareholder_reward"),
                        Alias::new("super_shareholder_reward"),
                        Alias::new("sweets_credits_reward"),
                    ])
                    .to_owned(),
            )
            .await?;

        // 2) Convert the column to TEXT explicitly using USING, so we can rewrite values safely
        manager
            .get_connection()
            .execute_raw(Statement::from_string(
                manager.get_database_backend(),
                "ALTER TABLE \"discount_codes\" ALTER COLUMN \"code_type\" TYPE TEXT USING \"code_type\"::TEXT".to_string(),
            ))
            .await?;

        // 3) Remap data to the new labels
        let stmt = Statement::from_string(
            manager.get_database_backend(),
            r#"UPDATE discount_codes SET code_type = CASE code_type
                WHEN 'welcome' THEN 'shareholder_reward'
                WHEN 'referral' THEN 'sweets_credits_reward'
                WHEN 'purchase_reward' THEN 'shareholder_reward'
                WHEN 'redeemed' THEN 'sweets_credits_reward'
                ELSE 'sweets_credits_reward'
            END"#
                .to_owned(),
        );
        manager.get_connection().execute_raw(stmt).await?;

        // 4) Convert the column to the new enum type explicitly with USING
        manager
            .get_connection()
            .execute_raw(Statement::from_string(
                manager.get_database_backend(),
                "ALTER TABLE \"discount_codes\" ALTER COLUMN \"code_type\" TYPE code_type_new USING \"code_type\"::code_type_new".to_string(),
            ))
            .await?;

        // 5) Drop the old enum type now that nothing references it
        manager
            .drop_type(Type::drop().name(Alias::new("code_type")).to_owned())
            .await?;

        // 6) Rename the new enum type to the original name for backward compatibility
        manager
            .get_connection()
            .execute_raw(Statement::from_string(
                manager.get_database_backend(),
                "ALTER TYPE code_type_new RENAME TO code_type".to_string(),
            ))
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

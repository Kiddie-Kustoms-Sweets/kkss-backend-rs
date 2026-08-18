use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum Users {
    Table,
    MembershipExpiresAt,
}

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager.has_column("users", "membership_expires_at").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(Users::Table)
                        .add_column(
                            ColumnDef::new(Users::MembershipExpiresAt)
                                .timestamp_with_time_zone()
                                .null(),
                        )
                        .to_owned(),
                )
                .await?;
        }

        // Create partial index using raw statement (SeaQuery doesn't support WHERE on index yet)
        let stmt = sea_orm::Statement::from_string(
            manager.get_database_backend(),
            "CREATE INDEX IF NOT EXISTS idx_users_membership_expires_at ON users(membership_expires_at) WHERE membership_expires_at IS NOT NULL".to_owned(),
        );
        manager.get_connection().execute_raw(stmt).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

//! doc schema 迁移。

use sea_orm_migration::prelude::*;

/// 初始化迁移。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("CREATE SCHEMA IF NOT EXISTS doc")
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Spaces::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Spaces::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Spaces::Name).string_len(64).not_null())
                    .col(ColumnDef::new(Spaces::Type).string_len(16).not_null())
                    .col(ColumnDef::new(Spaces::OwnerId).uuid().not_null())
                    .col(
                        ColumnDef::new(Spaces::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(SpaceMembers::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(SpaceMembers::SpaceId).uuid().not_null())
                    .col(ColumnDef::new(SpaceMembers::UserId).uuid().not_null())
                    .col(ColumnDef::new(SpaceMembers::Role).string_len(16).not_null())
                    .col(
                        ColumnDef::new(SpaceMembers::JoinedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(SpaceMembers::SpaceId)
                            .col(SpaceMembers::UserId),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Nodes::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Nodes::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Nodes::SpaceId).uuid().not_null())
                    .col(ColumnDef::new(Nodes::ParentId).uuid())
                    .col(ColumnDef::new(Nodes::Kind).string_len(16).not_null())
                    .col(ColumnDef::new(Nodes::Title).string_len(255).not_null())
                    .col(
                        ColumnDef::new(Nodes::Position)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Nodes::Version)
                            .big_integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(Nodes::ContentMd)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(ColumnDef::new(Nodes::UpdatedBy).uuid().not_null())
                    .col(ColumnDef::new(Nodes::DeletedAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Nodes::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Nodes::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ix_doc_nodes_space_parent")
                    .table(Nodes::Table)
                    .col(Nodes::SpaceId)
                    .col(Nodes::ParentId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(PageVersions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PageVersions::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(PageVersions::NodeId).uuid().not_null())
                    .col(
                        ColumnDef::new(PageVersions::Version)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(PageVersions::ContentMd).text().not_null())
                    .col(ColumnDef::new(PageVersions::AuthorId).uuid().not_null())
                    .col(
                        ColumnDef::new(PageVersions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("ux_doc_versions_node_version")
                    .table(PageVersions::Table)
                    .col(PageVersions::NodeId)
                    .col(PageVersions::Version)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(PageVersions::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(Nodes::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(SpaceMembers::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(Spaces::Table).if_exists().to_owned())
            .await?;
        Ok(())
    }
}

/// 迁移入口。
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(Migration),
            Box::new(crate::migration2::Migration),
        ]
    }
}

/// spaces 表标识符。
#[derive(DeriveIden)]
pub enum Spaces {
    /// 表。
    Table,
    /// id。
    Id,
    /// name。
    Name,
    /// type。
    Type,
    /// owner_id。
    OwnerId,
    /// created_at。
    CreatedAt,
}

/// space_members 表标识符。
#[derive(DeriveIden)]
pub enum SpaceMembers {
    /// 表。
    Table,
    /// space_id。
    SpaceId,
    /// user_id。
    UserId,
    /// role。
    Role,
    /// joined_at。
    JoinedAt,
}

/// nodes 表标识符。
#[derive(DeriveIden)]
pub enum Nodes {
    /// 表。
    Table,
    /// id。
    Id,
    /// space_id。
    SpaceId,
    /// parent_id。
    ParentId,
    /// kind。
    Kind,
    /// title。
    Title,
    /// position。
    Position,
    /// version。
    Version,
    /// content_md。
    ContentMd,
    /// updated_by。
    UpdatedBy,
    /// deleted_at。
    DeletedAt,
    /// created_at。
    CreatedAt,
    /// updated_at。
    UpdatedAt,
}

/// page_versions 表标识符。
#[derive(DeriveIden)]
pub enum PageVersions {
    /// 表。
    Table,
    /// id。
    Id,
    /// node_id。
    NodeId,
    /// version。
    Version,
    /// content_md。
    ContentMd,
    /// author_id。
    AuthorId,
    /// created_at。
    CreatedAt,
}

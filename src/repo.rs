//! 数据访问层：只操作 doc schema。

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr,
    EntityTrait, QueryFilter, QueryOrder, Set, Statement,
};
use uuid::Uuid;

use club_common::{new_id, AppError};

use crate::entity::{node, space, space_member, version};

/// 数据库错误 → 统一错误。
pub fn map_db_err(err: DbErr) -> AppError {
    AppError::internal(err)
}

/// 创建空间并授权所有者。
pub async fn create_space(
    db: &DatabaseConnection,
    owner_id: Uuid,
    name: &str,
    kind: &str,
    now: DateTime<Utc>,
) -> Result<space::Model, AppError> {
    let model = space::ActiveModel {
        id: Set(new_id()),
        name: Set(name.to_string()),
        r#type: Set(kind.to_string()),
        owner_id: Set(owner_id),
        created_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)?;
    space_member::ActiveModel {
        space_id: Set(model.id),
        user_id: Set(owner_id),
        role: Set(space_member::ROLE_ADMIN.to_string()),
        joined_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)?;
    Ok(model)
}

/// 用户可访问的空间。
pub async fn list_spaces(
    db: &DatabaseConnection,
    user_id: Uuid,
) -> Result<Vec<space::Model>, AppError> {
    let members = space_member::Entity::find()
        .filter(space_member::Column::UserId.eq(user_id))
        .all(db)
        .await
        .map_err(map_db_err)?;
    let ids: Vec<Uuid> = members.iter().map(|m| m.space_id).collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    space::Entity::find()
        .filter(space::Column::Id.is_in(ids))
        .order_by_asc(space::Column::CreatedAt)
        .all(db)
        .await
        .map_err(map_db_err)
}

/// 校验成员并返回成员记录。
pub async fn ensure_member(
    db: &DatabaseConnection,
    space_id: Uuid,
    user_id: Uuid,
) -> Result<space_member::Model, AppError> {
    space_member::Entity::find_by_id((space_id, user_id))
        .one(db)
        .await
        .map_err(map_db_err)?
        .ok_or_else(|| AppError::forbidden("DOC_NOT_MEMBER", "无权访问该空间"))
}

/// 校验可编辑（admin/editor）。
pub async fn ensure_editor(
    db: &DatabaseConnection,
    space_id: Uuid,
    user_id: Uuid,
) -> Result<space_member::Model, AppError> {
    let member = ensure_member(db, space_id, user_id).await?;
    if matches!(
        member.role.as_str(),
        space_member::ROLE_ADMIN | space_member::ROLE_EDITOR
    ) {
        Ok(member)
    } else {
        Err(AppError::forbidden(
            "DOC_FORBIDDEN",
            "仅管理员/编辑者可修改",
        ))
    }
}

/// 创建节点（文件夹/页面）。
pub async fn create_node(
    db: &DatabaseConnection,
    space_id: Uuid,
    parent_id: Option<Uuid>,
    kind: &str,
    title: &str,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<node::Model, AppError> {
    // 同级 position：现有最大 +1（单实例够用，冲突时排序稳定即可）
    let max_position: Option<i64> = node::Entity::find()
        .filter(node::Column::SpaceId.eq(space_id))
        .filter(match parent_id {
            Some(parent) => node::Column::ParentId.eq(parent),
            None => node::Column::ParentId.is_null(),
        })
        .order_by_desc(node::Column::Position)
        .one(db)
        .await
        .map_err(map_db_err)?
        .map(|row| row.position);
    node::ActiveModel {
        id: Set(new_id()),
        space_id: Set(space_id),
        parent_id: Set(parent_id),
        kind: Set(kind.to_string()),
        title: Set(title.to_string()),
        position: Set(max_position.unwrap_or(0) + 1),
        version: Set(1),
        content_md: Set(String::new()),
        updated_by: Set(user_id),
        deleted_at: Set(None),
        created_at: Set(now.fixed_offset()),
        updated_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)
}

/// 查找节点（含已删除）。
pub async fn find_node(
    db: &DatabaseConnection,
    node_id: Uuid,
) -> Result<Option<node::Model>, AppError> {
    node::Entity::find_by_id(node_id)
        .one(db)
        .await
        .map_err(map_db_err)
}

/// 空间全部有效节点（树构建用）。
pub async fn list_nodes(
    db: &DatabaseConnection,
    space_id: Uuid,
) -> Result<Vec<node::Model>, AppError> {
    node::Entity::find()
        .filter(node::Column::SpaceId.eq(space_id))
        .filter(node::Column::DeletedAt.is_null())
        .order_by_asc(node::Column::Position)
        .all(db)
        .await
        .map_err(map_db_err)
}

/// 乐观锁更新正文：`base_version` 不匹配返回 409；成功则版本 +1 并留快照。
pub async fn update_content(
    db: &DatabaseConnection,
    node: &node::Model,
    base_version: i64,
    content: &str,
    editor: Uuid,
    now: DateTime<Utc>,
) -> Result<node::Model, AppError> {
    if node.version != base_version {
        return Err(AppError::conflict(
            "DOC_VERSION_CONFLICT",
            "内容已被他人修改，请刷新后重试",
        ));
    }
    // 快照当前版本
    version::ActiveModel {
        id: Set(new_id()),
        node_id: Set(node.id),
        version: Set(node.version),
        content_md: Set(node.content_md.clone()),
        author_id: Set(node.updated_by),
        created_at: Set(now.fixed_offset()),
    }
    .insert(db)
    .await
    .map_err(map_db_err)?;

    // 条件更新：version = base 才生效（并发下仅一个成功）
    let statement = Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "UPDATE nodes SET content_md = $2, version = version + 1, updated_by = $3, updated_at = $4 \
         WHERE id = $1 AND version = $5 RETURNING version",
        vec![
            node.id.into(),
            content.into(),
            editor.into(),
            now.fixed_offset().into(),
            base_version.into(),
        ],
    );
    let row = db.query_one_raw(statement).await.map_err(map_db_err)?;
    if row.is_none() {
        return Err(AppError::conflict(
            "DOC_VERSION_CONFLICT",
            "内容已被他人修改，请刷新后重试",
        ));
    }
    find_node(db, node.id)
        .await?
        .ok_or_else(|| AppError::not_found("DOC_NODE_NOT_FOUND", "节点不存在"))
}

/// 历史版本列表（新到旧）。
pub async fn list_versions(
    db: &DatabaseConnection,
    node_id: Uuid,
) -> Result<Vec<version::Model>, AppError> {
    version::Entity::find()
        .filter(version::Column::NodeId.eq(node_id))
        .order_by_desc(version::Column::Version)
        .all(db)
        .await
        .map_err(map_db_err)
}

/// 查找指定历史版本。
pub async fn find_version(
    db: &DatabaseConnection,
    node_id: Uuid,
    version_number: i64,
) -> Result<Option<version::Model>, AppError> {
    version::Entity::find()
        .filter(version::Column::NodeId.eq(node_id))
        .filter(version::Column::Version.eq(version_number))
        .one(db)
        .await
        .map_err(map_db_err)
}

/// 当前位置（数字越小越靠前）。
pub fn next_position_from(max: Option<i64>) -> i64 {
    max.unwrap_or(0) + 1
}

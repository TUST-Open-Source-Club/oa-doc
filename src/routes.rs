//! HTTP 路由。

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use club_auth_sdk::AuthUser;
use club_common::{AppError, FieldError};

use crate::entity::{node, space};
use crate::repo;
use crate::state::SharedState;

/// 存活检查。
pub async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// 就绪检查。
pub async fn readyz(State(state): State<SharedState>) -> Json<Value> {
    match state.db.ping().await {
        Ok(_) => Json(json!({ "status": "ready", "database": "ok" })),
        Err(err) => {
            tracing::error!(error = %err, "数据库就绪检查失败");
            Json(json!({ "status": "degraded", "database": "error" }))
        }
    }
}

/// 用户 ID。
fn user_id_of(auth: &AuthUser) -> Result<Uuid, AppError> {
    auth.claims()
        .sub
        .parse()
        .map_err(|_| AppError::unauthorized("AUTH_INVALID_TOKEN", "访问令牌无效"))
}

/// 节点 DTO。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDto {
    /// ID。
    pub id: String,
    /// 父节点。
    pub parent_id: Option<String>,
    /// folder / page。
    pub kind: String,
    /// 标题。
    pub title: String,
    /// 排序。
    pub position: i64,
    /// 版本。
    pub version: i64,
    /// 更新时间。
    pub updated_at: DateTime<FixedOffset>,
}

impl From<&node::Model> for NodeDto {
    fn from(model: &node::Model) -> Self {
        Self {
            id: model.id.to_string(),
            parent_id: model.parent_id.map(|id| id.to_string()),
            kind: model.kind.clone(),
            title: model.title.clone(),
            position: model.position,
            version: model.version,
            updated_at: model.updated_at,
        }
    }
}

/// 创建空间请求。
#[derive(Debug, Deserialize)]
pub struct CreateSpaceRequest {
    /// 名称。
    pub name: String,
    /// team/personal/public。
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// `POST /spaces`。
pub async fn create_space(
    State(state): State<SharedState>,
    auth: AuthUser,
    Json(input): Json<CreateSpaceRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let user_id = user_id_of(&auth)?;
    let name = input.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(AppError::unprocessable(
            "DOC_VALIDATION",
            "空间名称需为 1 ~ 64 字符",
            vec![FieldError::new("name", "非法")],
        ));
    }
    let kind = input.kind.unwrap_or_else(|| space::TYPE_TEAM.to_string());
    if ![space::TYPE_TEAM, space::TYPE_PERSONAL, space::TYPE_PUBLIC].contains(&kind.as_str()) {
        return Err(AppError::unprocessable(
            "DOC_VALIDATION",
            "空间类型不合法",
            vec![FieldError::new("type", "仅支持 team/personal/public")],
        ));
    }
    let model = repo::create_space(&state.db, user_id, name, &kind, state.now()).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": model.id, "name": model.name, "type": model.r#type })),
    ))
}

/// `GET /spaces`。
pub async fn list_spaces(
    State(state): State<SharedState>,
    auth: AuthUser,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    let spaces = repo::list_spaces(&state.db, user_id).await?;
    Ok(Json(json!(spaces
        .iter()
        .map(|space| json!({ "id": space.id, "name": space.name, "type": space.r#type }))
        .collect::<Vec<_>>())))
}

/// `GET /spaces/{id}/tree`：嵌套树。
pub async fn space_tree(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(space_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_member(&state.db, space_id, user_id).await?;
    let nodes = repo::list_nodes(&state.db, space_id).await?;
    Ok(Json(build_tree(&nodes, None)))
}

/// 递归构建树。
fn build_tree(nodes: &[node::Model], parent: Option<Uuid>) -> Value {
    let children: Vec<Value> = nodes
        .iter()
        .filter(|item| item.parent_id == parent)
        .map(|item| {
            let mut entry = serde_json::to_value(NodeDto::from(item)).unwrap_or(Value::Null);
            entry["children"] = build_tree(nodes, Some(item.id));
            entry
        })
        .collect();
    Value::Array(children)
}

/// 创建节点请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNodeRequest {
    /// folder / page。
    pub kind: String,
    /// 标题。
    pub title: String,
    /// 父节点。
    pub parent_id: Option<Uuid>,
}

/// `POST /spaces/{id}/nodes`。
pub async fn create_node(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path(space_id): Path<Uuid>,
    Json(input): Json<CreateNodeRequest>,
) -> Result<(StatusCode, Json<NodeDto>), AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_editor(&state.db, space_id, user_id).await?;
    if ![node::KIND_FOLDER, node::KIND_PAGE].contains(&input.kind.as_str()) {
        return Err(AppError::unprocessable(
            "DOC_VALIDATION",
            "节点类型不合法",
            vec![FieldError::new("kind", "仅支持 folder/page")],
        ));
    }
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 255 {
        return Err(AppError::unprocessable(
            "DOC_VALIDATION",
            "标题需为 1 ~ 255 字符",
            vec![FieldError::new("title", "非法")],
        ));
    }
    if let Some(parent_id) = input.parent_id {
        let parent = repo::find_node(&state.db, parent_id)
            .await?
            .filter(|item| item.space_id == space_id && item.deleted_at.is_none())
            .ok_or_else(|| AppError::not_found("DOC_NODE_NOT_FOUND", "父节点不存在"))?;
        if parent.kind != node::KIND_FOLDER {
            return Err(AppError::unprocessable(
                "DOC_VALIDATION",
                "父节点必须是文件夹",
                vec![FieldError::new("parentId", "必须是文件夹")],
            ));
        }
    }
    let model = repo::create_node(
        &state.db,
        space_id,
        input.parent_id,
        &input.kind,
        title,
        user_id,
        state.now(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(NodeDto::from(&model))))
}

/// `GET /spaces/{id}/nodes/{node_id}`：节点详情（含正文）。
pub async fn get_node(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((space_id, node_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_member(&state.db, space_id, user_id).await?;
    let model = load_node(&state, space_id, node_id).await?;
    Ok(Json(json!({
        "id": model.id,
        "kind": model.kind,
        "title": model.title,
        "version": model.version,
        "contentMd": model.content_md,
        "updatedAt": model.updated_at
    })))
}

/// 加载并校验节点。
async fn load_node(
    state: &SharedState,
    space_id: Uuid,
    node_id: Uuid,
) -> Result<node::Model, AppError> {
    repo::find_node(&state.db, node_id)
        .await?
        .filter(|item| item.space_id == space_id && item.deleted_at.is_none())
        .ok_or_else(|| AppError::not_found("DOC_NODE_NOT_FOUND", "节点不存在"))
}

/// 更新正文请求。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateContentRequest {
    /// Markdown 正文。
    pub content_md: String,
    /// 期望的当前版本（乐观锁）。
    pub base_version: i64,
}

/// `PUT /spaces/{id}/nodes/{node_id}/content`。
pub async fn update_content(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((space_id, node_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateContentRequest>,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_editor(&state.db, space_id, user_id).await?;
    let model = load_node(&state, space_id, node_id).await?;
    if model.kind != node::KIND_PAGE {
        return Err(AppError::unprocessable(
            "DOC_VALIDATION",
            "仅页面可编辑正文",
            vec![FieldError::new("contentMd", "文件夹无正文")],
        ));
    }
    let updated = repo::update_content(
        &state.db,
        &model,
        input.base_version,
        &input.content_md,
        user_id,
        state.now(),
    )
    .await?;
    Ok(Json(
        json!({ "version": updated.version, "updatedAt": updated.updated_at }),
    ))
}

/// `GET /spaces/{id}/nodes/{node_id}/versions`。
pub async fn list_versions(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((space_id, node_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_member(&state.db, space_id, user_id).await?;
    load_node(&state, space_id, node_id).await?;
    let versions = repo::list_versions(&state.db, node_id).await?;
    Ok(Json(json!(versions
        .iter()
        .map(|item| json!({
            "version": item.version,
            "authorId": item.author_id,
            "createdAt": item.created_at
        }))
        .collect::<Vec<_>>())))
}

/// `POST /spaces/{id}/nodes/{node_id}/versions/{version}/restore`：回滚到历史版本。
pub async fn restore_version(
    State(state): State<SharedState>,
    auth: AuthUser,
    Path((space_id, node_id, target_version)): Path<(Uuid, Uuid, i64)>,
) -> Result<Json<Value>, AppError> {
    let user_id = user_id_of(&auth)?;
    repo::ensure_editor(&state.db, space_id, user_id).await?;
    let model = load_node(&state, space_id, node_id).await?;
    let target = repo::find_version(&state.db, node_id, target_version)
        .await?
        .ok_or_else(|| AppError::not_found("DOC_VERSION_NOT_FOUND", "历史版本不存在"))?;
    let updated = repo::update_content(
        &state.db,
        &model,
        model.version,
        &target.content_md,
        user_id,
        state.now(),
    )
    .await?;
    Ok(Json(json!({ "version": updated.version })))
}

/// `/api/v1/doc` 路由。
pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/spaces", get(list_spaces).post(create_space))
        .route("/spaces/{id}/tree", get(space_tree))
        .route("/spaces/{id}/nodes", post(create_node))
        .route("/spaces/{id}/nodes/{node_id}", get(get_node))
        .route(
            "/spaces/{id}/nodes/{node_id}/content",
            axum::routing::put(update_content),
        )
        .route("/spaces/{id}/nodes/{node_id}/versions", get(list_versions))
        .route(
            "/spaces/{id}/nodes/{node_id}/versions/{version}/restore",
            post(restore_version),
        )
}

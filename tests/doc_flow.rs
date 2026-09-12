//! doc 集成测试：空间/树/乐观锁/版本回滚 + 权限与校验。

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;
use uuid::Uuid;

async fn create_space(app: &TestApp, token: &str) -> String {
    let response = request(
        &app.app,
        "POST",
        "/api/v1/doc/spaces",
        Some(token),
        Some(&json!({ "name": "知识库", "type": "team" })),
    )
    .await;
    response.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn page_tree_update_conflict_and_restore() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);
    let space_id = create_space(&app, &token).await;

    // 文件夹 + 页面
    let folder = request(
        &app.app,
        "POST",
        &format!("/api/v1/doc/spaces/{space_id}/nodes"),
        Some(&token),
        Some(&json!({ "kind": "folder", "title": "手册" })),
    )
    .await;
    let folder_id = folder.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let page = request(
        &app.app,
        "POST",
        &format!("/api/v1/doc/spaces/{space_id}/nodes"),
        Some(&token),
        Some(&json!({ "kind": "page", "title": "快速开始", "parentId": folder_id })),
    )
    .await;
    let page = page.expect(StatusCode::CREATED);
    let page_id = page["id"].as_str().unwrap().to_string();
    assert_eq!(page["version"], 1);

    // 树嵌套
    let tree = request(
        &app.app,
        "GET",
        &format!("/api/v1/doc/spaces/{space_id}/tree"),
        Some(&token),
        None,
    )
    .await;
    let tree = tree.expect(StatusCode::OK);
    assert_eq!(tree[0]["title"], "手册");
    assert_eq!(tree[0]["children"][0]["title"], "快速开始");

    // 更新正文 v1 → v2
    let updated = request(
        &app.app,
        "PUT",
        &format!("/api/v1/doc/spaces/{space_id}/nodes/{page_id}/content"),
        Some(&token),
        Some(&json!({ "contentMd": "# 第一版", "baseVersion": 1 })),
    )
    .await;
    assert_eq!(updated.expect(StatusCode::OK)["version"], 2);

    // 过期的 baseVersion → 409
    let conflict = request(
        &app.app,
        "PUT",
        &format!("/api/v1/doc/spaces/{space_id}/nodes/{page_id}/content"),
        Some(&token),
        Some(&json!({ "contentMd": "# 冲突", "baseVersion": 1 })),
    )
    .await;
    let conflict = conflict.expect(StatusCode::CONFLICT);
    assert_eq!(conflict["code"], "DOC_VERSION_CONFLICT");

    // 再更新到 v3
    request(
        &app.app,
        "PUT",
        &format!("/api/v1/doc/spaces/{space_id}/nodes/{page_id}/content"),
        Some(&token),
        Some(&json!({ "contentMd": "# 第二版", "baseVersion": 2 })),
    )
    .await
    .expect(StatusCode::OK);

    // 版本历史（v1、v2 快照）
    let versions = request(
        &app.app,
        "GET",
        &format!("/api/v1/doc/spaces/{space_id}/nodes/{page_id}/versions"),
        Some(&token),
        None,
    )
    .await;
    let versions = versions.expect(StatusCode::OK);
    assert_eq!(versions.as_array().unwrap().len(), 2);
    assert_eq!(versions[0]["version"], 2);

    // 回滚到 v1（内容为初始空串）→ 新版本 v4
    let restored = request(
        &app.app,
        "POST",
        &format!("/api/v1/doc/spaces/{space_id}/nodes/{page_id}/versions/1/restore"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(restored.expect(StatusCode::OK)["version"], 4);
    let detail = request(
        &app.app,
        "GET",
        &format!("/api/v1/doc/spaces/{space_id}/nodes/{page_id}"),
        Some(&token),
        None,
    )
    .await;
    let detail = detail.expect(StatusCode::OK);
    assert_eq!(detail["contentMd"], "");
    assert_eq!(detail["version"], 4);
}

#[tokio::test]
async fn permissions_and_validation() {
    let app = spawn().await;
    let owner = Uuid::now_v7();
    let outsider = Uuid::now_v7();
    let token_owner = issue_token(&app, owner);
    let token_outsider = issue_token(&app, outsider);
    let space_id = create_space(&app, &token_owner).await;

    // 非成员
    let denied = request(
        &app.app,
        "GET",
        &format!("/api/v1/doc/spaces/{space_id}/tree"),
        Some(&token_outsider),
        None,
    )
    .await;
    denied.expect(StatusCode::FORBIDDEN);

    // 非法类型
    let bad_kind = request(
        &app.app,
        "POST",
        &format!("/api/v1/doc/spaces/{space_id}/nodes"),
        Some(&token_owner),
        Some(&json!({ "kind": "sheet", "title": "x" })),
    )
    .await;
    bad_kind.expect(StatusCode::UNPROCESSABLE_ENTITY);

    // 空标题
    let bad_title = request(
        &app.app,
        "POST",
        &format!("/api/v1/doc/spaces/{space_id}/nodes"),
        Some(&token_owner),
        Some(&json!({ "kind": "page", "title": "  " })),
    )
    .await;
    bad_title.expect(StatusCode::UNPROCESSABLE_ENTITY);

    // 文件夹不能写正文
    let folder = request(
        &app.app,
        "POST",
        &format!("/api/v1/doc/spaces/{space_id}/nodes"),
        Some(&token_owner),
        Some(&json!({ "kind": "folder", "title": "资料" })),
    )
    .await;
    let folder_id = folder.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let edit_folder = request(
        &app.app,
        "PUT",
        &format!("/api/v1/doc/spaces/{space_id}/nodes/{folder_id}/content"),
        Some(&token_owner),
        Some(&json!({ "contentMd": "x", "baseVersion": 1 })),
    )
    .await;
    edit_folder.expect(StatusCode::UNPROCESSABLE_ENTITY);

    // 未登录
    request(&app.app, "GET", "/api/v1/doc/spaces", None, None)
        .await
        .expect(StatusCode::UNAUTHORIZED);

    // 健康检查
    let health = request(&app.app, "GET", "/healthz", None, None).await;
    assert_eq!(health.expect(StatusCode::OK)["status"], "ok");
    let ready = request(&app.app, "GET", "/readyz", None, None).await;
    assert_eq!(ready.expect(StatusCode::OK)["database"], "ok");
}

#[tokio::test]
async fn config_and_schema_validation_paths() {
    // schema 名非法
    let err = doc_service::db::connect_with_schema(&test_database_url(), "Bad-Name").await;
    assert!(err.is_err());
    // 缺少 DATABASE_URL
    assert!(doc_service::config::Config::from_map(Default::default()).is_err());
    // from_env 正常路径
    std::env::set_var("DATABASE_URL", test_database_url());
    let config = doc_service::config::Config::from_env().expect("env config");
    assert_eq!(config.bind_addr, "0.0.0.0:8084");
    std::env::remove_var("DATABASE_URL");
}

#[tokio::test]
async fn not_found_and_restore_error_paths() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);
    let space_id = create_space(&app, &token).await;
    let missing = Uuid::now_v7();

    // 不存在的父节点
    let bad_parent = request(
        &app.app,
        "POST",
        &format!("/api/v1/doc/spaces/{space_id}/nodes"),
        Some(&token),
        Some(&json!({ "kind": "page", "title": "x", "parentId": missing })),
    )
    .await;
    bad_parent.expect(StatusCode::NOT_FOUND);

    // 页面不存在
    let not_found = request(
        &app.app,
        "GET",
        &format!("/api/v1/doc/spaces/{space_id}/nodes/{missing}"),
        Some(&token),
        None,
    )
    .await;
    not_found.expect(StatusCode::NOT_FOUND);

    // 历史版本不存在
    let page = request(
        &app.app,
        "POST",
        &format!("/api/v1/doc/spaces/{space_id}/nodes"),
        Some(&token),
        Some(&json!({ "kind": "page", "title": "p" })),
    )
    .await;
    let page_id = page.expect(StatusCode::CREATED)["id"]
        .as_str()
        .unwrap()
        .to_string();
    let missing_version = request(
        &app.app,
        "POST",
        &format!("/api/v1/doc/spaces/{space_id}/nodes/{page_id}/versions/99/restore"),
        Some(&token),
        None,
    )
    .await;
    missing_version.expect(StatusCode::NOT_FOUND);

    // 非法空间类型与超长名称
    let bad_type = request(
        &app.app,
        "POST",
        "/api/v1/doc/spaces",
        Some(&token),
        Some(&json!({ "name": "x", "type": "wiki" })),
    )
    .await;
    bad_type.expect(StatusCode::UNPROCESSABLE_ENTITY);
    let long_name = "a".repeat(65);
    let bad_name = request(
        &app.app,
        "POST",
        "/api/v1/doc/spaces",
        Some(&token),
        Some(&json!({ "name": long_name })),
    )
    .await;
    bad_name.expect(StatusCode::UNPROCESSABLE_ENTITY);

    // 非法令牌
    let bad_token = request(&app.app, "GET", "/api/v1/doc/spaces", Some("invalid"), None).await;
    bad_token.expect(StatusCode::UNAUTHORIZED);
}

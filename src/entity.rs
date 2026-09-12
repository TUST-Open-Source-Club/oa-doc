//! doc schema 实体。

/// 文档空间。
pub mod space {
    use sea_orm::entity::prelude::*;

    /// 团队空间。
    pub const TYPE_TEAM: &str = "team";
    /// 个人空间。
    pub const TYPE_PERSONAL: &str = "personal";
    /// 公开空间。
    pub const TYPE_PUBLIC: &str = "public";

    /// 空间模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "spaces")]
    pub struct Model {
        /// ID。
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        /// 名称。
        pub name: String,
        /// 类型。
        pub r#type: String,
        /// 所有者。
        pub owner_id: Uuid,
        /// 创建时间。
        pub created_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// 空间成员。
pub mod space_member {
    use sea_orm::entity::prelude::*;

    /// 管理员。
    pub const ROLE_ADMIN: &str = "admin";
    /// 可编辑。
    pub const ROLE_EDITOR: &str = "editor";
    /// 只读。
    pub const ROLE_VIEWER: &str = "viewer";

    /// 成员模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "space_members")]
    pub struct Model {
        /// 空间。
        #[sea_orm(primary_key, auto_increment = false)]
        pub space_id: Uuid,
        /// 用户。
        #[sea_orm(primary_key, auto_increment = false)]
        pub user_id: Uuid,
        /// 角色。
        pub role: String,
        /// 加入时间。
        pub joined_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// 节点（文件夹/页面）。
pub mod node {
    use sea_orm::entity::prelude::*;

    /// 文件夹。
    pub const KIND_FOLDER: &str = "folder";
    /// 页面。
    pub const KIND_PAGE: &str = "page";

    /// 节点模型（页面正文与版本号内联，历史在 page_versions）。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "nodes")]
    pub struct Model {
        /// ID。
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        /// 所属空间。
        pub space_id: Uuid,
        /// 父节点。
        #[sea_orm(nullable)]
        pub parent_id: Option<Uuid>,
        /// folder / page。
        pub kind: String,
        /// 标题。
        pub title: String,
        /// 同级排序。
        pub position: i64,
        /// 当前版本号（从 1 开始）。
        pub version: i64,
        /// 当前正文（Markdown；文件夹为空）。
        #[sea_orm(column_type = "Text")]
        pub content_md: String,
        /// 创建者/最后编辑者。
        pub updated_by: Uuid,
        /// 软删除时间。
        #[sea_orm(nullable)]
        pub deleted_at: Option<DateTimeWithTimeZone>,
        /// 创建时间。
        pub created_at: DateTimeWithTimeZone,
        /// 更新时间。
        pub updated_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// 页面历史版本。
pub mod version {
    use sea_orm::entity::prelude::*;

    /// 版本模型。
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "page_versions")]
    pub struct Model {
        /// ID。
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        /// 页面节点。
        pub node_id: Uuid,
        /// 版本号。
        pub version: i64,
        /// 该版本正文。
        #[sea_orm(column_type = "Text")]
        pub content_md: String,
        /// 作者。
        pub author_id: Uuid,
        /// 时间。
        pub created_at: DateTimeWithTimeZone,
    }

    /// 关系。
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

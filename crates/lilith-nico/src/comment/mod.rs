use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: u64,
    pub thread_id: u64,
    pub no: u64,
    pub vpos: u64,
    pub vpos_ms: u64,
    pub body: String,
    pub mail: Vec<String>,
    pub posted_at: String,
    pub premium: bool,
    pub owner: bool,
    pub layer: i32,
    pub user_id: Option<String>,
    pub nicoru_count: u64,
    pub source: String,
    pub is_my_post: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentSource {
    pub threads: Vec<CommentThread>,
    pub nv_comment: Option<NvCommentContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentThread {
    pub id: u64,
    pub fork_label: String,
    pub layer: i32,
    pub is_active: bool,
    pub is_default_post_target: bool,
    pub is_owner_thread: bool,
    pub is_leaf_required: bool,
    pub server: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvCommentContext {
    pub server: String,
    pub thread_key: String,
    pub targets: Vec<NvCommentTarget>,
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvCommentTarget {
    pub id: u64,
    pub fork: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentApiResponse {
    pub global_comments: Vec<GlobalCommentCount>,
    pub threads: Vec<CommentApiThread>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalCommentCount {
    pub id: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentApiThread {
    pub id: u64,
    pub fork: String,
    pub comment_count: u64,
    pub comments: Vec<CommentApiItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentApiItem {
    pub id: u64,
    pub no: u64,
    pub vpos_ms: u64,
    pub body: String,
    pub commands: Vec<String>,
    pub user_id: Option<String>,
    pub is_premium: bool,
    pub posted_at: String,
    pub nicoru_count: u64,
    pub source: String,
    pub is_my_post: bool,
}

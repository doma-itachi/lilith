use reqwest::{Client, Url};
use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::{
    comment::{
        CommentApiItem, CommentApiResponse, CommentApiThread, CommentSource, CommentThread,
        GlobalCommentCount, NvCommentContext, NvCommentTarget,
    },
    video::VideoMetadata,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchMetadata {
    pub video: VideoMetadata,
    pub comment: CommentSource,
}

#[derive(Debug, Clone)]
pub struct NicoApiClient {
    http: Client,
}

impl Default for NicoApiClient {
    fn default() -> Self {
        Self {
            http: Client::builder()
                .user_agent(format!("lilith/{}", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("reqwest client should build"),
        }
    }
}

impl NicoApiClient {
    pub fn new(http: Client) -> Self {
        Self { http }
    }

    pub async fn fetch_watch_metadata(&self, watch_url: &str) -> Result<WatchMetadata, NicoApiError> {
        let mut request_url = Url::parse(watch_url)
            .map_err(|_| NicoApiError::InvalidWatchUrl(watch_url.to_string()))?;
        request_url.query_pairs_mut().append_pair("responseType", "json");

        let response = self.http.get(request_url).send().await?;
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(classify_watch_error(status, &body, watch_url));
        }

        Self::parse_watch_metadata(&body)
    }

    pub fn parse_watch_metadata(body: &str) -> Result<WatchMetadata, NicoApiError> {
        let envelope: WatchResponseEnvelope = serde_json::from_str(body)?;
        let layer_map = envelope.data.response.comment.layer_map();

        Ok(WatchMetadata {
            video: VideoMetadata {
                id: envelope.data.response.video.id,
                title: envelope.data.response.video.title,
                duration_seconds: envelope.data.response.video.duration,
                registered_at: envelope.data.response.video.registered_at,
            },
            comment: CommentSource {
                threads: envelope
                    .data
                    .response
                    .comment
                    .threads
                    .into_iter()
                    .map(|thread| {
                        let layer = layer_map
                            .iter()
                            .find(|candidate| {
                                candidate.thread_id == thread.id
                                    && candidate.fork_label == thread.fork_label
                            })
                            .map(|candidate| candidate.layer)
                            .unwrap_or(-1);

                        CommentThread {
                            id: thread.id,
                            fork_label: thread.fork_label,
                            layer,
                            is_active: thread.is_active,
                            is_default_post_target: thread.is_default_post_target,
                            is_owner_thread: thread.is_owner_thread,
                            is_leaf_required: thread.is_leaf_required,
                            server: empty_to_none(thread.server),
                        }
                    })
                    .collect(),
                nv_comment: envelope.data.response.comment.nv_comment.map(|nv_comment| {
                    NvCommentContext {
                        server: nv_comment.server,
                        thread_key: nv_comment.thread_key,
                        language: nv_comment.params.language,
                        targets: nv_comment
                            .params
                            .targets
                            .into_iter()
                            .map(|target| NvCommentTarget {
                                id: target.id.parse().unwrap_or_default(),
                                fork: target.fork,
                            })
                            .collect(),
                    }
                }),
            },
        })
    }

    pub async fn fetch_comments(
        &self,
        comment_source: &CommentSource,
    ) -> Result<CommentApiResponse, NicoApiError> {
        let nv_comment = comment_source
            .nv_comment
            .as_ref()
            .ok_or(NicoApiError::MissingNvCommentContext)?;

        let body = self
            .http
            .post(format!("{}/v1/threads", nv_comment.server.trim_end_matches('/')))
            .header("x-frontend-id", "6")
            .header("x-frontend-version", "0")
            .json(&CommentRequestBody {
                params: CommentRequestParams {
                    targets: nv_comment
                        .targets
                        .iter()
                        .map(|target| CommentRequestTarget {
                            id: target.id.to_string(),
                            fork: target.fork.clone(),
                        })
                        .collect(),
                    language: nv_comment.language.clone(),
                },
                thread_key: nv_comment.thread_key.clone(),
                additionals: CommentRequestAdditionals {},
            })
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        Self::parse_comment_response(&body)
    }

    pub fn parse_comment_response(body: &str) -> Result<CommentApiResponse, NicoApiError> {
        let envelope: CommentResponseEnvelope = serde_json::from_str(body)?;

        Ok(CommentApiResponse {
            global_comments: envelope
                .data
                .global_comments
                .into_iter()
                .map(|item| GlobalCommentCount {
                    id: item.id,
                    count: item.count,
                })
                .collect(),
            threads: envelope
                .data
                .threads
                .into_iter()
                .map(|thread| CommentApiThread {
                    id: thread.id,
                    fork: thread.fork,
                    comment_count: thread.comment_count,
                    comments: thread
                        .comments
                        .into_iter()
                        .map(|comment| CommentApiItem {
                            id: comment.id,
                            no: comment.no,
                            vpos_ms: comment.vpos_ms,
                            body: comment.body,
                            commands: comment.commands,
                            user_id: comment.user_id,
                            is_premium: comment.is_premium,
                            posted_at: comment.posted_at,
                            nicoru_count: comment.nicoru_count,
                            source: comment.source,
                            is_my_post: comment.is_my_post,
                        })
                        .collect(),
                })
                .collect(),
        })
    }
}

#[derive(Debug, Error)]
pub enum NicoApiError {
    #[error("invalid watch URL: {0}")]
    InvalidWatchUrl(String),

    #[error("request to NicoNico failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("this video appears to be sensitive or login-restricted; please sign in with NicoNico and try again: {watch_url}")]
    SensitiveVideo { watch_url: String },

    #[error("failed to fetch watch metadata: HTTP {status} {code}")]
    WatchRequestFailed { status: u16, code: String },

    #[error("failed to parse watch metadata response: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("watch metadata did not include nvcomment context")]
    MissingNvCommentContext,
}

fn empty_to_none(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn classify_watch_error(status: reqwest::StatusCode, body: &str, watch_url: &str) -> NicoApiError {
    if let Ok(error_body) = serde_json::from_str::<WatchErrorEnvelope>(body) {
        if status == reqwest::StatusCode::BAD_REQUEST && error_body.meta.code == "FORBIDDEN" {
            return NicoApiError::SensitiveVideo {
                watch_url: watch_url.to_string(),
            };
        }

        return NicoApiError::WatchRequestFailed {
            status: status.as_u16(),
            code: error_body.meta.code,
        };
    }

    NicoApiError::WatchRequestFailed {
        status: status.as_u16(),
        code: "UNKNOWN".to_string(),
    }
}

#[derive(Debug, Deserialize)]
struct WatchErrorEnvelope {
    meta: WatchErrorMeta,
}

#[derive(Debug, Deserialize)]
struct WatchErrorMeta {
    code: String,
}

#[derive(Debug, Deserialize)]
struct WatchResponseEnvelope {
    data: WatchResponseData,
}

#[derive(Debug, Deserialize)]
struct WatchResponseData {
    response: WatchResponse,
}

#[derive(Debug, Deserialize)]
struct WatchResponse {
    video: RawVideo,
    comment: RawComment,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawVideo {
    id: String,
    title: String,
    duration: u64,
    registered_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawComment {
    layers: Vec<RawCommentLayer>,
    threads: Vec<RawThread>,
    nv_comment: Option<RawNvComment>,
}

impl RawComment {
    fn layer_map(&self) -> Vec<LayerMapping> {
        self.layers
            .iter()
            .flat_map(|layer| {
                layer.thread_ids.iter().map(|thread| LayerMapping {
                    thread_id: thread.id,
                    fork_label: thread.fork_label.clone(),
                    layer: layer.index,
                })
            })
            .collect()
    }
}

#[derive(Debug)]
struct LayerMapping {
    thread_id: u64,
    fork_label: String,
    layer: i32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCommentLayer {
    index: i32,
    thread_ids: Vec<RawLayerThreadId>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLayerThreadId {
    id: u64,
    fork_label: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawThread {
    id: u64,
    fork_label: String,
    is_active: bool,
    is_default_post_target: bool,
    is_owner_thread: bool,
    is_leaf_required: bool,
    server: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawNvComment {
    server: String,
    thread_key: String,
    params: RawNvCommentParams,
}

#[derive(Debug, Deserialize)]
struct RawNvCommentParams {
    targets: Vec<RawNvCommentTarget>,
    language: String,
}

#[derive(Debug, Deserialize)]
struct RawNvCommentTarget {
    id: String,
    fork: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommentRequestBody {
    params: CommentRequestParams,
    thread_key: String,
    additionals: CommentRequestAdditionals,
}

#[derive(Debug, Serialize)]
struct CommentRequestParams {
    targets: Vec<CommentRequestTarget>,
    language: String,
}

#[derive(Debug, Serialize)]
struct CommentRequestTarget {
    id: String,
    fork: String,
}

#[derive(Debug, Default, Serialize)]
struct CommentRequestAdditionals {}

#[derive(Debug, Deserialize)]
struct CommentResponseEnvelope {
    data: CommentResponseData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommentResponseData {
    global_comments: Vec<RawGlobalCommentCount>,
    threads: Vec<RawCommentApiThread>,
}

#[derive(Debug, Deserialize)]
struct RawGlobalCommentCount {
    id: String,
    count: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCommentApiThread {
    #[serde(deserialize_with = "deserialize_u64_from_string_or_number")]
    id: u64,
    fork: String,
    comment_count: u64,
    comments: Vec<RawCommentApiItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCommentApiItem {
    #[serde(deserialize_with = "deserialize_u64_from_string_or_number")]
    id: u64,
    no: u64,
    vpos_ms: u64,
    body: String,
    #[serde(default)]
    commands: Vec<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    is_premium: bool,
    posted_at: String,
    #[serde(default)]
    nicoru_count: u64,
    source: String,
    #[serde(default)]
    is_my_post: bool,
}

fn deserialize_u64_from_string_or_number<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum U64Value {
        String(String),
        Number(u64),
    }

    match U64Value::deserialize(deserializer)? {
        U64Value::String(value) => value.parse().map_err(D::Error::custom),
        U64Value::Number(value) => Ok(value),
    }
}

#[cfg(test)]
mod tests {
    use super::NicoApiClient;

    const WATCH_RESPONSE_FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/testdata/nico_watch_sm9.json"
    ));

    #[test]
    fn parses_watch_metadata_fixture() {
        let metadata = NicoApiClient::parse_watch_metadata(WATCH_RESPONSE_FIXTURE).unwrap();

        assert_eq!(metadata.video.id, "sm9");
        assert_eq!(metadata.video.title, "Let's Go! Onmyouji");
        assert_eq!(metadata.video.duration_seconds, 320);
        assert_eq!(metadata.comment.threads.len(), 2);
        assert_eq!(metadata.comment.threads[0].fork_label, "owner");
        assert_eq!(metadata.comment.threads[0].layer, 0);
        assert_eq!(metadata.comment.threads[1].layer, 1);

        let nv_comment = metadata.comment.nv_comment.unwrap();
        assert_eq!(nv_comment.server, "https://public.nvcomment.nicovideo.jp");
        assert_eq!(nv_comment.thread_key, "thread-key");
        assert_eq!(nv_comment.targets.len(), 2);
        assert_eq!(nv_comment.targets[0].id, 1173108780);
    }

    #[test]
    fn parses_comment_response_fixture() {
        let response = NicoApiClient::parse_comment_response(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/testdata/nico_comments_sm9.json"
        )))
        .unwrap();

        assert_eq!(response.global_comments[0].count, 3);
        assert_eq!(response.threads.len(), 3);
        assert_eq!(response.threads[1].fork, "main");
        assert_eq!(response.threads[1].comments[0].body, "aaa");
    }

    #[test]
    fn classifies_sensitive_watch_error() {
        let error = super::classify_watch_error(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"meta":{"status":400,"code":"FORBIDDEN"}}"#,
            "https://www.nicovideo.jp/watch/sm44867689",
        );

        assert!(matches!(error, super::NicoApiError::SensitiveVideo { .. }));
    }
}

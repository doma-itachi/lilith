use crate::comment::{Comment, CommentApiResponse, CommentApiThread, CommentThread};

pub fn normalize(response: &CommentApiResponse, watch_threads: &[CommentThread]) -> Vec<Comment> {
    let mut comments = response
        .threads
        .iter()
        .flat_map(|thread| normalize_thread(thread, watch_threads))
        .collect::<Vec<_>>();

    comments.sort_by(|left, right| {
        left.vpos_ms
            .cmp(&right.vpos_ms)
            .then_with(|| left.posted_at.cmp(&right.posted_at))
            .then_with(|| left.id.cmp(&right.id))
    });

    comments
}

fn normalize_thread(thread: &CommentApiThread, watch_threads: &[CommentThread]) -> Vec<Comment> {
    let layer = watch_threads
        .iter()
        .find(|candidate| candidate.id == thread.id && candidate.fork_label == thread.fork)
        .map(|candidate| candidate.layer)
        .unwrap_or(-1);
    let owner = thread.fork == "owner";

    thread
        .comments
        .iter()
        .map(|comment| Comment {
            id: comment.id,
            thread_id: thread.id,
            no: comment.no,
            vpos: comment.vpos_ms / 10,
            vpos_ms: comment.vpos_ms,
            body: comment.body.clone(),
            mail: comment.commands.clone(),
            posted_at: comment.posted_at.clone(),
            premium: comment.is_premium,
            owner,
            layer,
            user_id: comment.user_id.clone(),
            nicoru_count: comment.nicoru_count,
            source: comment.source.clone(),
            is_my_post: comment.is_my_post,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{api::NicoApiClient, comment::CommentThread};

    use super::normalize;

    const WATCH_RESPONSE_FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/testdata/nico_watch_sm9.json"
    ));
    const COMMENT_RESPONSE_FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/testdata/nico_comments_sm9.json"
    ));

    #[test]
    fn normalizes_comment_response() {
        let metadata = NicoApiClient::parse_watch_metadata(WATCH_RESPONSE_FIXTURE).unwrap();
        let comments = NicoApiClient::parse_comment_response(COMMENT_RESPONSE_FIXTURE).unwrap();

        let normalized = normalize(&comments, &metadata.comment.threads);

        assert_eq!(normalized.len(), 3);
        assert_eq!(normalized[0].body, "aaa");
        assert_eq!(normalized[0].vpos, 10);
        assert!(!normalized[0].owner);
        assert_eq!(normalized[0].layer, 1);
        assert_eq!(normalized[1].body, "owner cmd");
        assert!(normalized[1].owner);
        assert_eq!(normalized[1].layer, 0);
        assert_eq!(normalized[2].mail, vec!["shita", "red"]);
    }

    #[test]
    fn falls_back_to_unknown_layer_when_thread_missing() {
        let comments = NicoApiClient::parse_comment_response(COMMENT_RESPONSE_FIXTURE).unwrap();
        let normalized = normalize(
            &comments,
            &[CommentThread {
                id: 1,
                fork_label: "missing".to_string(),
                layer: 9,
                is_active: true,
                is_default_post_target: false,
                is_owner_thread: false,
                is_leaf_required: false,
                server: None,
            }],
        );

        assert!(normalized.iter().all(|comment| comment.layer == -1));
    }
}

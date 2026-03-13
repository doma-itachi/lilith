use crate::job::Job;

pub fn build_job(url: &str) -> Result<Job, String> {
    let video_id = parse_video_id(url)?;

    Ok(Job {
        watch_url: url.to_string(),
        video_id,
    })
}

fn parse_video_id(url: &str) -> Result<String, String> {
    let marker = "/watch/";
    let Some(index) = url.find(marker) else {
        return Err("expected a NicoNico watch URL containing /watch/".to_string());
    };

    let video_id = &url[index + marker.len()..];
    let video_id = video_id.split('?').next().unwrap_or(video_id).trim();

    if video_id.is_empty() {
        return Err("video id was empty".to_string());
    }

    Ok(video_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_video_id;

    #[test]
    fn parses_watch_url() {
        let actual = parse_video_id("https://www.nicovideo.jp/watch/sm45174902").unwrap();
        assert_eq!(actual, "sm45174902");
    }
}

pub fn overlay_filter() -> &'static str {
    "[0:v][1:v]overlay=shortest=1[vout]"
}

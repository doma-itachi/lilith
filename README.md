# Lilith

Lilith is a Rust workspace for a fast NicoNico downloader that fetches a source video, collects comments, renders them, and produces a comment-overlaid mp4.

## Current status

- Phase 0 workspace scaffold is in place.
- Phase 1 CLI parsing and job bootstrap are implemented.
- Download, NicoNico API, renderer, and ffmpeg integration are still scaffold crates.

## Workspace

- `crates/lilith-cli`: CLI entry point and option parsing
- `crates/lilith-core`: job building, config, and shared domain models
- `crates/lilith-nico`: NicoNico metadata and comment acquisition
- `crates/lilith-render`: comment layout and rendering engine
- `crates/lilith-ffmpeg`: ffmpeg command and pipe integration
- `vendor/niconicomments`: TypeScript reference implementation kept for the Rust port

## Prerequisites

- Rust stable with Edition 2024 support
- `yt-dlp`
- `ffmpeg`

## Development

```bash
cargo check --workspace
cargo test --workspace
```

Run the current CLI scaffold:

```bash
cargo run -p lilith-cli --bin lilith -- https://www.nicovideo.jp/watch/sm45174902
```

You can also inspect the available options:

```bash
cargo run -p lilith-cli --bin lilith -- --help
```

## Notes

- Output defaults to `output/`.
- Temporary job files are planned under `output/.lilith/<video_id>/`.
- `vendor/niconicomments/.git/` is ignored so the vendored reference can stay in-tree without nested repository noise.

<img width="1383" height="412" alt="Image" src="https://github.com/user-attachments/assets/ec9b19e6-e451-4a20-9c01-e24f4f38dc78" />

# Lilith
LilithはRust製の高速コメ付きダウンローダです  
ニコニコ動画の動画をコメ付きでダウンロードできます

## 著者
doma-itachi @itachi_yukari

## ライセンス
Copyright (c) 2026 doma-itachi
このプロジェクトはMITライセンスの下で公開されています

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

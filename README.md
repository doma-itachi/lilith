![Logo](https://github.com/user-attachments/assets/db00f98b-30f8-4d18-a6d4-9a8577ab229d)

# Lilith
LilithはRust製の高速コメ付きダウンローダです  
ニコニコ動画の動画をコメ付きでダウンロードできます

## サンプル
[【マリオ64実況】　奴が来る　壱【幕末志士】(sm5457137)](https://www.nicovideo.jp/watch/sm5457137)
<video src="https://github.com/user-attachments/assets/025e9c08-ac0f-4892-bc98-5f9a3efa766e" controls="true"></video>

[利息回収前夜(sm38495149)](https://www.nicovideo.jp/watch/sm38495149)
<video src="https://github.com/user-attachments/assets/29642235-949b-463c-9f62-db72e376fc48" controls="true"></video>


## 貢献
貢献を歓迎します

- **バグ報告・機能要望**: リポジトリの Issues からお願いします
- **プルリクエスト**: 修正や機能追加は PR で送ってください。大きな変更の場合は先に Issue で相談するとスムーズです

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
- The renderer bundles `assets/fonts/NotoSansJP-VariableFont_wght.ttf` into the binary and uses it as the fallback default font.
- On macOS, Lilith prefers installed Hiragino families first and falls back to bundled `Noto Sans JP` when Hiragino is unavailable.
- `--font` still works as an override when you want to force a different local font.
- `vendor/niconicomments/.git/` is ignored so the vendored reference can stay in-tree without nested repository noise.

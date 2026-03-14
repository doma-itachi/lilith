use std::path::PathBuf;

use anyhow::Result;
use clap::{error::ErrorKind, Parser, ValueEnum};
use lilith_core::{build_job, AppConfig, HardwareAccel};

use crate::commands;

#[derive(Debug, Parser)]
#[command(
    name = "lilith",
    about = "Fast NicoNico comment video downloader",
    version
)]
struct Cli {
    #[arg(value_name = "URL")]
    url: String,

    #[arg(
        short = 'o',
        long = "output-dir",
        value_name = "DIR",
        default_value = "."
    )]
    output_dir: PathBuf,

    #[arg(long)]
    keep_temp: bool,

    #[arg(long, value_enum, default_value_t = HwaccelArg::Auto)]
    hwaccel: HwaccelArg,

    #[arg(long, value_name = "FONT")]
    font: Option<PathBuf>,

    #[arg(short, long)]
    quiet: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum HwaccelArg {
    Auto,
    None,
    Videotoolbox,
    Nvenc,
    Qsv,
    Amf,
}

impl From<HwaccelArg> for HardwareAccel {
    fn from(value: HwaccelArg) -> Self {
        match value {
            HwaccelArg::Auto => HardwareAccel::Auto,
            HwaccelArg::None => HardwareAccel::None,
            HwaccelArg::Videotoolbox => HardwareAccel::VideoToolbox,
            HwaccelArg::Nvenc => HardwareAccel::Nvenc,
            HwaccelArg::Qsv => HardwareAccel::Qsv,
            HwaccelArg::Amf => HardwareAccel::Amf,
        }
    }
}

pub async fn run(args: impl IntoIterator<Item = String>) -> Result<()> {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            error.print()?;
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };

    let job = build_job(&cli.url, config_from_cli(&cli))?;

    commands::download::run(job).await
}

fn config_from_cli(cli: &Cli) -> AppConfig {
    AppConfig {
        output_dir: cli.output_dir.clone(),
        keep_temp: cli.keep_temp,
        hwaccel: cli.hwaccel.into(),
        font: cli.font.clone(),
        quiet: cli.quiet,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::Cli;
    use clap::Parser;

    #[test]
    fn parses_expected_options() {
        let cli = Cli::parse_from([
            "lilith",
            "https://www.nicovideo.jp/watch/sm9",
            "--output-dir",
            "dist",
            "--keep-temp",
            "--hwaccel",
            "videotoolbox",
            "--font",
            "assets/fonts/default.ttf",
            "--quiet",
        ]);

        assert_eq!(cli.url, "https://www.nicovideo.jp/watch/sm9");
        assert_eq!(cli.output_dir, PathBuf::from("dist"));
        assert!(cli.keep_temp);
        assert!(cli.quiet);
        assert_eq!(cli.font, Some(PathBuf::from("assets/fonts/default.ttf")));
    }
}

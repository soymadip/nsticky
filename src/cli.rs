use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

/// nsticky CLI client
#[derive(Parser, Debug)]
#[command(name = "nsticky")]
#[command(version)]
#[command(about = "Manage sticky windows via CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Manage sticky windows
    Sticky {
        #[command(subcommand)]
        action: StickyAction,
    },
    /// Manage staged windows
    Stage {
        #[command(subcommand)]
        action: StageAction,
    },
}

#[derive(Subcommand, Debug)]
enum StickyAction {
    /// Add window to sticky list
    #[command(alias = "a")]
    Add {
        /// Window ID to add to sticky list
        window_id: u64,
    },
    /// Remove window from sticky list
    #[command(alias = "r")]
    Remove {
        /// Window ID to remove from sticky list
        window_id: u64,
    },
    /// List all sticky windows
    #[command(alias = "l")]
    List,
    /// Toggle active window in sticky list
    #[command(alias = "t")]
    ToggleActive,
    /// Toggle window by app ID in sticky list
    #[command(alias = "ta")]
    ToggleAppid {
        /// Application ID to toggle
        appid: String,
    },
    /// Toggle window by title in sticky list
    #[command(alias = "tt")]
    ToggleTitle {
        /// Window title to toggle
        title: String,
    },
}

#[derive(Subcommand, Debug)]
enum StageAction {
    /// List all staged windows
    #[command(alias = "l")]
    List,
    /// Add window to stage (move from sticky to stage workspace)
    #[command(alias = "a")]
    Add {
        /// Window ID to stage
        window_id: u64,
    },
    /// Remove window from stage (move from stage to current workspace)
    #[command(alias = "r")]
    Remove {
        /// Window ID to unstage
        window_id: u64,
    },
    /// Toggle active window in stage
    #[command(alias = "t")]
    ToggleActive,
    /// Toggle window by app ID in stage
    #[command(alias = "ta")]
    ToggleAppid {
        /// Application ID to toggle
        appid: String,
    },
    /// Toggle window by title in stage
    #[command(alias = "tt")]
    ToggleTitle {
        /// Window title to toggle
        title: String,
    },
    /// Add all sticky windows to stage
    #[command(alias = "aa")]
    AddAll,
    /// Remove all staged windows
    #[command(alias = "ra")]
    RemoveAll,
}

pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();

    let socket_path = "/tmp/niri_sticky_cli.sock";
    let stream = UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Generate command string based on subcommand
    let cmd_str = match cli.command {
        Commands::Sticky { action } => match action {
            StickyAction::Add { window_id } => format!("add {window_id}\n"),
            StickyAction::Remove { window_id } => format!("remove {window_id}\n"),
            StickyAction::List => "list\n".to_string(),
            StickyAction::ToggleActive => "toggle_active\n".to_string(),
            StickyAction::ToggleAppid { appid } => format!("toggle_appid {appid}\n"),
            StickyAction::ToggleTitle { title } => format!("toggle_title \"{title}\"\n"),
        },
        Commands::Stage { action } => match action {
            StageAction::List => "stage --list\n".to_string(),
            StageAction::Add { window_id } => format!("stage {window_id}\n"),
            StageAction::Remove { window_id } => format!("unstage {window_id}\n"),
            StageAction::ToggleActive => "stage --active\n".to_string(),
            StageAction::ToggleAppid { appid } => format!("stage --toggle-appid {appid}\n"),
            StageAction::ToggleTitle { title } => format!("stage --toggle-title \"{title}\"\n"),
            StageAction::AddAll => "stage --all\n".to_string(),
            StageAction::RemoveAll => "unstage --all\n".to_string(),
        },
    };

    writer.write_all(cmd_str.as_bytes()).await?;
    writer.flush().await?;

    let mut response = String::new();
    reader.read_line(&mut response).await?;
    print!("{response}");

    Ok(())
}

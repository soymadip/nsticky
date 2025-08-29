use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

/// nsticky CLI client
#[derive(Parser, Debug)]
#[command(name = "nsticky")]
#[command(about = "Manage sticky windows via CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Add {
        /// Window ID to add to sticky list
        window_id: u64,
    },
    Remove {
        /// Window ID to remove from sticky list
        window_id: u64,
    },
    List,
    ToggleActive,
    Stage(StageArgs),
    Unstage(UnstageArgs),
}

#[derive(clap::Args, Debug)]
pub struct StageArgs {
    #[arg(group = "target", required = true)]
    pub window_id: Option<u64>,
    #[arg(long, group = "target", required = true)]
    pub all: bool,
    #[arg(long, group = "target", required = true)]
    pub list: bool,
    #[arg(long, group = "target", required = true)]
    pub active: bool,
}

#[derive(clap::Args, Debug)]
pub struct UnstageArgs {
    #[arg(group = "target", required = true)]
    pub window_id: Option<u64>,
    #[arg(long, group = "target", required = true)]
    pub all: bool,
    #[arg(long, group = "target", required = true)]
    pub active: bool,
}

pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();

    let socket_path = "/tmp/niri_sticky_cli.sock";
    let stream = UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // 根据子命令构造命令字符串
    let cmd_str = match cli.command {
        Commands::Add { window_id } => format!("add {window_id}\n"),
        Commands::Remove { window_id } => format!("remove {window_id}\n"),
        Commands::List => "list\n".to_string(),
        Commands::ToggleActive => "toggle_active\n".to_string(),
        Commands::Stage(args) => {
            if args.all {
                "stage --all\n".to_string()
            } else if args.list {
                "stage --list\n".to_string()
            } else if args.active {
                "stage --active\n".to_string()
            } else {
                format!("stage {}\n", args.window_id.unwrap())
            }
        }
        Commands::Unstage(args) => {
            if args.all {
                "unstage --all\n".to_string()
            } else if args.active {
                "unstage --active\n".to_string()
            } else {
                format!("unstage {}\n", args.window_id.unwrap())
            }
        }
    };


    writer.write_all(cmd_str.as_bytes()).await?;
    writer.flush().await?;


    let mut response = String::new();
    reader.read_line(&mut response).await?;
    print!("{response}");

    Ok(())
}

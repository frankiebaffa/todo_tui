use clap::Parser;
#[derive(Parser, Clone)]
pub struct PathArgs {
    #[clap()]
    pub list_path: String,
}
#[derive(Parser, Clone)]
pub enum Mode {
    New(PathArgs),
    Open(PathArgs),
}
#[derive(Parser, Clone)]
pub struct Args {
    #[clap(short, long)]
    pub debug: bool,
    #[clap(short='s', long)]
    pub display_hidden: bool,
    #[clap(subcommand)]
    pub mode: Mode,
}

mod module;
mod script;
mod test;
mod binary;
mod client;

use clap::{Parser, Subcommand};
use crate::binary::{handle_binary_command, BinarySub};
use crate::client::{handle_client_command, ClientSub};
use crate::module::{handle_module_command, ModuleSub};
use crate::script::{handle_script_command, ScriptSub};
use crate::test::{handle_test_command, TestSub};

#[derive(Parser, Debug, Clone)]
#[clap(author="mon3stera", version="0.1.0", about="A manager of t(ee) t(ests) and their environment.")]
pub struct Args {
    #[clap(subcommand)]
    sub: Subcommands,
}

#[derive(Subcommand, Clone, Debug)]
enum Subcommands {
    /// management for kernel modules.
    Module {
        #[clap(subcommand)]
        sub: ModuleSub,
    },
    /// management for tests.
    Test {
        #[clap(subcommand)]
        sub: TestSub,
    },
    /// management for scripts.
    Script {
        #[clap(subcommand)]
        sub: ScriptSub,
    },
    /// management for binary files.
    Binary {
        #[clap(subcommand)]
        sub: BinarySub,
    },
    /// client for communication with the guest OS.
    Client {
        #[clap(subcommand)]
        sub: ClientSub,
        #[clap(default_value_t = 8088)]
        port: u16,
    },
}

async fn handle_command(sub: &Subcommands) -> anyhow::Result<()> {
    match sub {
        Subcommands::Module { sub } => handle_module_command(sub),
        Subcommands::Test { sub } => handle_test_command(sub).await,
        Subcommands::Script { sub } => handle_script_command(sub),
        Subcommands::Binary { sub } => handle_binary_command(sub),
        Subcommands::Client { sub, port } => handle_client_command(sub, *port).await,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    handle_command(&args.sub).await?;
    Ok(())
}

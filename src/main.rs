use jj_cli::{
    cli_util::{CliRunner, CommandHelper},
    command_error::CommandError,
    ui::Ui,
};
mod commands;

#[derive(clap::Parser, Clone, Debug)]
enum CustomCommand {
    /// Commands for working with ReDB repositories and remotes
    Re {
        #[command(subcommand)]
        command: ReCommand,
    },
}

#[derive(clap::Subcommand, Clone, Debug)]
enum ReCommand {
    /// Create a new repo backed by a clone of a ReDB repo
    Clone {
        source: String,
        #[arg(default_value = ".", value_hint = clap::ValueHint::DirPath)]
        destination: String,
    },
    /// Export to a ReDB remote
    #[command(group(
        clap::ArgGroup::new("mode")
            .required(true)
            .multiple(false)
            .args(["destination", "client", "server"])
    ))]
    Export {
        /// The destination (local path or ssh remote)
        #[arg(value_hint = clap::ValueHint::DirPath)]
        destination: Option<String>,
        /// Run as client (stdin/stdout protocol)
        #[arg(long)]
        client: bool,
        /// Run as server (stdin/stdout protocol)
        #[arg(long)]
        server: bool,
    },
    #[command(group(
        clap::ArgGroup::new("mode")
            .required(true)
            .multiple(false)
            .args(["source", "client", "server"])
    ))]
    /// Import from a ReDB remote
    Import {
        /// The source (local path or ssh remote)
        #[arg(value_hint = clap::ValueHint::DirPath)]
        source: Option<String>,
        /// Run as client (stdin/stdout protocol)
        #[arg(long)]
        client: bool,
        /// Run as server (stdin/stdout protocol)
        #[arg(long)]
        server: bool,
    },
    /// Create a new ReDB backed repo
    Init {
        #[arg(default_value = ".", value_hint = clap::ValueHint::DirPath)]
        destination: String,

        /// A bare repository with no working-copy
        #[arg(long)]
        bare: bool,
    },
}

async fn run_custom_command(
    ui: &mut Ui,
    ch: &CommandHelper,
    command: CustomCommand,
) -> Result<(), CommandError> {
    match command {
        CustomCommand::Re { command } => match command {
            ReCommand::Init { destination, bare } => {
                commands::init(ui, ch, &destination, bare).await
            }
            ReCommand::Export {
                destination,
                client,
                server,
            } => commands::export(ui, ch, destination.as_deref(), client, server).await,
            ReCommand::Import {
                source,
                client,
                server,
            } => commands::import(ui, ch, source.as_deref(), client, server).await,
            ReCommand::Clone {
                source,
                destination,
            } => commands::clone(ui, ch, &source, &destination).await,
        },
    }
}

fn main() -> std::process::ExitCode {
    CliRunner::init()
        .add_store_factories(jj::store_factories())
        .add_subcommand(run_custom_command)
        .run()
        .into()
}

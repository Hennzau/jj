use jj_cli::{
    cli_util::{CliRunner, CommandHelper},
    command_error::CommandError,
    ui::Ui,
};

mod commands;

#[derive(clap::Parser, Clone, Debug)]
enum CustomCommand {
    /// Initialize a new repository.
    Init {
        /// The destination directory for the new repository.
        #[arg(default_value = ".", value_hint = clap::ValueHint::DirPath)]
        destination: String,

        /// Whether to create a bare repository (one with no workspace).
        #[arg(short, long)]
        bare: bool,
    },
    /// Clone a repository.
    Clone {
        /// The source repository.
        source: String,

        /// The destination directory for the new repository.
        #[arg(default_value = ".", value_hint = clap::ValueHint::DirPath)]
        destination: String,
    },
}

fn run_custom_command(
    ui: &mut Ui,
    ch: &CommandHelper,
    command: CustomCommand,
) -> Result<(), CommandError> {
    match command {
        CustomCommand::Init { destination, bare } => commands::init(ui, ch, destination, bare),
        CustomCommand::Clone {
            source,
            destination,
        } => commands::clone(ui, ch, source, destination),
    }
}

fn main() -> std::process::ExitCode {
    CliRunner::init()
        .add_store_factories(jj::store_factories())
        .add_subcommand(run_custom_command)
        .run()
        .into()
}

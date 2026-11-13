use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use liotek::backup::{self, BackupName, RestoreSource, Trigger};
use liotek::config::Config;
use liotek::db::Db;
use liotek::domain::uploads;
use liotek::http::{self, AppState};
use liotek::{blob, build, doctor, export, health, import};

#[derive(Parser)]
#[command(
    name = "liotek",
    about = "A personal library daemon",
    disable_version_flag = true
)]
struct Cli {
    /// Print the version, Git revision, and supported schema version
    #[arg(short = 'V', long)]
    version: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the daemon
    Serve(ConfigArg),
    /// Report what `serve` would use and whether it is safe to start; changes nothing
    Doctor {
        #[command(flatten)]
        config: ConfigArg,
        /// Also ask the object store whether every referenced object exists
        #[arg(long)]
        check_objects: bool,
    },
    /// Check that the running daemon is up and is this exact build
    Health(ConfigArg),
    /// Write the library as one consistent, portable SQLite file
    Export {
        #[command(flatten)]
        config: ConfigArg,
        /// Where to write the export; must not exist yet
        #[arg(long)]
        output: PathBuf,
    },
    /// Back the library up to the object store now
    Backup(ConfigArg),
    /// Inspect the backups in the object store
    #[command(subcommand)]
    Backups(BackupsCommand),
    /// Restore the library from a backup or an export; never replaces an existing database
    Restore {
        #[command(flatten)]
        config: ConfigArg,
        /// A backup name from `liotek backups list`, or the path of a local export
        #[arg(long)]
        from: String,
    },
    /// Manage upload sessions
    #[command(subcommand)]
    Uploads(UploadsCommand),
    /// Import a consistent snapshot of a Bibliotek database
    ImportV1 {
        #[command(flatten)]
        config: ConfigArg,
        /// The Bibliotek snapshot to read; it is opened read-only
        #[arg(long)]
        from: PathBuf,
        /// Report what would be imported without keeping any of it
        #[arg(long)]
        dry_run: bool,
        /// Where to write the verification report
        #[arg(long, default_value = "migration-report.json")]
        report: PathBuf,
    },
}

#[derive(Subcommand)]
enum BackupsCommand {
    /// List every backup, newest first
    List(ConfigArg),
}

#[derive(Subcommand)]
enum UploadsCommand {
    /// Expire abandoned upload sessions and abort them in the object store
    Gc {
        #[command(flatten)]
        config: ConfigArg,
        /// List what would be expired without changing anything
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Args)]
struct ConfigArg {
    /// Path to the config file [default: ~/.liotek/config.yaml]
    #[arg(short, long)]
    config: Option<PathBuf>,
}

impl ConfigArg {
    fn load(&self) -> anyhow::Result<Config> {
        let path = match &self.config {
            Some(path) => path.clone(),
            None => Config::default_path().context("HOME is not set; pass --config")?,
        };
        Ok(Config::load(&path)?)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    match run(cli).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("liotek: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    if cli.version {
        println!("{}", build::identity());
        return Ok(ExitCode::SUCCESS);
    }
    let Some(command) = cli.command else {
        bail!("no command given; try `liotek --help`");
    };

    match command {
        Command::Serve(config) => serve(config.load()?).await?,
        Command::Doctor {
            config,
            check_objects,
        } => {
            let report = doctor::examine(&config.load()?, check_objects).await;
            print!("{report}");
            if !report.is_healthy() {
                return Ok(ExitCode::FAILURE);
            }
        }
        Command::Health(config) => println!("{}", health::check(&config.load()?).await?),
        Command::Export { config, output } => {
            let db = open_database(&config.load()?).await?;
            let summary = export::export(&db, &output).await?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Command::Backup(config) => {
            let config = config.load()?;
            let db = open_database(&config).await?;
            let blobs = blob::open(&config.blob_store).await?;
            let name = BackupName::now(Trigger::Manual);
            let made = backup::create(&db, blobs.as_ref(), &config.temp_dir(), name).await?;
            println!("{}", serde_json::to_string_pretty(&made)?);
        }
        Command::Backups(BackupsCommand::List(config)) => {
            let blobs = blob::open(&config.load()?.blob_store).await?;
            let backups = backup::list(blobs.as_ref()).await?;
            println!("{}", serde_json::to_string_pretty(&backups)?);
        }
        Command::Restore { config, from } => {
            let config = config.load()?;
            let blobs = blob::open(&config.blob_store).await?;
            let restored = backup::restore(&config, blobs.as_ref(), &RestoreSource::parse(&from)).await?;
            println!("{}", serde_json::to_string_pretty(&restored)?);
        }
        Command::Uploads(UploadsCommand::Gc { config, dry_run }) => {
            let config = config.load()?;
            let db = open_database(&config).await?;
            let blobs = blob::open(&config.blob_store).await?;
            let report = uploads::collect_garbage(&db, blobs.as_ref(), dry_run).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::ImportV1 {
            config,
            from,
            dry_run,
            report,
        } => {
            let db = open_database(&config.load()?).await?;
            let migration = import::v1::run(&db, &from, dry_run).await?;
            std::fs::write(&report, serde_json::to_string_pretty(&migration)?)
                .with_context(|| format!("cannot write {}", report.display()))?;
            println!("wrote {}", report.display());
            if !migration.succeeded {
                eprintln!("liotek: the import has unexplained losses and was not kept; see the report");
                return Ok(ExitCode::FAILURE);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Opens the configured database and brings its schema up to this build's version.
async fn open_database(config: &Config) -> anyhow::Result<Db> {
    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("cannot create data directory {}", config.data_dir.display()))?;
    let db = Db::open(&config.database).await?;
    for version in db.migrate().await? {
        tracing::info!(version, "applied schema migration");
    }
    Ok(db)
}

async fn serve(config: Config) -> anyhow::Result<()> {
    let db = Arc::new(open_database(&config).await?);
    let blobs = blob::open(&config.blob_store).await?;
    backup::sweep_temp_dir(&config.temp_dir());

    tracing::info!(
        build = %build::identity(),
        environment = %config.environment,
        config = %config.config_path.display(),
        database = %config.database.path.display(),
        object_store = %blobs.describe(),
        backups = %config.backups.map_or("on request only".to_string(), |schedule| schedule.to_string()),
        "starting"
    );

    let listener = TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("cannot listen on {}", config.bind))?;
    tracing::info!(address = %config.bind, "listening");

    let stop = CancellationToken::new();
    tokio::spawn({
        let stop = stop.clone();
        async move {
            shutdown_signal().await;
            stop.cancel();
        }
    });
    let scheduled_backups = config.backups.map(|schedule| {
        tokio::spawn(backup::run_schedule(
            db.clone(),
            blobs.clone(),
            config.temp_dir(),
            schedule,
            stop.clone().cancelled_owned(),
        ))
    });

    let state = AppState {
        config: Arc::new(config),
        db,
        blobs,
    };
    let served = axum::serve(listener, http::router(state))
        .with_graceful_shutdown(stop.clone().cancelled_owned())
        .await;
    stop.cancel();
    if let Some(task) = scheduled_backups
        && let Err(error) = task.await
    {
        tracing::error!(%error, "the backup schedule ended abnormally");
    }
    served?;
    tracing::info!("stopped");
    Ok(())
}

/// Resolves on Ctrl-C or SIGTERM, which is how launchd asks the daemon to stop.
async fn shutdown_signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install the SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}

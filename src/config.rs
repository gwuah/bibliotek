use std::fmt;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use serde::de::IgnoredAny;

const DEFAULT_DATABASE_FILE: &str = "liotek.db";
const DEFAULT_BACKUP_EVERY_HOURS: u64 = 24;
const DEFAULT_BACKUPS_KEPT: usize = 30;

mod bibliotek {
    pub const PORT: u16 = 5678;
    pub const DATA_DIR: &str = ".bibliotek";
    pub const NAME: &str = "bibliotek";
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read config file {path}: {source}")]
    Unreadable { path: PathBuf, source: std::io::Error },
    #[error("config file {path} is not valid: {source}")]
    Malformed {
        path: PathBuf,
        source: serde_yaml::Error,
    },
    #[error("{field}: {problem}")]
    Invalid { field: &'static str, problem: String },
    #[error("refusing to start: {0}")]
    Collision(String),
}

fn invalid(field: &'static str, problem: impl Into<String>) -> ConfigError {
    ConfigError::Invalid {
        field,
        problem: problem.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Development,
    Preview,
    Production,
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Environment::Development => "development",
            Environment::Preview => "preview",
            Environment::Production => "production",
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub environment: Environment,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub bind: SocketAddr,
    pub admin_token: Secret,
    pub extension_origins: Vec<String>,
    pub database: DatabaseConfig,
    pub blob_store: BlobStoreConfig,
    pub research_database: Option<PathBuf>,
    pub reader_books_dir: Option<PathBuf>,
    pub backups: Option<BackupSchedule>,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupSchedule {
    pub every: Duration,
    pub keep: NonZeroUsize,
}

impl fmt::Display for BackupSchedule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "every {}h, keeping the newest {} scheduled",
            self.every.as_secs() / 3600,
            self.keep
        )
    }
}

#[derive(Debug, Clone)]
pub enum BlobStoreConfig {
    Filesystem { root: PathBuf },
    S3(S3Config),
}

#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub namespace: String,
    pub access_key_id: Secret,
    pub secret_access_key: Secret,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        let config_path = std::path::absolute(path).map_err(|source| ConfigError::Unreadable {
            path: path.to_path_buf(),
            source,
        })?;
        let yaml = std::fs::read_to_string(&config_path).map_err(|source| ConfigError::Unreadable {
            path: config_path.clone(),
            source,
        })?;
        let home = std::env::var_os("HOME").map(PathBuf::from);
        Config::from_yaml(&yaml, &config_path, home.as_deref())
    }

    pub fn from_yaml(yaml: &str, config_path: &Path, home: Option<&Path>) -> Result<Config, ConfigError> {
        let raw: RawConfig = serde_yaml::from_str(yaml).map_err(|source| ConfigError::Malformed {
            path: config_path.to_path_buf(),
            source,
        })?;
        let config = raw.resolve(config_path)?;
        config.check_isolation(home)?;
        Ok(config)
    }

    pub fn temp_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
    }

    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".liotek").join("config.yaml"))
    }

    fn check_isolation(&self, home: Option<&Path>) -> Result<(), ConfigError> {
        let collision = |message: String| Err(ConfigError::Collision(message));

        if self.bind.port() == bibliotek::PORT && self.environment != Environment::Production {
            return collision(format!(
                "port {} belongs to Bibliotek; only a production cutover may bind it",
                bibliotek::PORT
            ));
        }

        let mut local_paths = vec![&self.data_dir, &self.database.path];
        if let BlobStoreConfig::Filesystem { root } = &self.blob_store {
            local_paths.push(root);
        }
        for path in local_paths {
            if let Some(home) = home
                && path.starts_with(home.join(bibliotek::DATA_DIR))
            {
                return collision(format!("{} is inside Bibliotek's data directory", path.display()));
            }
            if file_name_mentions_bibliotek(path) {
                return collision(format!("{} is named after Bibliotek", path.display()));
            }
        }

        if let BlobStoreConfig::S3(s3) = &self.blob_store {
            let required = format!("liotek/{}/", self.environment);
            if s3.bucket.to_lowercase().contains(bibliotek::NAME) && s3.namespace != required {
                return collision(format!(
                    "bucket {} is shared with Bibliotek, so blob_store.namespace must be exactly {required}",
                    s3.bucket
                ));
            }
        }

        if self.environment == Environment::Development && matches!(self.blob_store, BlobStoreConfig::S3(_)) {
            return collision("development must use the filesystem blob store".to_string());
        }

        Ok(())
    }
}

fn file_name_mentions_bibliotek(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_lowercase().contains(bibliotek::NAME))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    environment: Environment,
    data_dir: PathBuf,
    bind: String,
    admin_token: RawSecret,
    #[serde(default)]
    extension_origins: Vec<String>,
    #[serde(default)]
    database: RawDatabase,
    blob_store: RawBlobStore,
    research: Option<RawResearch>,
    reader: Option<RawReader>,
    backups: Option<RawBackups>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawDatabase {
    path: Option<PathBuf>,
    /// Accepted only to be refused with an explanation: replication was removed, and an old
    /// config should say why it no longer works, not just "unknown field".
    replication: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBackups {
    every_hours: Option<u64>,
    keep: Option<usize>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum RawBlobStore {
    Filesystem {
        root: Option<PathBuf>,
    },
    S3 {
        endpoint: String,
        region: String,
        bucket: String,
        #[serde(default)]
        namespace: String,
        access_key_id: RawSecret,
        secret_access_key: RawSecret,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawResearch {
    database_path: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawReader {
    books_dir: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSecret {
    env: Option<String>,
    value: Option<String>,
}

impl RawSecret {
    fn resolve(self, field: &'static str) -> Result<Secret, ConfigError> {
        let value = match (self.env, self.value) {
            (Some(variable), None) => std::env::var(&variable)
                .map_err(|_| invalid(field, format!("environment variable {variable} is not set")))?,
            (None, Some(value)) => value,
            _ => return Err(invalid(field, "set exactly one of `env` or `value`")),
        };
        if value.trim().is_empty() {
            return Err(invalid(field, "must not be empty"));
        }
        Ok(Secret(value))
    }
}

impl RawConfig {
    fn resolve(self, config_path: &Path) -> Result<Config, ConfigError> {
        let base = config_path.parent().unwrap_or(Path::new("/"));
        let data_dir = resolve_path(base, &self.data_dir);

        let bind: SocketAddr = self
            .bind
            .parse()
            .map_err(|_| invalid("bind", "expected an address such as 127.0.0.1:6579"))?;

        if self.database.replication.is_some() {
            return Err(invalid(
                "database.replication",
                "replication was removed: the database is a local SQLite file, copied to the object \
                 store by `backups` instead. Delete `database.replication` and see `backups` in \
                 config.example.yaml",
            ));
        }
        let database = DatabaseConfig {
            path: resolve_path(
                &data_dir,
                &self.database.path.unwrap_or(DEFAULT_DATABASE_FILE.into()),
            ),
        };

        let blob_store = match self.blob_store {
            RawBlobStore::Filesystem { root } => BlobStoreConfig::Filesystem {
                root: resolve_path(&data_dir, &root.unwrap_or("blobs".into())),
            },
            RawBlobStore::S3 {
                endpoint,
                region,
                bucket,
                namespace,
                access_key_id,
                secret_access_key,
            } => {
                if !namespace.is_empty() && !namespace.ends_with('/') {
                    return Err(invalid("blob_store.namespace", "must end with `/`"));
                }
                BlobStoreConfig::S3(S3Config {
                    endpoint,
                    region,
                    bucket,
                    namespace,
                    access_key_id: access_key_id.resolve("blob_store.access_key_id")?,
                    secret_access_key: secret_access_key.resolve("blob_store.secret_access_key")?,
                })
            }
        };

        Ok(Config {
            environment: self.environment,
            config_path: config_path.to_path_buf(),
            bind,
            admin_token: self.admin_token.resolve("admin_token")?,
            extension_origins: self.extension_origins,
            database,
            blob_store,
            research_database: self
                .research
                .map(|research| resolve_path(base, &research.database_path)),
            reader_books_dir: self.reader.map(|reader| resolve_path(base, &reader.books_dir)),
            backups: self.backups.map(RawBackups::resolve).transpose()?,
            data_dir,
        })
    }
}

impl RawBackups {
    fn resolve(self) -> Result<BackupSchedule, ConfigError> {
        let every_hours = self.every_hours.unwrap_or(DEFAULT_BACKUP_EVERY_HOURS);
        if every_hours == 0 {
            return Err(invalid("backups.every_hours", "must be at least 1"));
        }
        let keep = NonZeroUsize::new(self.keep.unwrap_or(DEFAULT_BACKUPS_KEPT))
            .ok_or_else(|| invalid("backups.keep", "must be at least 1"))?;
        Ok(BackupSchedule {
            every: Duration::from_secs(every_hours * 60 * 60),
            keep,
        })
    }
}

fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    let mut resolved = PathBuf::new();
    for component in base.join(path).components() {
        match component {
            Component::ParentDir => {
                resolved.pop();
            }
            Component::CurDir => {}
            other => resolved.push(other),
        }
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/reader";

    fn load(yaml: &str) -> Result<Config, ConfigError> {
        Config::from_yaml(yaml, Path::new("/srv/liotek/config.yaml"), Some(Path::new(HOME)))
    }

    fn development(overrides: &str) -> String {
        format!(
            "environment: development\n\
             data_dir: data\n\
             bind: 127.0.0.1:6579\n\
             admin_token: {{ value: secret }}\n\
             blob_store: {{ kind: filesystem }}\n\
             {overrides}"
        )
    }

    fn preview_with_s3(bucket: &str, namespace: &str) -> String {
        format!(
            "environment: preview\n\
             data_dir: data\n\
             bind: 0.0.0.0:6578\n\
             admin_token: {{ value: secret }}\n\
             blob_store:\n\
             \x20 kind: s3\n\
             \x20 endpoint: https://t3.storage.dev\n\
             \x20 region: auto\n\
             \x20 bucket: {bucket}\n\
             \x20 namespace: \"{namespace}\"\n\
             \x20 access_key_id: {{ value: id }}\n\
             \x20 secret_access_key: {{ value: key }}\n"
        )
    }

    #[test]
    fn resolves_every_path_against_the_config_file() {
        let config = load(&development(
            "research: { database_path: ../research/app.db }\nreader: { books_dir: ../iconoclast/books }",
        ))
        .unwrap();

        assert_eq!(config.data_dir, Path::new("/srv/liotek/data"));
        assert_eq!(config.database.path, Path::new("/srv/liotek/data/liotek.db"));
        assert_eq!(
            config.research_database.unwrap(),
            Path::new("/srv/research/app.db")
        );
        assert_eq!(
            config.reader_books_dir.unwrap(),
            Path::new("/srv/iconoclast/books")
        );
        match config.blob_store {
            BlobStoreConfig::Filesystem { root } => assert_eq!(root, Path::new("/srv/liotek/data/blobs")),
            BlobStoreConfig::S3(_) => panic!("expected the filesystem store"),
        }
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(matches!(
            load(&development("databse: {}")),
            Err(ConfigError::Malformed { .. })
        ));
    }

    #[test]
    fn secrets_are_redacted_in_debug_output() {
        let config = load(&development("")).unwrap();
        assert!(!format!("{config:?}").contains("secret"));
    }

    #[test]
    fn rejects_bibliotek_port_outside_production() {
        let yaml = development("").replace("6579", "5678");
        assert!(matches!(load(&yaml), Err(ConfigError::Collision(_))));
    }

    #[test]
    fn rejects_paths_inside_bibliotek_data_directory() {
        let yaml = development("").replace("data_dir: data", "data_dir: /Users/reader/.bibliotek");
        assert!(matches!(load(&yaml), Err(ConfigError::Collision(_))));

        let escaped = development("database: { path: ../../../Users/reader/.bibliotek/x.db }");
        assert!(matches!(load(&escaped), Err(ConfigError::Collision(_))));
    }

    #[test]
    fn rejects_a_database_named_after_bibliotek() {
        let yaml = development("database: { path: bibliotek.db }");
        assert!(matches!(load(&yaml), Err(ConfigError::Collision(_))));
    }

    #[test]
    fn development_cannot_reach_cloud_services() {
        let s3 = preview_with_s3("liotek", "")
            .replace("preview", "development")
            .replace("6578", "6579");
        assert!(matches!(load(&s3), Err(ConfigError::Collision(_))));
    }

    #[test]
    fn shared_bucket_requires_the_environment_namespace() {
        assert!(matches!(
            load(&preview_with_s3("bibliotek", "")),
            Err(ConfigError::Collision(_))
        ));
        assert!(matches!(
            load(&preview_with_s3("bibliotek", "liotek/production/")),
            Err(ConfigError::Collision(_))
        ));
        assert!(load(&preview_with_s3("bibliotek", "liotek/preview/")).is_ok());
        assert!(load(&preview_with_s3("liotek-preview", "")).is_ok());
    }

    #[test]
    fn a_replication_setting_explains_that_backups_replaced_it() {
        let yaml = development(
            "database: { replication: { url: libsql://liotek-dev.turso.io, auth_token: { value: t } } }",
        );
        let error = load(&yaml).unwrap_err();

        assert!(
            matches!(
                error,
                ConfigError::Invalid {
                    field: "database.replication",
                    ..
                }
            ),
            "{error}"
        );
        assert!(error.to_string().contains("replication was removed"), "{error}");
        assert!(error.to_string().contains("backups"), "{error}");
    }

    #[test]
    fn backups_are_off_unless_scheduled() {
        assert_eq!(load(&development("")).unwrap().backups, None);

        let defaults = load(&development("backups: {}")).unwrap().backups.unwrap();
        assert_eq!(defaults.every, Duration::from_secs(24 * 60 * 60));
        assert_eq!(defaults.keep.get(), 30);

        let chosen = load(&development("backups: { every_hours: 6, keep: 4 }"))
            .unwrap()
            .backups
            .unwrap();
        assert_eq!(chosen.every, Duration::from_secs(6 * 60 * 60));
        assert_eq!(chosen.keep.get(), 4);
    }

    #[test]
    fn rejects_a_schedule_that_never_runs_or_keeps_nothing() {
        for yaml in ["backups: { every_hours: 0 }", "backups: { keep: 0 }"] {
            assert!(
                matches!(load(&development(yaml)), Err(ConfigError::Invalid { .. })),
                "{yaml}"
            );
        }
    }
}

use crate::error::BoxDynError;
use crate::migrate::{Migration, MigrationType};
use futures_core::future::BoxFuture;
#[cfg(not(any(feature = "_rt-actix", feature = "_rt-tokio")))]
use futures_util::TryStreamExt;
use sqlx_rt::fs;
use std::borrow::Cow;
use std::fmt::Debug;
use std::path::{Path, PathBuf};

pub trait MigrationSource<'s>: Debug {
    fn resolve(self) -> BoxFuture<'s, Result<Vec<Migration>, BoxDynError>>;
}

#[cfg(any(feature = "_rt-actix", feature = "_rt-tokio"))]
async fn get_next_dir(s: &mut fs::ReadDir) -> std::io::Result<Option<fs::DirEntry>> {
    s.next_entry().await
}

#[cfg(not(any(feature = "_rt-actix", feature = "_rt-tokio")))]
async fn get_next_dir(s: &mut fs::ReadDir) -> std::io::Result<Option<fs::DirEntry>> {
    s.try_next().await
}

impl<'s> MigrationSource<'s> for &'s Path {
    fn resolve(self) -> BoxFuture<'s, Result<Vec<Migration>, BoxDynError>> {
        Box::pin(async move {
            let mut s = fs::read_dir(self.canonicalize()?).await?;
            let mut migrations = Vec::new();

            while let Some(entry) = get_next_dir(&mut s).await? {
                if !entry.metadata().await?.is_file() {
                    // not a file; ignore
                    continue;
                }

                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();

                let parts = file_name.splitn(2, '_').collect::<Vec<_>>();

                if parts.len() != 2 || !parts[1].ends_with(".sql") {
                    // not of the format: <VERSION>_<DESCRIPTION>.sql; ignore
                    continue;
                }

                let version: i64 = parts[0].parse()?;

                let migration_type = MigrationType::from_filename(parts[1]);
                // remove the `.sql` and replace `_` with ` `
                let description = parts[1]
                    .trim_end_matches(migration_type.suffix())
                    .replace('_', " ")
                    .to_owned();

                let sql = fs::read_to_string(&entry.path()).await?;

                migrations.push(Migration::new(
                    version,
                    Cow::Owned(description),
                    migration_type,
                    Cow::Owned(sql),
                ));
            }

            // ensure that we are sorted by `VERSION ASC`
            migrations.sort_by_key(|m| m.version);

            Ok(migrations)
        })
    }
}

impl MigrationSource<'static> for PathBuf {
    fn resolve(self) -> BoxFuture<'static, Result<Vec<Migration>, BoxDynError>> {
        Box::pin(async move { self.as_path().resolve().await })
    }
}

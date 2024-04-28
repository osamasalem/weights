use core::panic;

use async_std::fs::read_dir;
use async_std::path::{Path, PathBuf};
use async_std::task::spawn;
use futures::future::{join_all, BoxFuture};
use futures::{FutureExt, StreamExt};

enum FSType {
    Folder(Vec<FSEntity>),
    File,
}

impl FSType {
    fn list(&self) -> &Vec<FSEntity> {
        match self {
            FSType::Folder(ref list) => list,
            _ => panic!("Invalid FSType"),
        }
    }

    fn list_mut(&mut self) -> &mut Vec<FSEntity> {
        match self {
            FSType::Folder(ref mut list) => list,
            _ => panic!("Invalid FSType"),
        }
    }
}

struct FSEntity {
    path: PathBuf,
    size: u64,
    kind: FSType,
}

fn format_size(size: u64) -> String {
    if size / (1024 * 1024) != 0 {
        format!("{:.2} MB", size as f64 / (1024 * 1024) as f64)
    } else if size / 1024 != 0 {
        format!("{:.2} KB", size as f64 / (1024) as f64)
    } else {
        format!("{:.2} Bytes", size)
    }
}

fn format_path(path: &Path) -> String {
    let out = path.to_str().unwrap_or("<UNKNOWN>");
    let len = out.len();
    if len > 50 {
        format!("{}...{}", &out[0..20], &out[len - 20..])
    } else {
        out.to_owned()
    }
}

impl FSEntity {
    async fn file(name: impl Into<PathBuf>, size: u64) -> Self {
        FSEntity {
            path: name.into(),
            size,
            kind: FSType::File,
        }
    }

    async fn folder(name: impl Into<PathBuf>) -> Self {
        let mut entity = FSEntity {
            path: name.into(),
            size: 0,
            kind: FSType::Folder(vec![]),
        };
        entity.size = entity.calculate_size().await;
        entity
    }

    fn calculate_size(&mut self) -> BoxFuture<u64> {
        async move {
            let mut tasks = vec![];

            let Ok(mut dir) = read_dir(self.path.to_string_lossy().into_owned()).await else {
                return 0;
            };

            let list = self.kind.list_mut();

            while let Some(entry) = dir.next().await {
                let Ok(entry) = entry else {
                    eprintln!("ERROR: Getting next entry");
                    continue;
                };
                let path = entry.path();
                let Ok(file_type) = entry.file_type().await else {
                    eprintln!("ERROR: Getting file type");
                    continue;
                };

                let Ok(metadata) = entry.metadata().await else {
                    eprintln!("ERROR: Getting entry metadata");
                    continue;
                };

                if file_type.is_file() {
                    list.push(FSEntity::file(path, metadata.len()).await)
                } else {
                    tasks.push(spawn(async { FSEntity::folder(path).await }));
                }
            }
            let mut results = join_all(tasks).await;
            list.append(&mut results);
            self.size += results.iter().map(|x| x.size).sum::<u64>();

            self.size
        }
        .boxed()
    }

    fn print(&self, level: i32) {
        let mut prefix = (0..level - 1).map(|_| "| ").collect::<String>();
        prefix.push_str("|_");

        let list = self.kind.list();

        for entity in list.iter() {
            let path = &entity.path;
            let ratio = if self.size != 0 {
                entity.size as f64 * 100.0 / self.size as f64
            } else {
                0.0
            };

            match entity.kind {
                FSType::Folder(_) => {
                    println!(
                        "FOLDER {} {} [{} = {:.2}%]",
                        prefix,
                        format_path(path),
                        format_size(entity.size),
                        ratio
                    );
                    entity.print(level + 1)
                }
                FSType::File => println!(
                    "FILE   {} {} [{} = {:.2}%]",
                    prefix,
                    format_path(path),
                    format_size(entity.size),
                    ratio
                ),
            }
        }
    }
}

#[async_std::main]
async fn main() {
    let f = FSEntity::folder(".".to_owned()).await;
    f.print(0);
}

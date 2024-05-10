use core::panic;
use std::cmp::Ordering;
use std::fmt::Display;

use async_std::fs::{metadata, read_dir};
use async_std::path::{Path, PathBuf};
use async_std::task::spawn;
use futures::future::{join_all, BoxFuture};
use futures::{FutureExt, StreamExt};

#[derive(Eq, PartialEq)]
enum FSType {
    Folder(Vec<FSEntity>),
    File,
}

impl PartialOrd for FSType {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FSType {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (FSType::File, FSType::Folder(_)) => Ordering::Less,
            (FSType::Folder(_), FSType::File) => Ordering::Greater,
            (_, _) => Ordering::Equal,
        }
    }
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

    fn printable_description(&self) -> &'static str {
        match self {
            Self::Folder(_) => "FOLDER",
            Self::File => "FILE",
        }
    }
}

impl Display for FSType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.printable_description())
    }
}

#[derive(Eq, PartialEq)]
struct FSEntity {
    path: PathBuf,
    size: u64,
    kind: FSType,
}

impl PartialOrd for FSEntity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FSEntity {
    fn cmp(&self, other: &Self) -> Ordering {
        Ord::cmp(&self.kind, &other.kind)
            .then(Ord::cmp(&self.size, &other.size))
            .then(Ord::cmp(&self.path, &other.path))
    }
}

fn format_size(size: u64) -> String {
    if size / (1024 * 1024 * 1024) != 0 {
        format!("{:.2} GB", size as f64 / (1024 * 1024 * 1024) as f64)
    } else if size / (1024 * 1024) != 0 {
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
    async fn file(name: impl Into<PathBuf>) -> Self {
        let path = name.into();
        FSEntity {
            size: metadata(&path).await.map(|map| map.len()).unwrap_or(0),
            path,
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

                if file_type.is_file() {
                    list.push(FSEntity::file(path).await)
                } else {
                    tasks.push(spawn(async { FSEntity::folder(path).await }));
                }
            }
            let mut results = join_all(tasks).await;
            list.append(&mut results);
            list.sort_by(|a, b| b.cmp(a));
            self.size += list.iter().map(|x| x.size).sum::<u64>();
            self.size
        }
        .boxed()
    }
}

fn print(parent: &FSEntity, level: u32) {
    let mut prefix = (0..level).map(|_| "|").collect::<String>();
    prefix.push_str("|_");

    let list = parent.kind.list();

    for entity in list.iter() {
        let path = &entity.path;
        let ratio = if entity.size != 0 {
            entity.size as f64 * 100.0 / parent.size as f64
        } else {
            0.0
        };

        println!(
            "{typ}\t[{size} = {ratio:.2}%]\t{prefix} {path}",
            typ = entity.kind,
            path = format_path(path),
            size = format_size(entity.size),
        );

        if let FSType::Folder(_) = entity.kind {
            print(entity, level + 1)
        }
    }
}

#[async_std::main]
async fn main() {
    println!("{}", std::env::current_dir().unwrap().display());

    let f = FSEntity::folder(".".to_owned()).await;
    print(&f, 0);
}

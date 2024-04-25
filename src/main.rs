use async_std::fs::read_dir;
use async_std::path::{Path, PathBuf};
use async_std::stream::StreamExt;
use async_std::task::spawn;
use futures::future::{join_all, BoxFuture};
use futures::FutureExt;

#[derive(Debug)]
struct Folder {
    path: PathBuf,
    folders: Vec<Folder>,
    content_size: u64,
    files: Vec<(PathBuf, u64)>,
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
        format!("{}", path.display())
    }
}

impl Folder {
    fn new(name: impl Into<PathBuf>) -> Self {
        Folder {
            path: name.into(),
            folders: Vec::new(),
            content_size: 0,
            files: Vec::new(),
        }
    }

    fn calculate_size(&mut self) -> BoxFuture<u64> {
        async move {
            let mut tasks = vec![];

            let Ok(mut dir) = read_dir(self.path.to_string_lossy().into_owned()).await else {
                return 0;
            };

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
                    let len = metadata.len();
                    self.content_size += len;
                    self.files.push((PathBuf::from(entry.file_name()), len));
                } else {
                    let task = spawn(async {
                        let mut folder = Folder::new(path);
                        let len = folder.calculate_size().await;
                        (len, folder)
                    });

                    tasks.push(task);
                }
            }
            let results = join_all(tasks).await;
            let (sizes, folders): (Vec<_>, Vec<_>) = results.into_iter().unzip();

            self.content_size += sizes.iter().sum::<u64>();
            self.folders = folders;

            self.folders
                .sort_by(|a, b| b.content_size.partial_cmp(&a.content_size).unwrap());
            self.files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            self.content_size
        }
        .boxed()
    }

    fn print(&self, level: i32) {
        let prefix = (0..level).map(|_| "->").collect::<String>();

        for folder in self.folders.iter() {
            let path = &folder.path;
            let ratio = if self.content_size != 0 {
                folder.content_size as f64 * 100.0 / self.content_size as f64
            } else {
                0.0
            };

            //if ratio > 1.0 {
            println!(
                "FOLDER {} {} [{} = {:.2}%]",
                prefix,
                format_path(path),
                format_size(folder.content_size),
                ratio
            );
            folder.print(level + 1);
            //}
        }
        for file in self.files.iter() {
            let path = &self.path;
            let ratio = if self.content_size != 0 {
                file.1 as f64 * 100.0 / self.content_size as f64
            } else {
                0.0
            };

            println!(
                "FILE   {} {} [{} = {:.2}%]",
                prefix,
                format_path(&path.join(&file.0)),
                (format_size(file.1)),
                ratio
            );
        }
    }
}

#[async_std::main]
async fn main() {
    let mut f = Folder::new("C:\\".to_owned());
    let _ = f.calculate_size().await;
    f.print(0);
}

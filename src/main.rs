use fst::Map;
// use rkyv::{Archive, Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::FileType as StdFileType;
use std::fs::{read_dir, DirEntry, Metadata};
use std::io::Result as IoResult;
use std::path::{Component, Path, PathBuf};
use time::TimeString;

mod time;

struct PathIndex {
    index_maps: Vec<Map<Vec<u8>>>,
    meta_store: MetaStore,
}

impl PathIndex {
    fn get_index(&self, path: &Path) -> Option<usize> {
        unimplemented!()
    }
}

pub struct SplitPath {
    path: PathBuf,
    components: Vec<String>,
    fst_outputs: Option<Vec<usize>>,
}

impl From<&Path> for SplitPath {
    fn from(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            components: path
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect(),
            fst_outputs: None,
        }
    }

    fn gen_fst_outputs(&mut self, index: usize) {}
}

pub struct PathIndexBuilder {
    paths: Vec<SplitPath>,
    meta_store: MetaStore,
}

impl PathIndexBuilder {
    fn insert_path(&mut self, path: &Path) {
        self.paths.push(path.into());
    }
}

impl From<PathIndexBuilder> for PathIndex {
    fn from(mut builder: PathIndexBuilder) -> Self {
        builder
            .paths
            .iter_mut()
            .for_each(|path| path.gen_fst_outputs(builder.meta_store.insert(path.path.as_path())));
        for (path, components) in builder
            .paths
            .into_iter()
            .zip(builder.path_components.into_iter())
        {
            components.sort_unstable();
            builder.meta_store.insert(&path);
        }
        unimplemented!()
    }
}

// #[derive(Archive, Serialize, Deserialize)]
struct MetaStore {
    count: usize,
    store: HashMap<usize, Option<PathMeta>>,
    free_indices: Vec<usize>,
}

impl MetaStore {
    fn generate_index(&mut self) -> usize {
        self.free_indices.pop().unwrap_or_else(|| {
            let index = self.count;
            self.count += 1;
            index
        })
    }

    fn insert(&mut self, path: &Path) -> usize {
        let index = self.generate_index();
        self.store
            .insert(index, path.metadata().ok().map(PathMeta::from))
            .unwrap();
        index
    }
}

// #[derive(Archive, Serialize, Deserialize)]
enum FileType {
    Directory,
    File,
    SymLink,
    Unknown,
}

impl From<StdFileType> for FileType {
    fn from(ft: StdFileType) -> Self {
        if ft.is_dir() {
            Self::Directory
        } else if ft.is_file() {
            Self::File
        } else if ft.is_symlink() {
            Self::SymLink
        } else {
            Self::Unknown
        }
    }
}

// #[derive(Archive, Serialize, Deserialize)]
struct PathMeta {
    file_type: FileType,
    modified: Option<String>,
    accessed: Option<String>,
    created: Option<String>,
}

impl From<Metadata> for PathMeta {
    fn from(metadata: Metadata) -> Self {
        Self {
            file_type: metadata.file_type().into(),
            modified: metadata
                .modified()
                .ok()
                .map(|sys_time| TimeString::from(sys_time).into()),
            accessed: metadata
                .accessed()
                .ok()
                .map(|sys_time| TimeString::from(sys_time).into()),
            created: metadata
                .created()
                .ok()
                .map(|sys_time| TimeString::from(sys_time).into()),
        }
    }
}

fn queue_fst_creation(path: PathBuf) {
    unimplemented!()
}

fn normalize(path: &Path) -> PathBuf {
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
        components.next();
        PathBuf::from(c.as_os_str())
    } else {
        PathBuf::new()
    };

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}

fn main() {
    println!("Hello, world!");
    let drive_entries = read_dir("C:\\")
        .expect("Could not open C:\\")
        .collect::<IoResult<Vec<DirEntry>>>()
        .unwrap();

    let per_segment_fst: Vec<Map<Vec<u8>>> = vec![];

    for entry in drive_entries.iter() {
        let path = normalize(&entry.path());
        let metadata = entry
            .metadata()
            .map_err(|io_err| {
                format!(
                    "Could not get metadata for path {}. Reason: {io_err}",
                    path.to_string_lossy()
                )
            })
            .unwrap();
        for component in path.components() {}
        if metadata.is_dir() {
            queue_fst_creation(path)
        }
    }
}

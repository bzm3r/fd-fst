use chrono::{DateTime, Utc};
use fst::Map;
use rkyv::AlignedVec;
use time::TimeString;
use std::fs::{read_dir, DirEntry};
use std::io::Result as IoResult;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rkyv::{
    collections::hash_map::ArchivedHashMap, vec::ArchivedVec, Archive, Deserialize, Serialize,
};

mod time;



type FstBytes = AlignedVec;

#[derive(Archive, Serialize, Deserialize)]
struct PathIndexer {
    automata: Vec<FstMap>,
}

impl PathIndexer {
    fn get_index(&self, path: &Path) -> usize {
        unimplemented!()
    }
}

// #[derive(Archive, Serialize, Deserialize)]
struct PathStore {
    store: ArchivedHashMap<usize, Option<PathMetadata>>,
}

enum FileType {
    Directory,
    File { len: u64 },
    SymLink { target: String },
}

struct PathMetadata {
    path: String,
    file_type: FileType,
    modified:
}




// source
// pub fn permissions(&self) -> Permissions

// Returns the permissions of the file this metadata is for.
// Examples

// use std::fs;

// fn main() -> std::io::Result<()> {
//     let metadata = fs::metadata("foo.txt")?;

//     assert!(!metadata.permissions().readonly());
//     Ok(())
// }

// 1.10.0 · source
// pub fn modified(&self) -> Result<SystemTime>

// Returns the last modification time listed in this metadata.

// The returned value corresponds to the mtime field of stat on Unix platforms and the ftLastWriteTime field on Windows platforms.
// Errors

// This field might not be available on all platforms, and will return an Err on platforms where it is not available.
// Examples

// use std::fs;

// fn main() -> std::io::Result<()> {
//     let metadata = fs::metadata("foo.txt")?;

//     if let Ok(time) = metadata.modified() {
//         println!("{time:?}");
//     } else {
//         println!("Not supported on this platform");
//     }
//     Ok(())
// }

// 1.10.0 · source
// pub fn accessed(&self) -> Result<SystemTime>

// Returns the last access time of this metadata.

// The returned value corresponds to the atime field of stat on Unix platforms and the ftLastAccessTime field on Windows platforms.

// Note that not all platforms will keep this field update in a file’s metadata, for example Windows has an option to disable updating this time when files are accessed and Linux similarly has noatime.
// Errors

// This field might not be available on all platforms, and will return an Err on platforms where it is not available.
// Examples

// use std::fs;

// fn main() -> std::io::Result<()> {
//     let metadata = fs::metadata("foo.txt")?;

//     if let Ok(time) = metadata.accessed() {
//         println!("{time:?}");
//     } else {
//         println!("Not supported on this platform");
//     }
//     Ok(())
// }

// 1.10.0 · source
// pub fn created(&self) -> Result<SystemTime>

// Returns the creation time listed in this metadata.

// The returned value corresponds to the btime field of statx on Linux kernel starting from to 4.11, the birthtime field of stat on other Unix platforms, and the ftCreationTime field on Windows platforms.
// Errors

// This field might not be available on all platforms, and will return an Err on platforms or filesystems where it is not available.
// Examples

// use std::fs;

// fn main() -> std::io::Result<()> {
//     let metadata = fs::metadata("foo.txt")?;

//     if let Ok(time) = metadata.created() {
//         println!("{time:?}");
//     } else {
//         println!("Not supported on this platform or filesystem");
//     }
//     Ok(())
// }

// struct NestedFst {
//     fst: IndexFst,
//     outputs: Vec<FstOut>,
// }

fn queue_fst_creation(path: PathBuf) {
    unimplemented!();
}

fn main() {
    println!("Hello, world!");
    let drive_entries = read_dir("C:\\")
        .expect("Could not open C:\\")
        .collect::<IoResult<Vec<DirEntry>>>()
        .unwrap();
    for entry in drive_entries.iter() {
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|io_err| {
                format!(
                    "Could not get metadata for path {}. Reason: {}",
                    path.to_string_lossy(),
                    io_err
                )
            })
            .unwrap();
        if metadata.is_dir() {
            queue_fst_creation(path)
        }
    }
}

use std::fs;
use std::path::Path;

use bzip2::read::MultiBzDecoder;
use tar::Archive;

use crate::download;

pub fn extract(path: &str, folder: &str) {
    let file = fs::File::open(path).unwrap();

    let decoder = MultiBzDecoder::new(file);

    println!("Unpacking file...");
    let time = std::time::Instant::now();
    let mut archive = Archive::new(decoder);
    archive.unpack(folder).unwrap();
    println!("Done. Took {}s...", time.elapsed().as_secs());
}

pub async fn download_and_extract(url: &str, file_path: &str, folder: &str) -> Result<(), ()> {
    let folder_exists: bool = Path::new(folder).is_dir();

    if !folder_exists {
        let path = std::path::Path::new(file_path);

        if !path.is_file() {
            download::download_file(url, path.to_str().unwrap())
                .await
                .unwrap()
        }

        extract(file_path, folder);
    }

    Ok(())
}

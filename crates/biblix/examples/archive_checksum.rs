//! The checksum of a package unpacked from a `.tar.gz` (`git archive`, or a host's download):
//! `cargo run -p biblix --example archive_checksum -- kjv.tar.gz`

use biblix::{MemoryStorage, archive, package::checksum};

fn main() -> biblix::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).map_err(|e| biblix::BiblixError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let files = MemoryStorage::new();
        archive::unpack(&bytes, &files)?;
        println!("{path}: {}", checksum(&files)?);
    }
    Ok(())
}

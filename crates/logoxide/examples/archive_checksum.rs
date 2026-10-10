//! The checksum of a package unpacked from a `.tar.gz` (`git archive`, or a host's download):
//! `cargo run -p logoxide --example archive_checksum -- kjv.tar.gz`

use logoxide::{MemoryStorage, net::archive, package::checksum};

fn main() -> logoxide::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes =
            std::fs::read(&path).map_err(|e| logoxide::LogoxideError::io(&path.clone(), e))?;
        let files = MemoryStorage::new();
        archive::unpack(&bytes, &files)?;
        println!("{path}: {}", checksum(&files)?);
    }
    Ok(())
}

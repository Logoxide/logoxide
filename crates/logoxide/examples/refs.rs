//! A repository's refs, from the answer to its `info/refs` (as a host downloads it):
//! `curl -o refs.bin "$(...)/info/refs?service=git-upload-pack" && cargo run -p logoxide --example refs -- refs.bin`

fn main() -> logoxide::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).map_err(|e| logoxide::LogoxideError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let refs = logoxide::archive::Refs::parse(&bytes)?;
        println!("HEAD {:?}", refs.head);
        for (tag, commit) in &refs.tags {
            println!("tag {tag} {commit}");
        }
        for (branch, commit) in &refs.branches {
            println!("branch {branch} {commit}");
        }
    }
    Ok(())
}

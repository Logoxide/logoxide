//! A repository's refs, from the answer to its `info/refs` (as a host downloads it):
//! `curl -o refs.bin "$(...)/info/refs?service=git-upload-pack" && cargo run -p biblix --example refs -- refs.bin`

fn main() -> biblix::Result<()> {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).map_err(|e| biblix::BiblixError::Io {
            path: path.clone(),
            message: e.to_string(),
        })?;
        let refs = biblix::archive::Refs::parse(&bytes)?;
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

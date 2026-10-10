//! git, as lx runs it, runs nothing a repository asks for: hooks, Git LFS's filters, and
//! `core.fsmonitor`. This is the binary's only test, since it points git at its own global config

#![cfg(unix)]

use std::{os::unix::fs::PermissionsExt, path::Path, process::Command};

use logoxide::{
    MemoryStorage, Storage,
    git::{Fetcher, GitCli, GitRef},
};

fn run(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn git_runs_nothing_a_repository_asks_for() {
    let root = std::env::temp_dir().join(format!("logoxide-git-runs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (hooks, repo) = (root.join("hooks"), root.join("repo"));
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::create_dir_all(&repo).unwrap();
    let ran = |what: &str| root.join(format!("{what}-ran"));
    // The user's own hooks and an LFS filter that must run, as `git lfs install` sets it
    script(
        &hooks.join("post-checkout"),
        &format!("touch '{}'", ran("hook").display()),
    );
    let config = root.join("gitconfig");
    std::fs::write(
        &config,
        format!(
            "[user]\n\tname = t\n\temail = t@t\n[commit]\n\tgpgsign = false\n\
             [init]\n\tdefaultBranch = main\n[core]\n\thooksPath = {}\n\
             [filter \"lfs\"]\n\tclean = cat\n\tsmudge = \"touch '{}'; cat\"\n\trequired = true\n",
            hooks.display(),
            ran("lfs").display()
        ),
    )
    .unwrap();
    // SAFETY: this binary's only test, and no thread has started yet
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &config);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }

    run(&repo, &["init", "-q"]);
    let pointer = "version https://git-lfs.github.com/spec/v1\noid sha256:00\nsize 3\n";
    std::fs::write(repo.join("audio.bin"), pointer).unwrap();
    std::fs::write(repo.join(".gitattributes"), "*.bin filter=lfs\n").unwrap();
    std::fs::write(repo.join("text.txt"), "text").unwrap();
    run(&repo, &["add", "-A"]);
    run(&repo, &["commit", "-q", "-m", "first"]);
    run(&repo, &["tag", "v1.0.0"]);
    let fsmonitor = root.join("fsmonitor");
    script(
        &fsmonitor,
        &format!("touch '{}'; exit 1", ran("fsmonitor").display()),
    );
    run(
        &repo,
        &["config", "core.fsmonitor", &fsmonitor.display().to_string()],
    );

    // Plain git runs all three, so the test would see them
    run(
        &root,
        &["clone", "-q", &repo.display().to_string(), "clone"],
    );
    run(&repo, &["status", "--porcelain"]);
    for what in ["hook", "lfs", "fsmonitor"] {
        assert!(ran(what).exists(), "{what} didn't run for plain git");
        std::fs::remove_file(ran(what)).unwrap();
    }

    let cli = GitCli::default();
    let url = repo.display().to_string();
    let all = MemoryStorage::new();
    cli.fetch(&url, &GitRef::Tag(String::from("v1.0.0")), &all)
        .unwrap();
    // LFS's pointer is kept as it is, so the package hashes the same with or without LFS
    assert_eq!(all.read_string("audio.bin").unwrap(), pointer);
    let some = MemoryStorage::new();
    cli.fetch_files(&url, &GitRef::Head, &["audio.bin"], &some)
        .unwrap();
    assert_eq!(some.read_string("audio.bin").unwrap(), pointer);
    assert_eq!(cli.tags(&url).unwrap().len(), 1);

    // In a repository of the user's, fsmonitor stays off
    assert_eq!(cli.package_in(&repo).unwrap().1, "");
    assert_eq!(cli.local_versions(&repo).unwrap().len(), 1);
    std::fs::write(repo.join("text.txt"), "more text").unwrap();
    cli.commit_and_tag(&repo, "second", "v1.0.1").unwrap();
    logoxide::publish::registry_build::git_in(&repo, &["status", "--porcelain"]).unwrap();

    for what in ["hook", "lfs", "fsmonitor"] {
        assert!(!ran(what).exists(), "{what} ran");
    }
    let _ = std::fs::remove_dir_all(&root);
}

//! Picks file paths out of tool inputs and shell commands, and resolves them against the agent's working directory.

use std::path::{Component, Path, PathBuf};

/// Words in `text` that look like paths: absolute, home-relative, or climbing out with `..`.
pub fn candidates(text: &str) -> Vec<String> {
    let separators = |c: char| c.is_whitespace() || "'\"`;|&<>()=,:".contains(c);
    text.split(separators)
        .filter(|word| {
            word.starts_with('/')
                || word.starts_with("~/")
                || *word == "~"
                || word.starts_with("$HOME")
                || word.starts_with("${HOME}")
                || word.starts_with("..")
        })
        .filter(|word| !word.starts_with("//"))
        .map(str::to_string)
        .collect()
}

/// The absolute, `..`-free form of a path word, read from `cwd`.
pub fn resolve(word: &str, cwd: &Path, home: &Path) -> PathBuf {
    let home_relative = ["~/", "$HOME/", "${HOME}/"]
        .iter()
        .find_map(|prefix| word.strip_prefix(prefix));
    let path = if let Some(rest) = home_relative {
        home.join(rest)
    } else if ["~", "$HOME", "${HOME}"].contains(&word) {
        home.to_path_buf()
    } else {
        cwd.join(word)
    };
    normalise(&path)
}

/// Removes `.` and `..` without touching the disk, so paths that do not exist still compare.
/// On macOS `/tmp` and `/var` are links into `/private`; both spellings compare equal after this.
pub fn normalise(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    for linked in ["/tmp", "/var"] {
        if out.starts_with(linked) {
            return Path::new("/private").join(out.strip_prefix("/").unwrap_or(&out));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_words_are_found_in_a_shell_command() {
        let found =
            candidates("cd ../.. && cat ~/notes.txt \"/etc/passwd\" src/main.rs --flag=/opt/x");
        assert_eq!(found, ["../..", "~/notes.txt", "/etc/passwd", "/opt/x"]);
    }

    #[test]
    fn climbing_out_of_the_workspace_resolves_outside_it() {
        let resolved = resolve("../../other", Path::new("/r/id/ws"), Path::new("/home/u"));
        assert_eq!(resolved, PathBuf::from("/r/other"));
        assert_eq!(
            resolve("~/x", Path::new("/r"), Path::new("/home/u")),
            PathBuf::from("/home/u/x")
        );
    }
}

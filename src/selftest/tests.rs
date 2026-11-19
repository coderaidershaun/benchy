//! Self-test tests: each synthetic workspace is made from the real references and holds the change it claims.

use super::*;
use crate::score::sources;
use crate::score::test_code::Suite;

fn benchy() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace(variant: Variant) -> tempfile::TempDir {
    let breakage: Breakage = read_toml(&benchy().join("validation/selftest/broken.toml")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    make_workspace(&benchy(), &dir.path().join("w"), variant, &breakage).unwrap();
    dir
}

fn lines(root: &Path, language: Language) -> usize {
    sources::production_line_total(&sources::production_files(root, language))
}

#[test]
fn deleting_tests_leaves_no_test_and_the_same_production_code() {
    let reference = workspace(Variant::Reference);
    let changed = workspace(Variant::NoTests);

    for language in Language::ALL {
        let root = changed.path().join("w");
        assert_eq!(
            Suite::read(&root, language).functions.len(),
            0,
            "{language:?}"
        );
        assert_eq!(
            lines(&root, language),
            lines(&reference.path().join("w"), language)
        );
    }
    let solution = fs::read_to_string(changed.path().join("w/csharp/Engine.slnx")).unwrap();
    assert!(!solution.contains("Tests"));
}

#[test]
fn dead_code_doubles_the_production_lines() {
    let reference = workspace(Variant::Reference);
    let changed = workspace(Variant::DeadCode);

    for language in Language::ALL {
        let before = lines(&reference.path().join("w"), language);
        let after = lines(&changed.path().join("w"), language);
        assert!(
            after >= 2 * before && after < 2 * before + 12,
            "{language:?}: {before} -> {after}"
        );
    }
}

#[test]
fn every_tautological_test_returns_before_its_own_code() {
    let changed = workspace(Variant::TautologicalTests);

    for language in Language::ALL {
        let root = changed.path().join("w");
        let suite = Suite::read(&root, language);
        let tests: Vec<_> = suite.tests().collect();
        assert!(tests.len() >= 10, "{language:?}");
        for test in tests {
            let code: String = suite.files[test.file]
                .text
                .chars()
                .skip(match test.body {
                    crate::score::test_code::Body::Block { open, .. } => open,
                    crate::score::test_code::Body::Expression { arrow, .. } => arrow,
                })
                .take(120)
                .collect();
            assert!(
                code.contains("return"),
                "{language:?} {}: {code}",
                test.name
            );
        }
    }
}

#[test]
fn the_broken_feature_and_the_filler_comments_change_what_they_say() {
    let broken = workspace(Variant::BrokenFeature);
    let filler = workspace(Variant::FillerDocs);
    let reference = workspace(Variant::Reference);

    let book = fs::read_to_string(broken.path().join("w/rust/src/book.rs")).unwrap();
    assert!(book.contains("if maker_id == taker.id {"));
    for language in Language::ALL {
        let comments = |root: &Path| -> usize {
            sources::production_files(root, language)
                .iter()
                .map(|f| f.comment_lines)
                .sum()
        };
        let before = comments(&reference.path().join("w"));
        let after = comments(&filler.path().join("w"));
        assert!(after > before + 10, "{language:?}: {before} -> {after}");
        assert_eq!(
            lines(&filler.path().join("w"), language),
            lines(&reference.path().join("w"), language)
        );
    }
}

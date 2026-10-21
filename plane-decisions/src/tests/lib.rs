//! Tests that hold the crate root's own prose to the tree it describes.

const LIB_RS: &str = include_str!("../lib.rs");

/// The crate's test files the crate root cites, by the path a doc cites them under. Read at compile
/// time, so a cited file that does not exist is a file this table cannot name, and a test it cites
/// is one whose definition can be searched for without the test reaching outside the crate at run
/// time.
const TEST_FILES: &[(&str, &str)] = &[(
    "tests/pii_witness.rs",
    include_str!("../../tests/pii_witness.rs"),
)];

/// Every `tests/<file>.rs` the crate root cites as the home of a guarantee is one of the crate's
/// test files, and a cited `::test_name` is a test that file defines.
#[test]
fn every_cited_test_file_exists_and_defines_the_cited_test() {
    let mut cited = 0;
    for token in LIB_RS.split('`').skip(1).step_by(2) {
        if !token.starts_with("tests/") && !token.starts_with("src/tests/") {
            continue;
        }
        let (file, test) = match token.split_once("::") {
            Some((file, test)) => (file, Some(test)),
            None => (token, None),
        };
        let Some((_, body)) = TEST_FILES.iter().find(|(path, _)| *path == file) else {
            panic!("a doc cites `{token}`, and the crate has no test file {file}");
        };
        if let Some(test) = test {
            assert!(
                body.contains(&format!("fn {test}(")),
                "a doc cites `{token}`, and {file} defines no such test"
            );
        }
        cited += 1;
    }
    assert!(cited >= 1, "the crate root cites its PII witness");
}

/// The crate root describes the served door: it names no crate the manifest does not, and claims
/// no statelessness the door does not have.
#[test]
fn the_crate_root_describes_the_door() {
    assert!(!LIB_RS.contains("busbar-substrate"));
    assert!(!LIB_RS.contains("DecisionPlane"));
    assert!(LIB_RS.contains("What it holds across calls"));
    let manifest = include_str!("../../Cargo.toml");
    assert!(!manifest
        .split("[dependencies]")
        .nth(1)
        .is_some_and(|deps| deps.contains("busbar-substrate")));
}

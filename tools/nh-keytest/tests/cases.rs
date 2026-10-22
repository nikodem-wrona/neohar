//! Runs the logical part of every case of `tests/keyboard/cases` through the real encoder.
//! The browser part is `tests/keyboard/keyboard.pw.ts`.

use std::path::{Path, PathBuf};

use nh_keytest::{Who, load_cases, parse_hex, run};

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/keyboard/cases")
}

#[test]
fn the_encoder_writes_the_expected_bytes_for_every_case() {
    let cases = load_cases(&cases_dir()).expect("the cases load");
    let mut failures = Vec::new();
    let mut ran = 0;
    for case in cases.iter().filter(|c| c.has_direct_input()) {
        ran += 1;
        let encoded = run(&case.request()).expect("the request runs");
        let expected = parse_hex(&case.expect).expect("the expected bytes are hexadecimal");
        let got = parse_hex(&encoded.hex).expect("the output is hexadecimal");
        if got != expected || encoded.delivery != case.delivery {
            failures.push(format!(
                "{}: expected {} ({:?}), got {} ({:?})",
                case.name, case.expect, case.delivery, encoded.hex, encoded.delivery
            ));
        }
    }
    assert!(ran > 0, "no case has logical input");
    assert!(
        failures.is_empty(),
        "{} of {ran} cases failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_matrix_is_large_and_every_case_names_its_source() {
    let cases = load_cases(&cases_dir()).expect("the cases load");
    assert!(cases.len() >= 400, "only {} cases", cases.len());
    let with_events = cases.iter().filter(|c| !c.events.is_empty()).count();
    assert!(
        with_events >= 400,
        "only {with_events} cases have browser events"
    );
    // A case with browser events and a logical part gets both checks.
    assert!(
        cases
            .iter()
            .any(|c| !c.events.is_empty() && c.has_direct_input())
    );
    assert!(
        cases
            .iter()
            .all(|c| c.delivery == Who::Application || c.expect.is_empty())
    );
}

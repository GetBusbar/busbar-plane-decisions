use super::*;

const SUCCESS: &[u8] =
    br#"{"request_id":"req_1","usage":{"units":42},"answers":{"decision":"approve"}}"#;

#[test]
fn reads_the_declared_pointers_off_a_success_body() {
    assert_eq!(read_raw(SUCCESS, PTR_USAGE_UNITS), Some(&b"42"[..]));
    assert_eq!(read_raw(SUCCESS, PTR_USAGE_INPUT_TOKENS), None);
}

/// A request is read at its top-level `model` alone (DECISIONS D8b), never at a `model` nested in
/// its `state`.
#[test]
fn a_request_is_read_at_its_top_level_model() {
    assert_eq!(
        read_raw(br#"{"state":{"model":"inner"},"model":"jev"}"#, PTR_MODEL),
        Some(&br#""jev""#[..])
    );
    assert_eq!(read_raw(br#"{"state":{"model":"inner"}}"#, PTR_MODEL), None);
}

/// No pointer the plane reads passes through a `state` or `answers` member (a member merely
/// spelled with those letters, as NanoJev's `/execution/states`, is another member).
#[test]
fn no_declared_pointer_names_state_or_answers() {
    for p in [
        PTR_MODEL,
        PTR_USAGE_UNITS,
        PTR_USAGE_INPUT_TOKENS,
        PTR_USAGE_OUTPUT_TOKENS,
        PTR_EXECUTION_STATES,
    ] {
        assert!(
            !p.split('/').any(|seg| seg == "state" || seg == "answers"),
            "{p}"
        );
    }
}

#[test]
fn the_error_body_is_the_dialects_shape() {
    assert_eq!(
        error_body("invalid_params", "no \"such\" model"),
        br#"{"error":{"code":"invalid_params","message":"no \"such\" model"}}"#
    );
}

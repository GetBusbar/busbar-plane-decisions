use super::*;
use crate::meta::CLASS_DECISION;

#[test]
fn systemone_arrives_as_the_first_op_class_with_a_principal() {
    let a = arrive("POST", ops::PATH_SYSTEMONE).expect("systemone is claimed");
    assert_eq!(a.op, ops::OP_SYSTEMONE);
    assert_eq!(a.op_class, 0);
    assert_eq!(tail::OP_CLASSES[a.op_class as usize], ops::OP_SYSTEMONE);
    assert_eq!(a.principal, PrincipalNeed::Required);
    assert_eq!(tail::DIALECTS[a.dialect as usize], "jev");
}

#[test]
fn models_is_not_claimed() {
    assert_eq!(arrive("GET", ops::PATH_MODELS), None);
    assert!(!tail::CLAIMS.iter().any(|(_, t)| *t == ops::PATH_MODELS));
    assert!(!tail::OP_CLASSES.contains(&ops::OP_MODELS));
}

#[test]
fn an_unknown_request_is_not_claimed() {
    assert_eq!(arrive("GET", ops::PATH_SYSTEMONE), None);
    assert_eq!(arrive("POST", "/v1/elsewhere"), None);
}

#[test]
fn every_claim_arrives() {
    for (verb, target) in tail::CLAIMS {
        assert!(arrive(verb, target).is_some(), "{verb} {target}");
    }
}

#[test]
fn the_tail_names_the_decision_class_at_its_index() {
    let (class, family) = tail::BILLABLE_CLASSES[tail::CLASS_DECISION_INDEX as usize];
    assert_eq!(class, CLASS_DECISION);
    assert_eq!(family, "decision");
    // The request fee is the one fee unit, and it is a class at its own index.
    assert_eq!(tail::FEE_UNITS, &[busbar_contract::plane::PER_REQUEST]);
    let (fee, fee_family) = tail::BILLABLE_CLASSES[tail::CLASS_FEE_INDEX as usize];
    assert_eq!(fee.as_str(), tail::FEE_PER_REQUEST);
    assert_eq!(fee_family, tail::FEE_FAMILY);
    assert_eq!(tail::SECTION_DECLARING, crate::config::SECTION);
    assert_eq!(tail::NEEDS, &[(crate::claims::TRANSPORT, "bearer")]);
    assert_eq!(tail::DIALECT_AUTH, &[(0, "bearer")]);
}

#[test]
fn the_caller_body_is_kept_and_an_empty_one_is_refused() {
    assert_eq!(caller_piece(b"{\"state\":{}}", true), CallerAnswer::Keep);
    assert_eq!(caller_piece(b"", false), CallerAnswer::Keep);
    assert_eq!(caller_piece(b"", true), CallerAnswer::Empty);
}

#[test]
fn an_attempt_forwards_the_caller_body_unchanged() {
    let body = br#"{"state":{"session":"s"},"context":{}}"#;
    let r = attempt_request(body);
    assert_eq!(r.verb, "POST");
    assert_eq!(r.target, "/v1/systemone");
    assert_eq!(r.fields, [("content-type", &b"application/json"[..])]);
    assert_eq!(r.auth, "decision-egress");
    assert!(
        std::ptr::eq(r.body, &body[..]),
        "the body is borrowed, not copied"
    );
}

/// The decision count and the fee a held answer settles to, under `status`.
fn settled(body: &[u8], status: Option<u32>) -> Result<Vec<(u32, u64)>, Uncounted> {
    let mut reading = FarEndReading::new();
    reading.piece(body).expect("under the backstop");
    reading
        .settle(status)
        .map(|s| s.units().iter().map(|u| (u.class, u.amount)).collect())
}

#[test]
fn far_end_pieces_are_held_whole_and_the_count_read_over_the_whole_answer() {
    let mut reading = FarEndReading::new();
    let a = br#"{"request_id":"req_1","usage":{"un"#;
    let b = br#"its":42},"answers":{"decision":"approve"}}"#;
    assert_eq!(reading.piece(a), Ok(()));
    assert_eq!(reading.piece(b), Ok(()));
    assert_eq!(
        reading.settle(Some(200)),
        Ok(Settled::Billed([
            Units {
                class: tail::CLASS_DECISION_INDEX,
                reported: true,
                amount: 42
            },
            Units {
                class: tail::CLASS_FEE_INDEX,
                reported: true,
                amount: 1
            },
            Units {
                class: tail::CLASS_INPUT_TOKENS_INDEX,
                reported: true,
                amount: 0
            },
            Units {
                class: tail::CLASS_OUTPUT_TOKENS_INDEX,
                reported: true,
                amount: 0
            },
        ]))
    );
}

/// ONE SUCCESS RULE ($, finding 4): a `2xx` status and no `/error` member (a `null` one is none).
/// What is not a success bills nothing at all: no decision, no fee.
#[test]
fn only_a_success_bills_and_the_count_and_fee_follow_one_rule() {
    let counted = br#"{"usage":{"units":3}}"#;
    assert_eq!(
        settled(counted, Some(200)),
        Ok(vec![(0, 3), (1, 1), (2, 0), (3, 0)])
    );
    assert_eq!(
        settled(counted, Some(204)),
        Ok(vec![(0, 3), (1, 1), (2, 0), (3, 0)])
    );
    // RED: a non-2xx answer carrying a count, with no `/error`, used to bill its decisions.
    for status in [Some(422), Some(500), Some(302), None] {
        assert_eq!(settled(counted, status), Ok(vec![]), "{status:?}");
    }
    // An error answer bills nothing, whatever its status.
    let errored = br#"{"error":{"code":"invalid_state"},"usage":{"units":3}}"#;
    assert_eq!(settled(errored, Some(200)), Ok(vec![]));
    // RED: `"error": null` is no error; a 2xx carrying it used to bill neither count nor fee.
    let null_error = br#"{"error":null,"usage":{"units":5}}"#;
    assert_eq!(
        settled(null_error, Some(200)),
        Ok(vec![(0, 5), (1, 1), (2, 0), (3, 0)])
    );
}

/// RED ($, finding 4): a SUCCESS with no whole count is refused, never billed as zero. The count
/// is a whole number a signed 64-bit count holds; zero is a count.
#[test]
fn a_success_with_no_whole_count_is_refused_never_free() {
    for body in [
        &br#"{"usage":{"units":-1}}"#[..],
        br#"{"usage":{"units":1.5}}"#,
        br#"{"usage":{"units":"7"}}"#,
        br#"{"usage":{}}"#,
        br#"{}"#,
    ] {
        assert_eq!(
            settled(body, Some(200)),
            Err(Uncounted),
            "{}",
            String::from_utf8_lossy(body)
        );
    }
    let max = format!(r#"{{"usage":{{"units":{}}}}}"#, i64::MAX);
    assert_eq!(
        settled(max.as_bytes(), Some(200)),
        Ok(vec![(0, i64::MAX as u64), (1, 1), (2, 0), (3, 0)])
    );
    let past = format!(r#"{{"usage":{{"units":{}}}}}"#, i64::MAX as u64 + 1);
    assert_eq!(settled(past.as_bytes(), Some(200)), Err(Uncounted));
    assert_eq!(
        settled(br#"{"usage":{"units":0}}"#, Some(200)),
        Ok(vec![(0, 0), (1, 1), (2, 0), (3, 0)])
    );
    // Not a success: no count is owed, so none is missing.
    assert_eq!(settled(br#"{}"#, Some(503)), Ok(vec![]));
}

#[test]
fn the_units_never_read_state_or_answers() {
    assert_eq!(
        settled(
            br#"{"usage":{"units":7},"state":{"units":999},"answers":{"units":888}}"#,
            Some(200)
        ),
        Ok(vec![(0, 7), (1, 1), (2, 0), (3, 0)])
    );
    assert_eq!(
        settled(
            br#"{"state":{"usage":{"units":9}},"answers":{"usage":{"units":9}}}"#,
            Some(200)
        ),
        Err(Uncounted)
    );
}

#[test]
fn a_refusal_carries_the_kernel_text_in_the_dialect_shape() {
    assert_eq!(
        refusal_body(403, "the caller may not perform this operation"),
        br#"{"error":{"code":"unsupported_operation","message":"the caller may not perform this operation"}}"#
    );
    assert_eq!(
        refusal_body(401, "no key"),
        br#"{"error":{"code":"invalid_request","message":"no key"}}"#
    );
    assert_eq!(
        refusal_body(404, "no provider"),
        br#"{"error":{"code":"invalid_params","message":"no provider"}}"#
    );
    assert_eq!(
        refusal_body(500, "x"),
        br#"{"error":{"code":"internal","message":"x"}}"#
    );
    assert_eq!(
        refusal_body(503, "x"),
        br#"{"error":{"code":"unsupported_operation","message":"x"}}"#
    );
}

#[test]
fn any_configured_model_mounts_the_claim() {
    assert!(served_claims(0).is_empty());
    assert_eq!(served_claims(1), tail::CLAIMS);
    // RED (finding 1): several models used to mount nothing.
    assert_eq!(served_claims(2), tail::CLAIMS);
    assert_eq!(served_claims(7), tail::CLAIMS);
}

fn models(json: &str) -> Models {
    Models::of(&crate::plane_door::read_settings(json.as_bytes()).expect("a section"))
}

/// MODEL RESOLUTION (DECISIONS D8b), every case a cell.
#[test]
fn a_request_routes_by_its_model() {
    let one = models(r#"{"models":{"jev":{"provider":"typesafe"}}}"#);
    let two = models(
        r#"{"models":{"jev":{"provider":"typesafe","upstream_model":"jev-1.13.0"},"alt":{"provider":"typesafe","upstream_model":"alt"}}}"#,
    );
    // One configured, none named: the default, nothing to rewrite.
    for body in [&br#"{"state":{}}"#[..], br#"{"model":null,"state":{}}"#] {
        assert_eq!(
            one.resolve(body),
            Ok(Routed {
                model: "jev",
                upstream: None
            })
        );
    }
    // One configured, it named.
    assert_eq!(
        one.resolve(br#"{"model":"jev"}"#).map(|r| r.model),
        Ok("jev")
    );
    // RED: one configured, an unknown one named, is 404 (predev routed it to the one).
    assert_eq!(one.resolve(br#"{"model":"other"}"#), Err(Unrouted::Unknown));
    assert_eq!(Unrouted::Unknown.status(), 404);
    // RED: several configured, none named, is 400.
    assert_eq!(two.resolve(br#"{"state":{}}"#), Err(Unrouted::NoneNamed));
    assert_eq!(Unrouted::NoneNamed.status(), 400);
    // Several configured, one named: it, with its differing upstream name.
    assert_eq!(
        two.resolve(br#"{"model":"jev"}"#),
        Ok(Routed {
            model: "jev",
            upstream: Some("jev-1.13.0")
        })
    );
    // An `upstream_model` equal to the key rewrites nothing.
    assert_eq!(
        two.resolve(br#"{"model":"alt"}"#),
        Ok(Routed {
            model: "alt",
            upstream: None
        })
    );
    // A `model` that is not a string is 400; the name is matched exactly, never inside `state`.
    assert_eq!(two.resolve(br#"{"model":7}"#), Err(Unrouted::NotAName));
    assert_eq!(Unrouted::NotAName.status(), 400);
    assert_eq!(
        two.resolve(br#"{"state":{"model":"jev"}}"#),
        Err(Unrouted::NoneNamed)
    );
    // No model configured: nothing routes (the generation mounts no claim).
    assert_eq!(
        models(r#"{"models":{}}"#).resolve(br#"{}"#),
        Err(Unrouted::Unknown)
    );
}

/// THE `upstream_model` SPLICE (DECISIONS D8b): only the top-level `model` value changes, every
/// other byte (spacing, order, a nested `model`) is kept; with no top-level `model` string there is
/// nothing to splice.
#[test]
fn upstream_model_rewrites_only_the_top_level_model_value() {
    let body = br#"{ "state" : {"model":"jev"},  "model" : "jev" ,"context":{}}"#;
    assert_eq!(
        splice_model(body, "jev-1.13.0").as_deref(),
        Some(&br#"{ "state" : {"model":"jev"},  "model" : "jev-1.13.0" ,"context":{}}"#[..])
    );
    assert_eq!(
        splice_model(br#"{"model":"a"}"#, "q\"x").as_deref(),
        Some(&br#"{"model":"q\"x"}"#[..]),
        "the name is written as a JSON string"
    );
    assert_eq!(splice_model(br#"{"state":{"model":"jev"}}"#, "x"), None);
    assert_eq!(splice_model(br#"{"model":null}"#, "x"), None);
}

/// THE CALLER'S FIELDS REACH THE FAR END (DEC-SERVE Q2, DIALECT-FIDELITY F2): every one, in order,
/// a repeated name keeping each value, but the governed set (the document type the plane writes, the
/// hop's `host`, the credential carriers), compared without case.
#[test]
fn every_caller_field_but_the_governed_set_is_relayed() {
    let caller: [(&[u8], &[u8]); 7] = [
        (b"X-Trace", b"a"),
        (b"Authorization", b"Bearer caller-key"),
        (b"content-type", b"text/plain"),
        (b"Host", b"busbar.example"),
        (b"x-trace", b"b"),
        (b"Proxy-Authorization", b"Basic x"),
        (b"accept-language", b"en"),
    ];
    assert_eq!(
        relayed_fields(caller),
        vec![
            (b"x-trace".to_vec(), b"a".to_vec()),
            (b"x-trace".to_vec(), b"b".to_vec()),
            (b"accept-language".to_vec(), b"en".to_vec()),
        ]
    );
    assert!(relayed_fields(std::iter::empty()).is_empty());
}

/// THE ABUSE BACKSTOP (#41): an answer that would grow past the cap is refused and nothing of the
/// piece is held; up to the cap is held whole, and taken out whole for the relay.
#[test]
fn an_answer_past_the_backstop_is_refused() {
    let mut reading = FarEndReading::new();
    assert_eq!(reading.piece_within(b"12345", 8), Ok(()));
    assert_eq!(reading.piece_within(b"678", 8), Ok(()));
    assert_eq!(reading.piece_within(b"9", 8), Err(OverBackstop));
    assert_eq!(reading.take_answer(), b"12345678");
    assert!(reading.take_answer().is_empty(), "taken, not copied");
    assert_eq!(ANSWER_BACKSTOP, 256 * 1024 * 1024);
}

/// RED ($, finding 5): the owner's decisions card prices `decision`, `input_tokens` and
/// `output_tokens`, family `decision`; the tail declares all three (and the fee unit), and an
/// answer's token counts are reported under them. Predev declared the decision class alone.
#[test]
fn the_card_classes_are_declared_and_the_token_counts_reported() {
    let named: Vec<(&str, &str)> = tail::BILLABLE_CLASSES
        .iter()
        .map(|(c, f)| (c.as_str(), *f))
        .collect();
    for class in ["decision", "input_tokens", "output_tokens"] {
        assert!(
            named.contains(&(class, "decision")),
            "{class} is declared in the decision family: {named:?}"
        );
    }
    assert_eq!(
        named[tail::CLASS_INPUT_TOKENS_INDEX as usize].0,
        "input_tokens"
    );
    assert_eq!(
        named[tail::CLASS_OUTPUT_TOKENS_INDEX as usize].0,
        "output_tokens"
    );
    assert_eq!(
        settled(
            br#"{"usage":{"units":2,"input_tokens":120,"output_tokens":7}}"#,
            Some(200)
        ),
        Ok(vec![(0, 2), (1, 1), (2, 120), (3, 7)])
    );
    // Absent token counts are 0 (0 = free); only the decision count is required.
    assert_eq!(
        settled(br#"{"usage":{"units":2,"output_tokens":7}}"#, Some(200)),
        Ok(vec![(0, 2), (1, 1), (2, 0), (3, 7)])
    );
    // A token count that is present but not whole is refused, never billed as zero.
    for body in [
        &br#"{"usage":{"units":2,"input_tokens":1.5}}"#[..],
        br#"{"usage":{"units":2,"output_tokens":-3}}"#,
        br#"{"usage":{"units":2,"output_tokens":"7"}}"#,
    ] {
        assert_eq!(
            settled(body, Some(200)),
            Err(Uncounted),
            "{}",
            String::from_utf8_lossy(body)
        );
    }
}

use super::*;
use crate::codec::tests::read_raw;
use crate::meta::CLASS_DECISION;

/// [`route_named`] without the `model` the body named.
fn route_model<'m, V>(
    body: &[u8],
    models: &'m std::collections::BTreeMap<String, V>,
    serves: impl Fn(&V) -> bool,
) -> Result<(&'m str, &'m V), Unrouted> {
    route_named(body, models, serves).map(|(name, entry, _)| (name, entry))
}

/// [`project_read`] over a read of the request made here.
fn project_hook<'r, V>(
    request: &[u8],
    dialect: u32,
    routes: &'r std::collections::BTreeMap<String, V>,
    dialect_of: impl Fn(&V) -> u32,
) -> Projected<'r> {
    project_read(request, &readable(request), dialect, routes, dialect_of)
}

/// [`rewritten_read`] over a read of the body made here.
fn rewritten(body: &[u8], rewrite: &[u8], routed: &str) -> Result<Option<Vec<u8>>, NotApplied> {
    rewritten_read(body, &readable(body), rewrite, routed)
}

impl RelayedFields {
    /// Whether no field is relayed.
    fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }
}

#[test]
fn systemone_arrives_as_the_first_op_class_with_a_principal() {
    let a = arrive("POST", ops::PATH_SYSTEMONE).expect("systemone is claimed");
    assert_eq!(a.op_class, 0);
    assert_eq!(tail::OP_CLASSES[a.op_class as usize], ops::OP_SYSTEMONE);
    assert_eq!(tail::DIALECTS[a.dialect as usize], "jev");
    let e = arrive("POST", ops::PATH_EVALUATE).expect("evaluate is claimed");
    assert_eq!(tail::OP_CLASSES[e.op_class as usize], ops::OP_EVALUATE);
    assert_eq!(tail::DIALECTS[e.dialect as usize], "nanojev");
}

#[test]
fn models_is_not_claimed() {
    assert_eq!(arrive("GET", "/v1/models"), None);
    assert!(!tail::CLAIMS.iter().any(|(_, t, _)| *t == "/v1/models"));
}

#[test]
fn an_unknown_request_is_not_claimed() {
    assert_eq!(arrive("GET", ops::PATH_SYSTEMONE), None);
    assert_eq!(arrive("POST", "/v1/elsewhere"), None);
}

#[test]
fn every_claim_arrives() {
    for (verb, target, dialect) in tail::CLAIMS {
        let a = arrive(verb, target).unwrap_or_else(|| panic!("{verb} {target}"));
        assert_eq!(a.dialect, *dialect, "{verb} {target}");
        let request = attempt_request(*dialect).expect("a claimed dialect is dialled");
        assert_eq!((request.verb, request.target), (*verb, *target));
    }
}

#[test]
fn the_tail_names_the_decision_class_at_its_index() {
    let (class, family) = tail::BILLABLE_CLASSES[tail::CLASS_DECISION_INDEX as usize];
    assert_eq!(class, CLASS_DECISION);
    assert_eq!(family, "decision");
    for (index, name) in [
        (tail::CLASS_INPUT_TOKENS_INDEX, "input_tokens"),
        (tail::CLASS_OUTPUT_TOKENS_INDEX, "output_tokens"),
    ] {
        let (class, family) = tail::BILLABLE_CLASSES[index as usize];
        assert_eq!((class.as_str(), family), (name, "decision"));
    }
    assert_eq!(tail::BILLABLE_CLASSES.len(), 4);

    assert_eq!(tail::FEE_UNITS, &[busbar_contract::plane::PER_REQUEST]);
    let (fee, fee_family) = tail::BILLABLE_CLASSES[tail::CLASS_FEE_INDEX as usize];
    assert_eq!(fee.as_str(), tail::FEE_PER_REQUEST);
    assert_eq!(fee_family, tail::FEE_FAMILY);
    assert_eq!(tail::SECTION_DECLARING, crate::config::SECTION);
    assert_eq!(tail::NEEDS, &[(crate::claims::TRANSPORT, "bearer")]);
    assert_eq!(tail::DIALECT_AUTH, &[(0, "bearer"), (1, "bearer")]);
    assert_eq!(tail::DIALECTS, &["jev", "nanojev"]);
}

#[test]
fn the_caller_body_is_kept_and_an_empty_one_is_refused() {
    assert_eq!(caller_piece(b"{\"state\":{}}", true), CallerAnswer::Keep);
    assert_eq!(caller_piece(b"", false), CallerAnswer::Keep);
    assert_eq!(caller_piece(b"", true), CallerAnswer::Empty);
}

#[test]
fn a_dialect_no_claim_names_is_never_dialled() {
    let unknown = u32::try_from(tail::DIALECTS.len()).expect("few dialects");
    assert_eq!(attempt_request(unknown), None);
}

#[test]
fn an_attempt_request_is_the_dialects_head() {
    let r = attempt_request(tail::DIALECT_JEV).expect("jev is dialled");
    assert_eq!(r.verb, "POST");
    assert_eq!(r.target, "/v1/systemone");
    assert_eq!(r.fields, [("content-type", &b"application/json"[..])]);
}

/// The answer's counts as `(class, amount)`, for a far end answering `status` with `body`.
fn billed(status: u32, body: &[u8]) -> Result<Vec<(u32, u64)>, UnreadableUsage> {
    let mut reading = FarEndReading::default();
    reading.piece(body);
    reading.counts(Some(status)).map(|c| {
        c.into_iter()
            .flatten()
            .map(|u| (u.class, u.amount))
            .collect()
    })
}

const DECISION: u32 = tail::CLASS_DECISION_INDEX;
const FEE: u32 = tail::CLASS_FEE_INDEX;
const INPUT: u32 = tail::CLASS_INPUT_TOKENS_INDEX;
const OUTPUT: u32 = tail::CLASS_OUTPUT_TOKENS_INDEX;

#[test]
fn the_count_is_read_over_the_whole_answer_across_pieces() {
    let mut reading = FarEndReading::default();
    let a = br#"{"request_id":"req_1","usage":{"un"#;
    let b = br#"its":42},"answers":{"decision":"approve"}}"#;
    reading.piece(a);
    reading.piece(b);
    let counts = reading.counts(Some(200)).expect("a whole count");
    assert_eq!(
        counts[0],
        Some(Units {
            class: DECISION,
            source: Source::Reported,
            amount: 42
        })
    );
}

#[test]
fn an_error_answer_bills_nothing() {
    assert_eq!(
        billed(
            200,
            br#"{"error":{"code":"invalid_state"},"usage":{"units":3}}"#
        ),
        Ok(vec![])
    );
}

#[test]
fn a_success_bills_one_decision_or_the_count_it_reports() {
    assert_eq!(billed(200, br#"{}"#), Ok(vec![(DECISION, 1), (FEE, 1)]));
    assert_eq!(
        billed(200, br#"{"usage":{}}"#),
        Ok(vec![(DECISION, 1), (FEE, 1)])
    );
    assert_eq!(
        billed(201, br#"{"usage":{"units":3}}"#),
        Ok(vec![(DECISION, 3), (FEE, 1)])
    );
    assert_eq!(
        billed(299, br#"{"usage":{"units":3}}"#),
        Ok(vec![(DECISION, 3), (FEE, 1)])
    );
    assert_eq!(
        billed(200, br#"{"usage":{"units":3},"error":null}"#),
        Ok(vec![(DECISION, 3), (FEE, 1)]),
        "a null error is no error"
    );
    let max = format!(r#"{{"usage":{{"units":{}}}}}"#, i64::MAX / 1_000_000);
    assert_eq!(
        billed(200, max.as_bytes()),
        Ok(vec![(DECISION, (i64::MAX / 1_000_000) as u64), (FEE, 1)])
    );
    // A stated zero is a reported count: not the one-decision floor, and not nothing.
    assert_eq!(
        billed(200, br#"{"usage":{"units":0}}"#),
        Ok(vec![(DECISION, 0), (FEE, 1)])
    );
}

#[test]
fn a_non_success_bills_nothing() {
    for status in [199, 300, 301, 304, 400, 422, 500, 503] {
        assert_eq!(
            billed(status, br#"{"usage":{"units":2}}"#),
            Ok(vec![]),
            "{status}"
        );
    }
    let mut reading = FarEndReading::default();
    reading.piece(br#"{"usage":{"units":2}}"#);
    assert_eq!(
        reading.counts(None),
        Ok([None; 4]),
        "no status read, no bill"
    );
}

#[test]
fn an_unrepresentable_usage_member_fails_loudly() {
    for units in [
        "-1".to_string(),
        "0.0000001".to_string(),
        "9223372036854.775808".to_string(),
        "9223372036855".to_string(),
        "\"7\"".to_string(),
        "{}".to_string(),
        (i64::MAX as u64 + 1).to_string(),
        u64::MAX.to_string(),
    ] {
        let body = format!(r#"{{"usage":{{"units":{units}}}}}"#);
        assert_eq!(
            billed(200, body.as_bytes()),
            Err(UnreadableUsage(crate::codec::PTR_USAGE_UNITS)),
            "{body}"
        );
    }
    assert_eq!(
        billed(200, br#"{"usage":{"units":1,"output_tokens":-2}}"#),
        Err(UnreadableUsage(crate::codec::PTR_USAGE_OUTPUT_TOKENS))
    );
}

#[test]
fn a_null_usage_count_is_absent_not_unreadable() {
    // null at a non-required count is absent: it bills the floor, not refused.
    assert_eq!(
        billed(200, br#"{"usage":{"units":null}}"#),
        Ok(vec![(DECISION, 1), (FEE, 1)])
    );
    // null tokens with a present decision count: the decision count is reported, tokens absent.
    assert_eq!(
        billed(
            200,
            br#"{"usage":{"units":3,"input_tokens":null,"output_tokens":null}}"#
        ),
        Ok(vec![(DECISION, 3), (FEE, 1)])
    );
    // null tokens with an absent decision count: floor of one decision.
    assert_eq!(
        billed(
            200,
            br#"{"usage":{"input_tokens":null,"output_tokens":null}}"#
        ),
        Ok(vec![(DECISION, 1), (FEE, 1)])
    );
}

#[test]
fn floor_and_reported_sources_are_distinguished() {
    // An absent /usage/units is Floor, never Reported.
    let mut reading = FarEndReading::default();
    reading.piece(br#"{"usage":{}}"#);
    let floor = reading.counts(Some(200)).unwrap()[0].unwrap();
    assert_eq!(floor.source, Source::Floor, "absent units must be Floor");
    assert_eq!(floor.amount, 1);

    // A null /usage/units is also Floor (absent, not unreadable).
    let mut reading = FarEndReading::default();
    reading.piece(br#"{"usage":{"units":null}}"#);
    let floor = reading.counts(Some(200)).unwrap()[0].unwrap();
    assert_eq!(floor.source, Source::Floor, "null units must be Floor");
    assert_eq!(floor.amount, 1);

    // An explicit /usage/units of 1 is Reported, not Floor.
    let mut reading = FarEndReading::default();
    reading.piece(br#"{"usage":{"units":1}}"#);
    let reported = reading.counts(Some(200)).unwrap()[0].unwrap();
    assert_eq!(
        reported.source,
        Source::Reported,
        "explicit units:1 must be Reported"
    );
    assert_eq!(reported.amount, 1);

    // The fee unit is always Reported.
    let mut reading = FarEndReading::default();
    reading.piece(br#"{"usage":{"units":5}}"#);
    let fee = reading.counts(Some(200)).unwrap()[3].unwrap();
    assert_eq!(fee.source, Source::Reported);
    assert_eq!(fee.amount, 1);
}

#[test]
fn a_success_bills_the_tokens_it_reports() {
    assert_eq!(
        billed(
            200,
            br#"{"usage":{"units":2,"input_tokens":12,"output_tokens":5}}"#
        ),
        Ok(vec![(DECISION, 2), (INPUT, 12), (OUTPUT, 5), (FEE, 1)])
    );
    assert_eq!(
        billed(200, br#"{"usage":{"input_tokens":9}}"#),
        Ok(vec![(DECISION, 1), (INPUT, 9), (FEE, 1)])
    );
    assert_eq!(
        billed(200, br#"{"usage":{"input_tokens":0,"output_tokens":0}}"#),
        Ok(vec![(DECISION, 1), (INPUT, 0), (OUTPUT, 0), (FEE, 1)]),
        "a stated zero token count is reported, not absent"
    );
}

#[test]
fn the_counts_never_read_state_or_answers() {
    assert_eq!(
        billed(
            200,
            br#"{"usage":{"units":7},"state":{"usage":{"units":999}},"answers":{"units":888}}"#
        ),
        Ok(vec![(DECISION, 7), (FEE, 1)])
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
        br#"{"error":{"code":"internal","message":"x"}}"#
    );
    assert_eq!(
        refusal_body(429, "x"),
        br#"{"error":{"code":"rate_limit","message":"x"}}"#
    );
    assert_eq!(
        refusal_body(422, "x"),
        br#"{"error":{"code":"invalid_request","message":"x"}}"#
    );
}

#[test]
fn one_or_more_models_mount_the_claim() {
    let systemone = ("POST", ops::PATH_SYSTEMONE, tail::DIALECT_JEV);
    let evaluate = ("POST", ops::PATH_EVALUATE, tail::DIALECT_NANOJEV);
    assert!(served_claims(&[]).is_empty());
    assert_eq!(
        served_claims(&[0]),
        vec![systemone, ("POST", crate::classification::PATH, 0)]
    );
    assert_eq!(
        served_claims(&[0, 0]),
        vec![systemone, ("POST", crate::classification::PATH, 0)]
    );
    assert_eq!(
        served_claims(&[1]),
        vec![evaluate, ("POST", crate::classification::PATH, 1)]
    );
    assert_eq!(
        served_claims(&[1, 0, 1]),
        vec![
            systemone,
            evaluate,
            ("POST", crate::classification::PATH, 1)
        ]
    );
    assert_eq!(dialect_of("nanojev"), Some(tail::DIALECT_NANOJEV));
    assert_eq!(dialect_of("anthropic"), None);
}

fn models(names: &[&str]) -> std::collections::BTreeMap<String, ()> {
    names.iter().map(|n| ((*n).to_string(), ())).collect()
}

#[test]
fn a_request_routes_over_the_model_it_names() {
    let two = models(&["a", "b"]);
    assert_eq!(
        route_model(br#"{"model":"b"}"#, &two, |_| true).map(|r| r.0),
        Ok("b")
    );
    assert_eq!(
        route_model(br#"{"model":"\u0061"}"#, &two, |_| true).map(|r| r.0),
        Ok("a"),
        "an escaped name is the name it spells"
    );
    assert_eq!(
        route_model(br#"{"state":{}}"#, &two, |_| true),
        Err(Unrouted::Unnamed)
    );
    assert_eq!(
        route_model(br#"{"model":"zz"}"#, &two, |_| true),
        Err(Unrouted::Unknown("zz".to_string()))
    );
    assert_eq!(
        route_model(br#"{"model":null}"#, &two, |_| true),
        Err(Unrouted::NotAName)
    );
    assert_eq!(
        route_model(br#"{"model":["a"]}"#, &two, |_| true),
        Err(Unrouted::NotAName)
    );

    assert_eq!(
        route_model(br#"{"state":{"model":"a"}}"#, &two, |_| true),
        Err(Unrouted::Unnamed)
    );

    let one = models(&["jev"]);
    assert_eq!(
        route_model(br#"{"state":{}}"#, &one, |_| true).map(|r| r.0),
        Ok("jev")
    );

    assert_eq!(
        route_model(b"not json", &one, |_| true).map(|r| r.0),
        Err(Unrouted::Unparsable)
    );
    assert_eq!(
        route_model(br#"{"model":"other"}"#, &one, |_| true),
        Err(Unrouted::Unknown("other".to_string()))
    );
    assert_eq!(
        route_model(br#"{}"#, &models(&[]), |_| true),
        Err(Unrouted::NoModel)
    );

    assert_eq!(
        (Unrouted::Unnamed.status(), Unrouted::NotAName.status()),
        (400, 400)
    );
    assert_eq!(Unrouted::Unknown("zz".to_string()).status(), 404);
}

#[test]
fn the_splice_rewrites_the_top_level_model_value_alone() {
    let body = br#"{"model" : "jev", "state":{"model":"jev"}}"#;
    assert_eq!(
        spliced(body, "jev-1.13.0").as_deref(),
        Some(&br#"{"model" : "jev-1.13.0", "state":{"model":"jev"}}"#[..])
    );
    assert_eq!(spliced(body, "jev"), None, "the same name passes through");
    assert_eq!(
        spliced(br#"{"state":{}}"#, "x"),
        None,
        "no model to rewrite"
    );
    assert_eq!(spliced(br#"{"model":3}"#, "x"), None, "not a name");
    assert_eq!(
        spliced(br#"{"model":"a"}"#, "q\"x").as_deref(),
        Some(&br#"{"model":"q\"x"}"#[..]),
        "the value is written as a JSON string"
    );
}

/// BILLABLE SUCCESS ONLY: the request fee is counted once for a far end's 2xx answer with no
/// `/error` member, and never for an error answer, a non-2xx status or an answer with no status.
#[test]
fn the_request_fee_is_counted_on_a_success_alone() {
    let mut ok = FarEndReading::default();
    ok.piece(br#"{"usage":{"units":3}}"#);
    let fee = ok.counts(Some(200)).expect("a success")[3].expect("a success owes its fee");
    assert_eq!(
        (fee.class, fee.amount, fee.source),
        (FEE, 1, Source::Reported)
    );

    assert_eq!(
        ok.counts(Some(422)),
        Ok([None; 4]),
        "a refused request owes none"
    );
    assert_eq!(ok.counts(Some(500)), Ok([None; 4]));
    assert_eq!(ok.counts(None), Ok([None; 4]), "no status read, no fee");
    assert_eq!(
        billed(200, br#"{"error":{"code":"x","message":"y"}}"#),
        Ok(vec![]),
        "an error answer owes none"
    );
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
    let kept = relayed_fields(caller);
    assert_eq!(
        kept.iter()
            .map(|(n, v)| (n.to_vec(), v.to_vec()))
            .collect::<Vec<_>>(),
        vec![
            (b"x-trace".to_vec(), b"a".to_vec()),
            (b"x-trace".to_vec(), b"b".to_vec()),
            (b"accept-language".to_vec(), b"en".to_vec()),
        ]
    );
    assert!(relayed_fields(std::iter::empty()).is_empty());
}

#[test]
fn red_no_caller_credential_carrier_is_relayed() {
    let canaries: [(&[u8], &[u8]); 8] = [
        (b"x-api-key", b"CANARY-x-api-key"),
        (b"Api-Key", b"CANARY-api-key"),
        (b"X-Goog-Api-Key", b"CANARY-x-goog-api-key"),
        (b"Cookie", b"session=CANARY-cookie"),
        (b"x-amz-security-token", b"CANARY-x-amz-security-token"),
        (b"Authorization", b"Bearer CANARY-authorization"),
        (b"X-Trace", b"kept"),
        (b"set-cookie", b"CANARY-set-cookie"),
    ];
    let kept = relayed_fields(canaries);
    for (name, value) in kept.iter() {
        assert!(
            !value.windows(6).any(|w| w == b"CANARY"),
            "the caller's `{}` crossed to the far end",
            String::from_utf8_lossy(name)
        );
    }
    assert_eq!(
        kept.iter()
            .map(|(n, v)| (n.to_vec(), v.to_vec()))
            .collect::<Vec<_>>(),
        vec![(b"x-trace".to_vec(), b"kept".to_vec())]
    );
    for carrier in busbar_contract::abi::host::conn::connector::NEVER_KEPT {
        let field: [(&[u8], &[u8]); 1] = [(carrier.as_bytes(), b"CANARY")];
        assert!(relayed_fields(field).is_empty(), "`{carrier}` is relayed");
    }
}

/// The money rule's reading of a whole answer by its declared pointers, as the body would be read
/// kept whole: the reference the piece-by-piece reading is held to.
fn by_pointer(status: u32, body: &[u8]) -> Result<Vec<(u32, u64)>, UnreadableUsage> {
    let ok = (200..300).contains(&status) && read_raw(body, "/error").is_none_or(|e| e == b"null");
    if !ok {
        return Ok(vec![]);
    }

    if !body.trim_ascii_start().starts_with(b"{") {
        return Err(UnreadableUsage(crate::codec::PTR_USAGE_UNITS));
    }
    let count = |p: &'static str| match read_raw(body, p) {
        None => Ok(None),
        Some(raw) => count(raw)
            .map(|n| Some(reported(0, n).amount))
            .ok_or(UnreadableUsage(p)),
    };
    let mut out = vec![(DECISION, count(crate::codec::PTR_USAGE_UNITS)?.unwrap_or(1))];
    if let Some(n) = count(crate::codec::PTR_USAGE_INPUT_TOKENS)? {
        out.push((INPUT, n));
    }
    if let Some(n) = count(crate::codec::PTR_USAGE_OUTPUT_TOKENS)? {
        out.push((OUTPUT, n));
    }
    out.push((FEE, 1));
    Ok(out)
}

/// The answer read in two pieces split at `at`.
fn split_billed(status: u32, body: &[u8], at: usize) -> Result<Vec<(u32, u64)>, UnreadableUsage> {
    let mut reading = FarEndReading::default();
    reading.piece(&body[..at]);
    reading.piece(&body[at..]);
    reading.counts(Some(status)).map(|c| {
        c.into_iter()
            .flatten()
            .map(|u| (u.class, u.amount))
            .collect()
    })
}

#[test]
fn the_reading_is_the_pointers_reading_wherever_the_pieces_split() {
    let answers: [&[u8]; 14] = [
        br#"{"request_id":"req_1","usage":{"units":42},"answers":{"decision":"approve"}}"#,
        br#" { "usage" : { "input_tokens" : 12 , "units" : 3, "output_tokens":5 } } "#,
        br#"{"answers":{"usage":{"units":9}},"usage":{"units":2}}"#,
        br#"{"state":{"error":"x","usage":{"units":"q"}},"usage":{}}"#,
        br#"{"error":null,"usage":{"units":7}}"#,
        br#"{"error":{"code":"x","message":"y"},"usage":{"units":3}}"#,
        br#"{"error":"down"}"#,
        br#"{"usage":{"units":7.5}}"#,
        br#"{"usage":{"units":"7"}}"#,
        br#"{"usage":{"units":[1]}}"#,
        br#"{"usage":{"units":99999999999999999999999999}}"#,
        br#"{"a\"usage":{"units":"no"},"usage":{"output_tokens":-1}}"#,
        br#"{"usage":"n/a","x":[{"usage":{"units":7}}]}"#,
        br#"[{"usage":{"units":5}}]"#,
    ];
    for body in answers {
        for status in [200, 500] {
            let want = by_pointer(status, body);
            for at in 0..=body.len() {
                assert_eq!(
                    split_billed(status, body, at),
                    want,
                    "{status} {} split at {at}",
                    String::from_utf8_lossy(body)
                );
            }
        }
    }
}

/// Malformed bodies give the same result at every split point: the streaming reader must not
/// change its reading depending on where a piece boundary falls in broken JSON.
#[test]
fn malformed_bodies_are_split_consistent() {
    let malformed: &[&[u8]] = &[
        // Truncated mid-value: no closing brace.
        br#"{"usage":{"units":7"#,
        // Truncated inside a string.
        br#"{"error":"trunc"#,
        // Garbage after the opening brace.
        br#"{"usage":{"units":3},,}"#,
        // Array root (not an object): the reader rejects it.
        br#"[{"usage":{"units":5}}]"#,
    ];
    for body in malformed {
        for status in [200, 500] {
            // The whole-body reading is the reference for every split.
            let mut whole = FarEndReading::default();
            whole.piece(body);
            let want = whole.counts(Some(status));
            for at in 0..=body.len() {
                let mut reading = FarEndReading::default();
                reading.piece(&body[..at]);
                reading.piece(&body[at..]);
                assert_eq!(
                    reading.counts(Some(status)),
                    want,
                    "{status} {} split at {at}",
                    String::from_utf8_lossy(body)
                );
            }
        }
    }
}

/// An answer of any size is read whole, its count found past a megabyte of other bytes. That the
/// reading keeps nothing of it (no allocation, however long the answer runs) is held by
/// `tests/door_alloc.rs`, under a counting allocator.
#[test]
fn a_large_answer_is_read_without_keeping_it() {
    let mut reading = FarEndReading::default();
    reading.piece(br#"{"answers":{"blob":""#);
    let filler = [b'x'; 4096];
    for _ in 0..256 {
        reading.piece(&filler);
    }
    reading.piece(br#""},"usage":{"units":4}}"#);
    assert_eq!(
        reading.counts(Some(200)).map(|c| c[0].map(|u| u.amount)),
        Ok(Some(4))
    );
}

/// The counts of a `nanojev` answer, `(class, amount)`.
fn nanojev_billed(status: u32, body: &[u8]) -> Result<Vec<(u32, u64)>, UnreadableUsage> {
    let mut reading = FarEndReading::for_dialect(tail::DIALECT_NANOJEV);
    reading.piece(body);
    reading.counts(Some(status)).map(|c| {
        c.into_iter()
            .flatten()
            .map(|u| (u.class, u.amount))
            .collect()
    })
}

#[test]
fn a_nanojev_answer_bills_its_execution_states() {
    assert_eq!(
        nanojev_billed(200, br#"{"execution":{"states":3},"result":{}}"#),
        Ok(vec![(DECISION, 3), (FEE, 1)])
    );
    assert_eq!(
        nanojev_billed(
            200,
            br#"{"usage":{"units":9,"input_tokens":4},"execution":{"states":1}}"#
        ),
        Ok(vec![(DECISION, 1), (FEE, 1)]),
        "jev's usage members are not nanojev's"
    );
    for missing in [
        &br#"{"result":{}}"#[..],
        br#"{"execution":{}}"#,
        br#"{"usage":{"units":2}}"#,
    ] {
        assert_eq!(
            nanojev_billed(200, missing),
            Err(UnreadableUsage(crate::codec::PTR_EXECUTION_STATES)),
            "{}",
            String::from_utf8_lossy(missing)
        );
    }
    assert_eq!(
        nanojev_billed(200, br#"{"execution":{"states":2.5}}"#),
        Ok(vec![(DECISION, 2_500_000), (FEE, 1)])
    );
    assert_eq!(
        nanojev_billed(500, br#"{"result":{}}"#),
        Ok(vec![]),
        "a failure bills nothing"
    );

    let body = br#"{"execution" : {"trace":[{"states":99}], "states" : 12}}"#;
    for at in 0..=body.len() {
        let mut reading = FarEndReading::for_dialect(tail::DIALECT_NANOJEV);
        reading.piece(&body[..at]);
        reading.piece(&body[at..]);
        assert_eq!(
            reading.counts(Some(200)).map(|c| c[0].map(|u| u.amount)),
            Ok(Some(12)),
            "split at {at}"
        );
    }
}

#[test]
fn a_nanojev_200_with_error_bills_nothing() {
    // A 200 whose body carries `/error` (not null) is not a success: bills nothing.
    assert_eq!(
        nanojev_billed(
            200,
            br#"{"error":{"code":"x","message":"y"},"execution":{"states":3}}"#
        ),
        Ok(vec![]),
        "a 200 with a non-null /error must not bill"
    );
    assert_eq!(
        nanojev_billed(200, br#"{"error":"down","execution":{"states":5}}"#),
        Ok(vec![]),
        "a string /error at 200 must not bill"
    );
    // A null /error is not an error: it bills normally.
    assert_eq!(
        nanojev_billed(200, br#"{"error":null,"execution":{"states":2}}"#),
        Ok(vec![(DECISION, 2), (FEE, 1)]),
        "a null /error at 200 must still bill"
    );
    // Non-200 non-500 statuses also bill nothing.
    assert_eq!(
        nanojev_billed(429, br#"{"execution":{"states":1}}"#),
        Ok(vec![]),
        "a 429 must not bill"
    );
    // Split-equivalence for nanojev error-at-200.
    let body = br#"{"error":{"code":"x"},"execution":{"states":3}}"#;
    for at in 0..=body.len() {
        let mut reading = FarEndReading::for_dialect(tail::DIALECT_NANOJEV);
        reading.piece(&body[..at]);
        reading.piece(&body[at..]);
        assert_eq!(
            reading.counts(Some(200)),
            Ok([None; 4]),
            "nanojev error-at-200 split at {at}"
        );
    }
}

#[test]
fn a_model_of_the_other_dialect_is_this_dialects_unknown_model() {
    let models: std::collections::BTreeMap<String, u32> =
        [("j".to_string(), 0), ("n".to_string(), 1)].into();
    let jev = |d: &u32| *d == tail::DIALECT_JEV;
    let nano = |d: &u32| *d == tail::DIALECT_NANOJEV;
    assert_eq!(
        route_model(br#"{"model":"n"}"#, &models, jev),
        Err(Unrouted::Unknown("n".to_string()))
    );
    assert_eq!(
        route_model(br#"{"model":"n"}"#, &models, nano).map(|r| r.0),
        Ok("n")
    );
    assert_eq!(route_model(br#"{}"#, &models, jev).map(|r| r.0), Ok("j"));
    assert_eq!(route_model(br#"{}"#, &models, nano).map(|r| r.0), Ok("n"));
}

#[test]
fn a_success_with_no_count_bills_the_planes_floor_not_a_far_end_count() {
    let mut none = FarEndReading::default();
    none.piece(br#"{"answers":{}}"#);
    let counts = none.counts(Some(200)).expect("a success");
    assert_eq!(
        counts[0],
        Some(Units {
            class: DECISION,
            source: Source::Floor,
            amount: 1
        })
    );
    assert_eq!(counts[3].map(|f| f.source), Some(Source::Reported));
    let mut stated = FarEndReading::default();
    stated.piece(br#"{"usage":{"units":5}}"#);
    let counts = stated.counts(Some(200)).expect("a success");
    assert_eq!(counts[0].map(|d| d.source), Some(Source::Reported));
}

#[test]
fn an_unknown_models_echoed_name_is_bounded() {
    let models = std::collections::BTreeMap::from([("jev".to_string(), ())]);
    let short = route_model(br#"{"model":"zz"}"#, &models, |_| true).expect_err("unknown");
    assert_eq!(
        short.text(),
        "The model 'zz' does not exist or you do not have access to it."
    );

    let kilobytes = "k".repeat(64 * 1024);
    let body = format!("{{\"model\":\"{kilobytes}\"}}");
    let whole = route_model(body.as_bytes(), &models, |_| true).expect_err("unknown");
    assert_eq!(
        whole.text(),
        format!("The model '{kilobytes}' does not exist or you do not have access to it.")
    );
    let long = "é".repeat(600_000);
    let body = format!("{{\"model\":\"{long}\"}}");
    let unknown = route_model(body.as_bytes(), &models, |_| true).expect_err("unknown");
    assert_eq!(unknown.status(), 404);
    let text = unknown.text();
    let cap = busbar_contract::abi::plane::MAX_REFUSAL_TEXT as usize;
    assert!(
        text.len() <= cap && text.len() > cap - 8,
        "the refusal echoes {} bytes",
        text.len()
    );
    assert!(
        text.starts_with("The model 'éé") && text.contains("...' does not exist"),
        "{}",
        text.chars().take(40).collect::<String>()
    );
    let exact = "m".repeat(MAX_ECHOED_MODEL);
    assert_eq!(echoed(&exact), exact, "a name at the bound is echoed whole");
    assert_eq!(
        Unrouted::Unknown(exact).text().len(),
        cap,
        "and its refusal runs exactly the cap"
    );
    let over = "m".repeat(MAX_ECHOED_MODEL + 1);
    assert!(echoed(&over).ends_with("...") && echoed(&over).len() == MAX_ECHOED_MODEL);
}

#[test]
fn a_target_names_its_claims_dialect() {
    assert_eq!(dialect_at("/v1/systemone"), Some(tail::DIALECT_JEV));
    assert_eq!(dialect_at("/v1/systemone?x=1"), Some(tail::DIALECT_JEV));
    assert_eq!(dialect_at("/api/evaluate"), Some(tail::DIALECT_NANOJEV));
    assert_eq!(dialect_at("/v1/models"), None);
}

/// A hook's rewrite envelope as the kernel hands it: one message whose content is
/// `document`.
fn rewrite_of(document: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "messages": [{ "role": "user", "content": document }],
        "tools": [],
    }))
    .expect("JSON")
}

/// THE REWRITE REPLACES THE REQUEST WHOLESALE: the members the document
/// states, the caller's own bytes kept where a value is unchanged, the caller's `model` kept, and a
/// member the caller did not send added.
#[test]
fn a_rewrite_replaces_the_request_with_its_document() {
    let body = b"{ \"state\" : {\"amount\": 7},\n  \"model\":\"m\", \"note\": [1, 2] }";
    let out = rewritten(
        body,
        &rewrite_of(r#"{"state":{"amount":0},"note":[1,2]}"#),
        "m",
    )
    .expect("the model is the routed one")
    .expect("applied");
    assert_eq!(
        String::from_utf8(out).expect("UTF-8"),
        r#"{"state":{"amount":0},"model":"m","note":[1, 2]}"#
    );
    let added = rewritten(
        br#"{"state":{}}"#,
        &rewrite_of(r#"{"state":{},"context":{"k":1}}"#),
        "m",
    )
    .expect("no model named")
    .expect("applied");
    assert_eq!(added, br#"{"state":{},"context":{"k":1}}"#);
    let into_empty = rewritten(b"{}", &rewrite_of(r#"{"state":1}"#), "m")
        .expect("no model named")
        .expect("applied");
    assert_eq!(into_empty, br#"{"state":1}"#);

    let bare = br#"{"messages":[{"state":2}],"tools":[]}"#;
    assert_eq!(
        rewritten(br#"{"state":1}"#, bare, "m")
            .expect("no model named")
            .expect("applied"),
        br#"{"state":2}"#
    );
}

#[test]
fn a_member_the_rewrite_omits_is_dropped() {
    let body = br#"{"model":"m","state":{"amount":7},"pii":{"ssn":"123-45-6789"}}"#;
    let out = rewritten(body, &rewrite_of(r#"{"state":{"amount":7}}"#), "m")
        .expect("no other model named")
        .expect("applied");
    assert_eq!(out, br#"{"model":"m","state":{"amount":7}}"#);
    assert!(!String::from_utf8_lossy(&out).contains("123-45-6789"));
}

/// A nested member name that repeats is a value the hook never saw (it read the last one): the
/// caller's own bytes for that member never reach the far end, whether the document is unchanged
/// or changes another member.
#[test]
fn a_nested_duplicate_the_hook_never_saw_is_not_relayed() {
    let body = br#"{"model":"m","state":{"note":"SECRET-1","note":"ok"},"x":1}"#;
    let unchanged = rewritten(body, &rewrite_of(r#"{"state":{"note":"ok"},"x":1}"#), "m")
        .expect("the routed model")
        .expect("applied");
    assert!(
        !String::from_utf8_lossy(&unchanged).contains("SECRET-1"),
        "{}",
        String::from_utf8_lossy(&unchanged)
    );
    assert_eq!(unchanged, br#"{"model":"m","state":{"note":"ok"},"x":1}"#);
    let changed = rewritten(body, &rewrite_of(r#"{"state":{"note":"ok"},"x":2}"#), "m")
        .expect("the routed model")
        .expect("applied");
    assert_eq!(changed, br#"{"model":"m","state":{"note":"ok"},"x":2}"#);
    // An escaped spelling of the same name is the same name.
    let escaped = br#"{"model":"m","state":[{"n\u006fte":"SECRET-2","note":"ok"}]}"#;
    let out = rewritten(escaped, &rewrite_of(r#"{"state":[{"note":"ok"}]}"#), "m")
        .expect("the routed model")
        .expect("applied");
    assert_eq!(out, br#"{"model":"m","state":[{"note":"ok"}]}"#);
    // Without a repeat the caller's own bytes are kept as before.
    let plain = br#"{"model":"m","state":{"note" : "ok"},"x":1}"#;
    let kept = rewritten(plain, &rewrite_of(r#"{"state":{"note":"ok"},"x":2}"#), "m")
        .expect("the routed model")
        .expect("applied");
    assert_eq!(kept, br#"{"model":"m","state":{"note" : "ok"},"x":2}"#);
}

/// A rewrite whose `model` is no string is refused without its value being named: the refusal is
/// logged, and the value may hold decision content.
#[test]
fn a_rewrite_naming_a_non_string_model_does_not_state_it() {
    let body = br#"{"model":"m","state":{}}"#;
    let refused = rewritten(
        body,
        &rewrite_of(r#"{"model":{"ssn":"SECRET-3"},"state":{}}"#),
        "m",
    );
    let Err(NotApplied::Model(ModelRewritten { named })) = refused else {
        panic!("a non-string model is refused: {refused:?}");
    };
    assert!(!named.contains("SECRET-3"), "{named}");
}

#[test]
fn a_rewrite_naming_another_model_is_refused() {
    let body = br#"{"model":"m","state":{}}"#;
    assert_eq!(
        rewritten(body, &rewrite_of(r#"{"model":"other","state":{}}"#), "m"),
        Err(NotApplied::Model(ModelRewritten {
            named: "other".to_string()
        }))
    );
    let same = rewritten(body, &rewrite_of(r#"{"model":"m","state":{"a":1}}"#), "m")
        .expect("the routed model")
        .expect("applied");
    assert_eq!(same, br#"{"model":"m","state":{"a":1}}"#);

    assert_eq!(
        rewritten(
            br#"{"state":{}}"#,
            &rewrite_of(r#"{"model":"m","state":{}}"#),
            "m"
        ),
        Ok(Some(br#"{"state":{}}"#.to_vec()))
    );
}

#[test]
fn a_rewrite_applies_over_a_body_nested_past_the_span_scanners_depth() {
    let deep = format!("{}{}", "[".repeat(70), "]".repeat(70));
    let body = format!(r#"{{"model":"m","deep":{deep},"pii":"123-45-6789","state":{{}}}}"#);
    let out = rewritten(body.as_bytes(), &rewrite_of(r#"{"state":{}}"#), "m")
        .expect("no other model named")
        .expect("the hook's rewrite applies");
    let out: serde_json::Value = serde_json::from_slice(&out).expect("JSON");
    assert_eq!(out, serde_json::json!({"model": "m", "state": {}}));
}

/// A rewrite whose document is the request as it stands is an applied identity: the kernel refuses a
/// rewrite that answers no replacement, so the same bytes are answered, not `None`.
#[test]
fn an_identity_rewrite_answers_the_request_bytes() {
    let body = br#"{"state":{"amount":7}}"#;
    assert_eq!(
        rewritten(body, &rewrite_of(r#"{"state":{"amount":7}}"#), "m"),
        Ok(Some(body.to_vec()))
    );
    let with_model = br#"{ "model":"m", "state":{} }"#;
    assert_eq!(
        rewritten(with_model, &rewrite_of(r#"{"state":{}}"#), "m"),
        Ok(Some(with_model.to_vec()))
    );
}

/// A rewrite that names no document, or meets a body that is no object, leaves the request as it was
/// The kernel refuses it, which fails closed.
#[test]
fn a_rewrite_that_does_not_apply_leaves_the_request() {
    let body = br#"{"state":{"amount":7}}"#;
    for rewrite in [
        br#"{"messages":[],"tools":[]}"#,
        &rewrite_of("not a document")[..],
        b"not json",
    ] {
        assert_eq!(rewritten(body, rewrite, "m"), Ok(None));
    }
    assert_eq!(
        rewritten(b"[1,2]", &rewrite_of(r#"{"state":1}"#), "m"),
        Ok(None)
    );
}

/// The projection names the entry the request routes over among its dialect's models, the
/// dialect's name and the document's characters.
#[test]
fn a_request_is_projected_over_its_dialects_models() {
    let routes = std::collections::BTreeMap::from([
        ("a".to_string(), tail::DIALECT_JEV),
        ("n".to_string(), tail::DIALECT_NANOJEV),
    ]);
    let body = r#"{"model":"a","state":"é"}"#.as_bytes();
    let view = project_hook(body, tail::DIALECT_JEV, &routes, |d| *d);
    assert_eq!(
        view,
        Projected {
            entry: "a",
            dialect: "jev",
            chars: body.len() - 1,
        }
    );

    assert_eq!(
        project_hook(b"{}", tail::DIALECT_NANOJEV, &routes, |d| *d).entry,
        "n"
    );
    assert_eq!(
        project_hook(br#"{"model":"n"}"#, tail::DIALECT_JEV, &routes, |d| *d).entry,
        ""
    );
}

/// Content names never select a different JSON parser or numeric range.
#[test]
fn opaque_numbers_are_valid_under_every_content_key() {
    for number in [
        "1e309",
        "-1e400",
        "18446744073709551617",
        "27.0",
        "-0",
        "1e99999999999999999999",
    ] {
        assert_eq!(readable(number.as_bytes()), Ok(None));
        for key in [
            "x",
            "system",
            "stream",
            "stream_options",
            "__busbar_gemini_json_array",
        ] {
            let body = format!(r#"{{"model":"m","{key}":[{number}]}}"#);
            assert_eq!(readable(body.as_bytes()), Ok(Some(serde_json::json!("m"))));
            assert!(valid(body.as_bytes()));
            assert!(spliceable_named(body.as_bytes(), Some("m"), "upstream"));
            let expected = body.replacen(r#""model":"m""#, r#""model":"upstream""#, 1);
            assert_eq!(
                spliced(body.as_bytes(), "upstream"),
                Some(expected.into_bytes())
            );
        }
    }
}

#[test]
fn invalid_strings_are_refused_in_every_position() {
    for value in [
        r#""\uZZZZ""#,
        r#""\u12G4""#,
        r#""\ud800""#,
        r#""\udc00""#,
        r#""\udc00\ud800""#,
        r#""\u"   ""#,
    ] {
        for body in [
            value.to_string(),
            format!("[{value}]"),
            format!(r#"{{"model":"m","x":{value}}}"#),
            format!(r#"{{"model":"m","system":{value}}}"#),
            format!(r#"{{"model":"m","x":{{{value}:1}}}}"#),
        ] {
            assert_eq!(
                readable(body.as_bytes()),
                Err(Unrouted::Unparsable),
                "{body}"
            );
            assert!(!valid(body.as_bytes()), "{body}");
            assert!(parsed(body.as_bytes()).is_err(), "{body}");
        }
    }
    for body in [
        br#"{"model":"m","x":"\u00e9\ud83d\ude00"}"#.as_slice(),
        br#"["\"\\\/\b\f\n\r\t"]"#,
    ] {
        assert!(readable(body).is_ok());
        assert!(valid(body));
    }
}

#[test]
fn numeric_grammar_and_depth_still_bound_opaque_documents() {
    for number in [
        "01", "-01", "1.", "1e", "1e+", "+1", "NaN", "Infinity", "--1",
    ] {
        let body = format!(r#"{{"model":"m","x":{number}}}"#);
        assert_eq!(readable(body.as_bytes()), Err(Unrouted::Unparsable));
    }
    for depth in [127, 128, 200, 1000] {
        let body = format!(
            r#"{{"model":"m","x":{}{}}}"#,
            "[".repeat(depth),
            "]".repeat(depth)
        );
        assert_eq!(readable(body.as_bytes()).is_ok(), depth < MAX_PARSE_DEPTH);
    }
    let hidden = format!(
        r#"{{"model":"m","x":"\uAAA\","y":{}{}}}"#,
        "[".repeat(200),
        "]".repeat(200)
    );
    assert_eq!(readable(hidden.as_bytes()), Err(Unrouted::Unparsable));
}

#[test]
fn rewrites_preserve_untouched_large_numbers_exactly() {
    let mantissa = format!("0.{}1", "0".repeat(2000));
    for (exponent, stored_exponent) in [
        ("e12300", "e+12300"),
        ("E12300", "e+12300"),
        ("e+12300", "e+12300"),
        ("e-12300", "e-12300"),
    ] {
        let number = format!("{mantissa}{exponent}");
        let body = format!(r#"{{"model":"m","x":{number},"pii":"secret","state":{{}}}}"#);
        let document = parsed(body.as_bytes()).expect("valid exact document");
        // The DOM preserves every digit but normalizes exponent syntax. The rewrite must
        // preserve the caller's original bytes, including that syntax, for untouched values.
        assert_eq!(
            document["x"].to_string(),
            format!("{mantissa}{stored_exponent}")
        );
        let replacement = format!(r#"{{"x":{number},"state":{{}}}}"#);
        let actual = rewritten(body.as_bytes(), &rewrite_of(&replacement), "m")
            .expect("applied")
            .expect("redacted");
        let expected = format!(r#"{{"model":"m","x":{number},"state":{{}}}}"#);
        assert_eq!(actual, expected.as_bytes());
    }
}

#[test]
fn marker_objects_survive_actual_parse_and_serialize() {
    for (raw, expected) in [
        (
            r#"{"$serde_json::private::Number":"27"}"#,
            r#"{"$serde_json::private::Number":"27"}"#,
        ),
        (
            r#"{"\u0024serde_json::private::Number":"27"}"#,
            r#"{"$serde_json::private::Number":"27"}"#,
        ),
        (
            r#"{"$serde_json::private::Number":"not a number"}"#,
            r#"{"$serde_json::private::Number":"not a number"}"#,
        ),
        (
            r#"{"$serde_json::private::Number":null}"#,
            r#"{"$serde_json::private::Number":null}"#,
        ),
        (
            r#"{"$serde_json::private::Number":"27","keep":true}"#,
            r#"{"$serde_json::private::Number":"27","keep":true}"#,
        ),
        (
            r#"{"$serde_json::private::Number":"26","\u0024serde_json::private::Number":"27"}"#,
            r#"{"$serde_json::private::Number":"27"}"#,
        ),
    ] {
        for (prefix, suffix) in [("", ""), ("[", "]"), (r#"{"nested":["#, "]}")] {
            let input = format!("{prefix}{raw}{suffix}");
            let value = parsed(input.as_bytes()).expect("valid literal object");
            assert_eq!(value.to_string(), format!("{prefix}{expected}{suffix}"));
        }
    }
}

#[test]
fn marker_objects_are_preserved_in_string_and_object_hook_content() {
    let document = r#"{"x":{"\u0024serde_json::private::Number":"27"}}"#;
    let string_content = rewrite_of(document);
    let object_content =
        format!(r#"{{"messages":[{{"role":"user","content":{document}}}],"tools":[]}}"#);
    for rewrite in [&string_content[..], object_content.as_bytes()] {
        let out = rewritten(br#"{"model":"m","x":0}"#, rewrite, "m")
            .expect("applied")
            .expect("replacement");
        assert_eq!(
            out,
            br#"{"model":"m","x":{"$serde_json::private::Number":"27"}}"#
        );
    }
}

#[test]
fn marker_objects_never_compare_equal_to_actual_numbers() {
    let marker = r#"{"$serde_json::private::Number":"27"}"#;
    for (current, wanted) in [(marker, "27"), ("27", marker)] {
        let body = format!(r#"{{"model":"m","x":{current},"keep":1.2300E+400}}"#);
        let document = format!(r#"{{"x":{wanted},"keep":1.2300E+400}}"#);
        let out = rewritten(body.as_bytes(), &rewrite_of(&document), "m")
            .expect("applied")
            .expect("replacement");
        assert_eq!(
            out,
            format!(r#"{{"model":"m","x":{wanted},"keep":1.2300E+400}}"#).as_bytes()
        );
    }
    let body =
        br#"{ "model":"m", "x": {"\u0024serde_json::private::Number":"27"}, "keep": 1.2300E+400 }"#;
    let rewrite = rewrite_of(r#"{"x":{"$serde_json::private::Number":"27"},"keep":1.2300E+400}"#);
    assert_eq!(rewritten(body, &rewrite, "m"), Ok(Some(body.to_vec())));
}

#[test]
fn marker_objects_and_exact_numbers_survive_changed_members() {
    let number = format!(
        "18446744073709551617.{}1e+99999999999999999999",
        "0".repeat(2000)
    );
    let document = format!(r#"{{"x":[{{"$serde_json::private::Number":"27"}},{number}]}}"#);
    let out = rewritten(br#"{"model":"m","x":null}"#, &rewrite_of(&document), "m")
        .expect("applied")
        .expect("replacement");
    assert_eq!(
        out,
        format!(r#"{{"model":"m","x":[{{"$serde_json::private::Number":"27"}},{number}]}}"#)
            .as_bytes()
    );
    assert_eq!(
        parsed(number.as_bytes())
            .expect("exact numeric token")
            .to_string(),
        number
    );
    for malformed in [
        br#"{"x":{"$serde_json::private::Number":"27"}} trailing"#.as_slice(),
        br#"{"x":{"$serde_json::private::Number":"\ud800"}}"#,
    ] {
        assert!(parsed(malformed).is_err());
    }
}

#[test]
fn a_rewrite_over_a_body_that_does_not_parse_is_a_fault_before_the_rewrite_is_read() {
    let echo = br#"{"messages":[{"role":"user","content":"{\"x\":\"\\ud800\"}"}],"tools":[]}"#;
    for body in [
        &br#"{"model":"m","pii":"123-45-6789","x":"\ud800"}"#[..],
        br#"["\ud800"]"#,
    ] {
        for rewrite in [&echo[..], &rewrite_of(r#"{"state":{}}"#)[..], b"not json"] {
            assert_eq!(rewritten(body, rewrite, "m"), Err(NotApplied::Fault));
        }
    }
    assert_eq!(rewritten(br#"{"model":"m"}"#, b"not json", "m"), Ok(None));

    assert_eq!(
        rewritten(b"{\"model\":", &rewrite_of(r#"{"state":{}}"#), "m"),
        Err(NotApplied::Fault)
    );
}

/// The splice check validates the document directly, without reading its model again.
#[test]
fn spliceable_validates_without_a_second_head_read() {
    let bad = br#"{"model":"jev","x":"\uZZZZ"}"#;
    assert!(readable(bad).is_err());
    assert!(
        spliceable_named(bad, Some("jev"), "jev"),
        "no rewrite needed"
    );
    assert!(spliceable_named(bad, None, "jev-1"), "no model named");
    for bad in [
        bad.as_slice(),
        br#"{"model":"jev","x":"\ud800"}"#,
        br#"{"model":"jev","x":01}"#,
        br#"{"model":"jev","x":1e}"#,
        br#"{"model":"jev"} trailing"#,
    ] {
        assert!(!spliceable_named(bad, Some("jev"), "jev-1"), "{bad:?}");
    }
    assert!(spliceable_named(
        br#"{"model":"jev"}"#,
        Some("jev"),
        "jev-1"
    ));
}

#[test]
fn a_member_name_is_compared_with_its_escapes_decoded() {
    /// An answer, and what it bills.
    type Case = (&'static [u8], Vec<(u32, u64)>);
    let cases: [Case; 5] = [
        (
            br#"{"usage":{"\u0075nits":7}}"#,
            vec![(DECISION, 7), (FEE, 1)],
        ),
        (
            br#"{"\u0075sage":{"units":4}}"#,
            vec![(DECISION, 4), (FEE, 1)],
        ),
        (
            br#"{"usage":{"units":2,"input\u005Ftokens":3,"\/output_tokens":5}}"#,
            vec![(DECISION, 2), (INPUT, 3), (FEE, 1)],
        ),
        (
            br#"{"\u0065rror":{"code":"x"},"usage":{"units":3}}"#,
            vec![],
        ),
        (
            br#"{"err\\or":{"code":"x"},"usage":{"units":3}}"#,
            vec![(DECISION, 3), (FEE, 1)],
        ),
    ];
    for (body, want) in cases {
        for at in 0..=body.len() {
            assert_eq!(
                split_billed(200, body, at),
                Ok(want.clone()),
                "{} split at {at}",
                String::from_utf8_lossy(body)
            );
        }
        assert_eq!(by_pointer(200, body), Ok(want), "the pointers read it so");
    }
    assert_eq!(
        nanojev_billed(200, br#"{"execution":{"\u0073tates":6}}"#),
        Ok(vec![(DECISION, 6), (FEE, 1)])
    );
}

#[test]
fn a_large_rewrite_preserves_kept_bytes_and_applies_changes() {
    const MEMBERS: usize = 2000;
    let members: Vec<String> = (0..MEMBERS).map(|i| format!(r#""k{i}":{i}"#)).collect();
    let body = format!(r#"{{"model":"m",{}}}"#, members.join(","));

    let kept: Vec<String> = (1..MEMBERS)
        .map(|i| {
            if i == 7 {
                r#""k7":"seven""#.to_string()
            } else {
                format!(r#""k{i}":{i}"#)
            }
        })
        .collect();
    let document = format!("{{{}}}", kept.join(","));
    let out = rewritten(body.as_bytes(), &rewrite_of(&document), "m")
        .expect("applies")
        .expect("changes the request");
    assert_eq!(
        String::from_utf8(out).expect("UTF-8"),
        format!(r#"{{"model":"m",{}}}"#, kept.join(","))
    );
}

#[test]
fn bytes_after_the_answer_document_cannot_replace_its_usage_or_error() {
    for (first, want) in [
        (
            &br#"{"usage":{"units":5}}"#[..],
            vec![(DECISION, 5), (FEE, 1)],
        ),
        (&br#"{"error":{"code":"x"}}"#[..], vec![]),
    ] {
        for suffix in [
            &br#"}{"usage":{"units":9}}"#[..],
            br#"{"error":{}}"#,
            br#"{"error":null,"usage":{"units":9}}"#,
        ] {
            let mut body = first.to_vec();
            body.extend_from_slice(suffix);
            for at in 0..=body.len() {
                let mut reading = FarEndReading::default();
                reading.piece(&body[..at]);
                reading.piece(&body[at..]);
                let counts: Vec<_> = reading
                    .counts(Some(200))
                    .expect("the first document has readable usage")
                    .into_iter()
                    .flatten()
                    .map(|u| (u.class, u.amount))
                    .collect();
                assert_eq!(counts, want, "split at {at}: {body:?}");
            }
        }
    }
}

/// A duplicate usage parent replaces the whole earlier value, as the pointer reader does, even
/// when its replacement carries no counts.
#[test]
fn a_repeated_usage_parent_replaces_all_its_counts() {
    let bodies: &[&[u8]] = &[
        br#"{"usage":{"units":500,"input_tokens":90},"usage":{"units":2}}"#,
        br#"{"usage":{"units":500},"usage":{}}"#,
        br#"{"usage":{"units":500},"usage":null}"#,
        br#"{"usage":{"units":500},"usage":7}"#,
        br#"{"usage":{"units":500},"usage":"none"}"#,
        br#"{"usage":{"units":"bad"},"usage":{}}"#,
        br#"{"usage":{"units":500,"output_tokens":90},"\u0075sage":{"input_tokens":2}}"#,
    ];
    for body in bodies {
        let want = by_pointer(200, body);
        for at in 0..=body.len() {
            assert_eq!(split_billed(200, body, at), want, "{body:?} split at {at}");
        }
    }
    for replacement in ["{}", "null", "7", "\"none\""] {
        let body = format!(r#"{{"execution":{{"states":500}},"\u0065xecution":{replacement}}}"#);
        for at in 0..=body.len() {
            let mut reading = FarEndReading::for_dialect(tail::DIALECT_NANOJEV);
            reading.piece(&body.as_bytes()[..at]);
            reading.piece(&body.as_bytes()[at..]);
            assert_eq!(
                reading.counts(Some(200)),
                Err(UnreadableUsage(crate::codec::PTR_EXECUTION_STATES)),
                "{body} split at {at}"
            );
            assert_eq!(
                reading.floored(Some(200))[0].map(|u| u.source),
                Some(Source::Floor)
            );
        }
    }
}

#[test]
fn a_cut_in_a_replacement_usage_parent_cannot_bill_the_superseded_counts() {
    for (dialect, prefix) in [
        (
            tail::DIALECT_JEV,
            &br#"{"usage":{"units":500},"usage":{"units":2"#[..],
        ),
        (
            tail::DIALECT_NANOJEV,
            &br#"{"execution":{"states":500},"execution":{"states":2"#[..],
        ),
    ] {
        let mut reading = FarEndReading::for_dialect(dialect);
        reading.piece(prefix);
        assert_eq!(reading.cut(Some(200)), [None; 4], "{prefix:?}");
    }
}

/// RED (audit r3 r3-prod0-1): an identity rewrite over a body with a repeated root member does not
/// forward the caller's raw bytes: the earlier duplicate (which a hook redacted) is dropped.
#[test]
fn an_identity_rewrite_does_not_resurrect_an_earlier_duplicate_member() {
    let body = br#"{"model":"jev","state":"SSN 123-45-6789","state":"ok"}"#;
    let out = rewritten(body, &rewrite_of(r#"{"state":"ok"}"#), "jev")
        .expect("applies")
        .expect("a rewrite document");
    let text = String::from_utf8(out).expect("UTF-8");
    assert!(!text.contains("123-45-6789"), "{text}");
    assert!(text.contains(r#""state":"ok""#), "{text}");
    // A body with no repeated member is still forwarded byte for byte.
    let plain = br#"{"model":"jev", "state" : "ok"}"#;
    assert_eq!(
        rewritten(plain, &rewrite_of(r#"{"state":"ok"}"#), "jev").expect("applies"),
        Some(plain.to_vec())
    );
}

/// Zero models of a request's dialect is no "several": the refusal says none serves it, at 404,
/// since naming a model cannot route it (a request the generation that dropped its dialect's last
/// model still sees).
#[test]
fn a_dialect_with_no_model_is_refused_as_none_not_several() {
    let jev_only = std::collections::BTreeMap::from([("j".to_string(), tail::DIALECT_JEV)]);
    let nano = |d: &u32| *d == tail::DIALECT_NANOJEV;
    let refused = route_model(br#"{"input":{}}"#, &jev_only, nano).expect_err("no nanojev model");
    assert_eq!(refused, Unrouted::NoModel);
    assert_eq!(refused.status(), 404);
    assert!(!refused.text().contains("several"), "{}", refused.text());
    assert_eq!(
        route_model(br#"{"state":{}}"#, &models(&["a", "b"]), |_| true),
        Err(Unrouted::Unnamed),
        "several models of the dialect stay the 400"
    );
}

/// A cut answer bills no count a later member of the same name superseded: a repeated count (or a
/// repeated usage parent) replaces what was read before it from its key on, so a cut inside or right
/// after the replacing key bills none of the superseded value, as a cut in a replacement parent
/// already did.
#[test]
fn a_cut_after_a_repeated_count_cannot_bill_the_superseded_count() {
    for (dialect, prefix) in [
        (
            tail::DIALECT_JEV,
            &br#"{"usage":{"units":500,"units":2"#[..],
        ),
        (tail::DIALECT_JEV, br#"{"usage":{"units":500,"units":"#),
        (tail::DIALECT_JEV, br#"{"usage":{"units":500,"units""#),
        (tail::DIALECT_JEV, br#"{"usage":{"units":500},"usage""#),
        (tail::DIALECT_JEV, br#"{"usage":{"units":500},"usage":"#),
        (
            tail::DIALECT_NANOJEV,
            br#"{"execution":{"states":500,"states":2"#,
        ),
    ] {
        let mut reading = FarEndReading::for_dialect(dialect);
        reading.piece(prefix);
        assert_eq!(
            reading.cut(Some(200))[0],
            None,
            "{}",
            String::from_utf8_lossy(prefix)
        );
    }
    // A count read whole and never repeated still bills on a cut, and a whole answer's last
    // duplicate still wins.
    let mut reading = FarEndReading::for_dialect(tail::DIALECT_JEV);
    reading.piece(br#"{"usage":{"units":500,"other":1"#);
    assert_eq!(reading.cut(Some(200))[0].map(|u| u.amount), Some(500));
    assert_eq!(
        billed(200, br#"{"usage":{"units":500,"units":2}}"#),
        Ok(vec![(DECISION, 2), (FEE, 1)])
    );
}

#[test]
fn exact_decimal_counts_survive_every_fragment_boundary() {
    for (raw, amount, source) in [
        ("27.5", 27_500_000, Source::ReportedMicros),
        ("27.1", 27_100_000, Source::ReportedMicros),
        ("27.0", 27, Source::Reported),
        ("1e3", 1000, Source::Reported),
        ("0.000001", 1, Source::ReportedMicros),
        ("0.000000", 0, Source::Reported),
        (
            "9223372036854.775807",
            i64::MAX as u64,
            Source::ReportedMicros,
        ),
    ] {
        for (dialect, parent, field) in [(0, "usage", "units"), (1, "execution", "states")] {
            let body = format!(r#"{{"{parent}":{{"{field}":{raw}}}}}"#);
            for at in 0..=body.len() {
                let mut reading = FarEndReading::for_dialect(dialect);
                reading.piece(&body.as_bytes()[..at]);
                reading.piece(&body.as_bytes()[at..]);
                assert_eq!(
                    reading.counts(Some(200)).unwrap()[0],
                    Some(Units {
                        class: DECISION,
                        source,
                        amount,
                    }),
                    "{body} split at {at}"
                );
            }
        }
    }
}

#[test]
fn refused_decision_counts_never_become_a_floor() {
    for raw in ["0.0000001", "9223372036854.775808", "9223372036855", "-1"] {
        let body =
            format!(r#"{{"usage":{{"units":{raw},"input_tokens":27.1,"output_tokens":0}}}}"#);
        let mut reading = FarEndReading::default();
        reading.piece(body.as_bytes());
        assert!(reading.counts(Some(200)).is_err(), "{raw}");
        let kept = reading.floored(Some(200));
        assert_eq!(kept[0], None, "{raw}");
        assert_eq!(
            kept[1],
            Some(Units {
                class: INPUT,
                source: Source::ReportedMicros,
                amount: 27_100_000
            })
        );
        assert_eq!(
            kept[2],
            Some(Units {
                class: OUTPUT,
                source: Source::Reported,
                amount: 0
            })
        );
        assert_eq!(kept[3].map(|u| u.amount), Some(1));
    }
}

#[test]
fn a_cut_bills_only_completed_exact_decimal_counts() {
    let mut reading = FarEndReading::default();
    reading.piece(br#"{"usage":{"units":27.5,"input_tokens":27.1"#);
    assert_eq!(
        reading.cut(Some(200)),
        [
            Some(Units {
                class: DECISION,
                source: Source::ReportedMicros,
                amount: 27_500_000,
            }),
            None,
            None,
            None
        ]
    );
    reading.piece(br#", "output_tokens":0.0000001}}"#);
    assert_eq!(
        reading.cut(Some(200))[1],
        Some(Units {
            class: INPUT,
            source: Source::ReportedMicros,
            amount: 27_100_000,
        })
    );
    assert_eq!(reading.cut(Some(200))[2], None);
}

/// Malformed documents (bad structure, a bad literal, a bad escape, bytes that are not UTF-8) are
/// refused by both the routing read and the whole-body check; well-formed literals pass.
#[test]
fn a_malformed_document_is_refused_where_it_breaks() {
    for bad in [
        &br#"{1:2}"#[..],
        br#"{"a" 1}"#,
        br#"{"a":1 "b":2}"#,
        br#"[1 2]"#,
        br#"[{"a" 1}]"#,
        br#"[{"a":1 "b":2}]"#,
        br#"[fals]"#,
        b"\"\xff\"",
        br#"["\ud800"]"#,
        br#"["\x"]"#,
    ] {
        assert!(!valid(bad), "{}", String::from_utf8_lossy(bad));
        assert!(readable(bad).is_err(), "{}", String::from_utf8_lossy(bad));
    }
    for good in [
        &br#"[false, true, null]"#[..],
        br#"{"model":"m","f":false}"#,
    ] {
        assert!(valid(good), "{}", String::from_utf8_lossy(good));
        assert!(readable(good).is_ok(), "{}", String::from_utf8_lossy(good));
    }
}

/// The string check reads one string only: any other value is not a string that decodes.
#[test]
fn only_a_string_span_decodes_as_a_string() {
    assert!(string_decodes(br#""a\u00e9""#));
    assert!(!string_decodes(b"1"));
    assert!(!string_decodes(br#""\ud800""#));
}

/// A repeated member name is found at any depth and under any spelling; every other value reads.
#[test]
fn unique_names_reads_every_value_and_finds_a_repeat_at_any_depth() {
    assert!(unique_names(
        br#"{"a":[true,false,-1,1.5,1e400,null,"s",{"b":1,"c":{"b":2}}]}"#
    ));
    assert!(!unique_names(br#"[{"a":1,"\u0061":2}]"#));
    assert!(!unique_names(br#"{"x":{"y":[{"z":1,"z":2}]}}"#));
    assert!(!unique_names(b"{"));
}

/// A request that is not UTF-8 is counted as its lossy text, one replacement per bad byte.
#[test]
fn a_request_that_is_not_utf8_is_counted_lossily() {
    assert_eq!(chars_of(b"\xffab"), 3);
    assert_eq!(chars_of("é!".as_bytes()), 2);
}

/// A rewrite that names no document leaves the request as it is; the document is the last
/// message that resolves to an object.
#[test]
fn a_rewrite_names_its_document_by_the_last_object_message() {
    let body = br#"{"model":"m","state":{"a":1}}"#;
    for none in [
        &br#"[]"#[..],
        br#"{"messages":{}}"#,
        br#"{"tools":[]}"#,
        br#"{"messages":[{"role":"user"},7,{"content":7},{"content":"not json"},{"content":"[1]"}]}"#,
    ] {
        assert_eq!(
            rewritten(body, none, "m"),
            Ok(None),
            "{}",
            String::from_utf8_lossy(none)
        );
    }
    // A role-less message is itself the document; a later unusable one does not displace it.
    let out = rewritten(
        body,
        br#"{"messages":[{"state":{"a":2}},{"role":"user"},{"content":5}]}"#,
        "m",
    )
    .expect("the routed model")
    .expect("applied");
    assert_eq!(out, br#"{"model":"m","state":{"a":2}}"#);
}

/// A rewrite whose `model` is an array or a scalar other than the routed name is refused, the array
/// named by its kind and the scalar as written.
#[test]
fn a_rewrite_naming_an_array_or_number_model_is_refused_by_kind() {
    let body = br#"{"model":"m","state":{}}"#;
    for (model, named) in [("[1]", "<an array>"), ("7", "7"), ("true", "true")] {
        assert_eq!(
            rewritten(
                body,
                &rewrite_of(&format!(r#"{{"model":{model},"state":{{}}}}"#)),
                "m"
            ),
            Err(NotApplied::Model(ModelRewritten {
                named: named.to_string()
            })),
            "{model}"
        );
    }
}

/// A count spelled in 32 bytes or more is unreadable, even when its value is exact.
#[test]
fn a_count_spelled_past_the_scalar_cap_is_unreadable() {
    let long = format!("2.{}", "0".repeat(29));
    assert_eq!(long.len(), 31);
    assert_eq!(
        billed(200, format!(r#"{{"usage":{{"units":{long}}}}}"#).as_bytes())
            .expect("31 bytes read")[0],
        (DECISION, 2)
    );
    let longer = format!("{long}0");
    assert_eq!(
        billed(
            200,
            format!(r#"{{"usage":{{"units":{longer}}}}}"#).as_bytes()
        ),
        Err(UnreadableUsage(crate::codec::PTR_USAGE_UNITS))
    );
}

/// Member names are compared decoded: an escaped spelling of `usage` or `units` is the member,
/// and names carrying other escapes (or non-ASCII ones) are not.
#[test]
fn escaped_member_names_are_read_decoded() {
    let body =
        br#"{"\b\f\n\r\t":1,"\u00e9":{"units":9},"us\u0061ge":{"\u0075nits":4,"x\nunits":5}}"#;
    assert_eq!(billed(200, body).expect("a whole count")[0], (DECISION, 4));
}

/// A non-success bills nothing, whether read whole, after an unreadable count, or cut.
#[test]
fn a_non_success_bills_nothing_on_every_path() {
    let mut reading = FarEndReading::default();
    reading.piece(br#"{"usage":{"units":"x"}}"#);
    assert_eq!(reading.counts(Some(500)), Ok([None; 4]));
    assert_eq!(reading.floored(Some(500)), [None; 4]);
    assert_eq!(reading.cut(Some(500)), [None; 4]);
    assert_ne!(reading.floored(Some(200)), [None; 4]);
}

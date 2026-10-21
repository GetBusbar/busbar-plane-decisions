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
    let e = arrive("POST", ops::PATH_EVALUATE).expect("evaluate is claimed");
    assert_eq!(e.op, ops::OP_EVALUATE);
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
        assert_eq!(attempt_request(*dialect, b"{}").target, *target);
    }
}

/// RED (audit plane-decisions#5): THE DESIGN section 7, "its card prices `decision`, `input_tokens`
/// and `output_tokens` (family `decision`, all three required, 0 = free)".
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
    // The request fee is the one fee unit, and it is a class at its own index.
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
fn an_attempt_forwards_the_caller_body_unchanged() {
    let body = br#"{"state":{"session":"s"},"context":{}}"#;
    let r = attempt_request(tail::DIALECT_JEV, body);
    assert_eq!(r.verb, "POST");
    assert_eq!(r.target, "/v1/systemone");
    assert_eq!(r.fields, [("content-type", &b"application/json"[..])]);
    assert_eq!(r.auth, "decision-egress");
    assert!(
        std::ptr::eq(r.body, &body[..]),
        "the body is borrowed, not copied"
    );
}

/// The answer's counts as `(class, amount)`, for a far end answering `status` with `body`.
fn billed(status: u32, body: &[u8]) -> Result<Vec<(u32, u64)>, UnreadableUsage> {
    let mut reading = FarEndReading::new();
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
fn far_end_pieces_are_relayed_unchanged_and_the_count_read_over_the_whole_answer() {
    let mut reading = FarEndReading::new();
    let a = br#"{"request_id":"req_1","usage":{"un"#;
    let b = br#"its":42},"answers":{"decision":"approve"}}"#;
    assert_eq!(reading.piece(a), &a[..]);
    assert_eq!(reading.piece(b), &b[..]);
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

/// RED (audit plane-decisions#4): THE DESIGN section 7, "bills one `decision` per successful, settled
/// answer from every provider ... the hosted provider keeps 1.5.5's `/usage/units` as the decision
/// count". A success with no count bills one decision; one with a whole count bills the count.
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
        billed(200, br#"{"usage":{"units":3},"error":null}"#),
        Ok(vec![(DECISION, 3), (FEE, 1)]),
        "a null error is no error"
    );
    let max = format!(r#"{{"usage":{{"units":{}}}}}"#, i64::MAX);
    assert_eq!(
        billed(200, max.as_bytes()),
        Ok(vec![(DECISION, i64::MAX as u64), (FEE, 1)])
    );
}

/// RED (audit plane-decisions#4): one success rule for every count: a non-2xx answer bills nothing,
/// whatever usage it carries.
#[test]
fn a_non_success_bills_nothing() {
    for status in [400, 422, 500, 503] {
        assert_eq!(
            billed(status, br#"{"usage":{"units":2}}"#),
            Ok(vec![]),
            "{status}"
        );
    }
    let mut reading = FarEndReading::new();
    reading.piece(br#"{"usage":{"units":2}}"#);
    assert_eq!(
        reading.counts(None),
        Ok([None; 4]),
        "no status read, no bill"
    );
}

/// RED (audit plane-decisions#4): a success whose usage member is there but is not a whole count
/// fails loudly, naming the member (finding 395 kept: "LOCATED, never dropped"), never a silent 0.
#[test]
fn a_usage_member_that_is_not_a_whole_count_fails_loudly() {
    for units in [
        "-1".to_string(),
        "1.5".to_string(),
        "7.0".to_string(),
        "1e3".to_string(),
        "\"7\"".to_string(),
        "null".to_string(),
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

/// RED (audit plane-decisions#5): the token counts a success reports are billed in their classes.
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
        br#"{"error":{"code":"unsupported_operation","message":"x"}}"#
    );
}

/// RED (audit plane-decisions#1): every generation with a model mounts the claim (THE DESIGN
/// section 2: "more than one and none named is 400", so several models are served); none mounts
/// nothing, and the kernel's unclaimed-route bytes stand.
#[test]
fn one_or_more_models_mount_the_claim() {
    let systemone = ("POST", ops::PATH_SYSTEMONE, tail::DIALECT_JEV);
    let evaluate = ("POST", ops::PATH_EVALUATE, tail::DIALECT_NANOJEV);
    assert!(served_claims(&[]).is_empty());
    assert_eq!(served_claims(&[0]), vec![systemone]);
    assert_eq!(served_claims(&[0, 0]), vec![systemone]);
    assert_eq!(served_claims(&[1]), vec![evaluate]);
    assert_eq!(served_claims(&[1, 0, 1]), vec![systemone, evaluate]);
    assert_eq!(dialect_of("nanojev"), Some(tail::DIALECT_NANOJEV));
    assert_eq!(dialect_of("anthropic"), None);
}

fn models(names: &[&str]) -> std::collections::BTreeMap<String, ()> {
    names.iter().map(|n| ((*n).to_string(), ())).collect()
}

/// MODEL RESOLUTION (THE DESIGN section 2): the named model; one configured and none named is the
/// default; several and none named, or a `model` that is not a string, is 400; an unknown name 404.
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
    // A nested `model` is not the request's.
    assert_eq!(
        route_model(br#"{"state":{"model":"a"}}"#, &two, |_| true),
        Err(Unrouted::Unnamed)
    );

    let one = models(&["jev"]);
    assert_eq!(
        route_model(br#"{"state":{}}"#, &one, |_| true).map(|r| r.0),
        Ok("jev")
    );
    // A body 1.5.5 could not parse is refused as 1.5.5 refused it, never routed as naming none.
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
        Err(Unrouted::Unnamed)
    );

    assert_eq!(
        (Unrouted::Unnamed.status(), Unrouted::NotAName.status()),
        (400, 400)
    );
    assert_eq!(Unrouted::Unknown("zz".to_string()).status(), 404);
}

/// THE `upstream_model` SPLICE (THE DESIGN section 2): the top-level value alone, when it differs.
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
    let mut ok = FarEndReading::new();
    ok.piece(br#"{"usage":{"units":3}}"#);
    let fee = ok.counts(Some(200)).expect("a success")[3].expect("a success owes its fee");
    assert_eq!(
        (fee.class, fee.amount, fee.source),
        (FEE, 1, Source::Reported)
    );
    // RED ARMS.
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

/// The money rule's reading of a whole answer by its declared pointers, as the body would be read
/// kept whole: the reference the piece-by-piece reading is held to.
fn by_pointer(status: u32, body: &[u8]) -> Result<Vec<(u32, u64)>, UnreadableUsage> {
    use crate::codec::read_raw;
    let ok = (200..300).contains(&status) && read_raw(body, "/error").is_none_or(|e| e == b"null");
    if !ok {
        return Ok(vec![]);
    }
    let count = |p: &'static str| match read_raw(body, p) {
        None => Ok(None),
        Some(raw) => whole(raw).map(Some).ok_or(UnreadableUsage(p)),
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
    let mut reading = FarEndReading::new();
    reading.piece(&body[..at]);
    reading.piece(&body[at..]);
    reading.counts(Some(status)).map(|c| {
        c.into_iter()
            .flatten()
            .map(|u| (u.class, u.amount))
            .collect()
    })
}

/// RED (audit plane-decisions#8): the answer is read as it passes, in fixed space, wherever the
/// pieces split it, and reads exactly what the declared pointers read over the whole answer.
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
        br#"{"usage":"n/a","x":[{"usage":{"units":1}}]}"#,
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

/// An answer of any size is read in the reading's own fixed space.
#[test]
fn a_large_answer_is_read_without_keeping_it() {
    let mut reading = FarEndReading::new();
    let before = std::mem::size_of_val(&reading);
    reading.piece(br#"{"answers":{"blob":""#);
    let filler = [b'x'; 4096];
    for _ in 0..256 {
        reading.piece(&filler);
    }
    reading.piece(br#""},"usage":{"units":4}}"#);
    assert_eq!(std::mem::size_of_val(&reading), before);
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

/// RED (audit plane-decisions#3): THE DESIGN section 2, NanoJev "meters the upstream's own billing
/// unit, counted from the answer already read (`execution.states`; an answer with no count refuses
/// loudly". Its answer is read by its own pointer, never jev's.
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
        Err(UnreadableUsage(crate::codec::PTR_EXECUTION_STATES))
    );
    assert_eq!(
        nanojev_billed(500, br#"{"result":{}}"#),
        Ok(vec![]),
        "a failure bills nothing"
    );
    // Wherever the pieces split it.
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

/// RED (audit plane-decisions#3): a request routes among its claim's dialect's models; a model of
/// the other dialect is this dialect's unknown model, and one model of the dialect is its default.
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

/// RED (audit plane-decisions R3, the count's source): a success whose answer states no decision
/// count bills the plane's floor of one decision, stated as the plane's (`UNITS_FLOOR`), never as a
/// count the far end reported (THE DESIGN section 7 keeps the two apart); a stated count, and the
/// request fee, stay reported.
#[test]
fn a_success_with_no_count_bills_the_planes_floor_not_a_far_end_count() {
    let mut none = FarEndReading::new();
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
    let mut stated = FarEndReading::new();
    stated.piece(br#"{"usage":{"units":5}}"#);
    let counts = stated.counts(Some(200)).expect("a success");
    assert_eq!(counts[0].map(|d| d.source), Some(Source::Reported));
}

/// RED (audit plane-decisions R3, the unbounded echo): an unknown model's name is echoed into the
/// refusal whole, in 1.5.5's and predev's words, however long, up to what the refusal's words may
/// run (`MAX_REFUSAL_TEXT`); only a name past that is cut, so the refusal's words never outgrow
/// what an `arrive` may answer.
#[test]
fn an_unknown_models_echoed_name_is_bounded() {
    let models = std::collections::BTreeMap::from([("jev".to_string(), ())]);
    let short = route_model(br#"{"model":"zz"}"#, &models, |_| true).expect_err("unknown");
    assert_eq!(
        short.text(),
        "The model 'zz' does not exist or you do not have access to it."
    );
    // A name well past a kilobyte is echoed whole.
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

/// A hook's rewrite (1.5.5's rewrite JSON) as the kernel hands it: one message whose content is
/// `document`.
fn rewrite_of(document: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "messages": [{ "role": "user", "content": document }],
        "tools": [],
    }))
    .expect("JSON")
}

/// THE REWRITE REPLACES THE REQUEST WHOLESALE, as 1.5.5 replaced the body: the members the document
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
    // An object message itself, with no role, is a document too.
    let bare = br#"{"messages":[{"state":2}],"tools":[]}"#;
    assert_eq!(
        rewritten(br#"{"state":1}"#, bare, "m")
            .expect("no model named")
            .expect("applied"),
        br#"{"state":2}"#
    );
}

/// RED (coordinator ruling on the rewrite, 1.5.5 parity: v1.5.5 `RewriteReply` replaces the body):
/// a member the document omits is DROPPED, never kept, so a hook redacts by omission.
#[test]
fn a_member_the_rewrite_omits_is_dropped() {
    let body = br#"{"model":"m","state":{"amount":7},"pii":{"ssn":"123-45-6789"}}"#;
    let out = rewritten(body, &rewrite_of(r#"{"state":{"amount":7}}"#), "m")
        .expect("no other model named")
        .expect("applied");
    assert_eq!(out, br#"{"model":"m","state":{"amount":7}}"#);
    assert!(!String::from_utf8_lossy(&out).contains("123-45-6789"));
}

/// RED (coordinator ruling on the rewrite): the `model` is no rewritable member. A document naming
/// the routed model (or the caller's own) applies; one naming another is refused as the hook's
/// failure, never a silent switch of provider.
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
    // A request naming no model, routed over the only one: the document may name that one.
    assert_eq!(
        rewritten(
            br#"{"state":{}}"#,
            &rewrite_of(r#"{"model":"m","state":{}}"#),
            "m"
        ),
        Ok(None)
    );
}

/// RED (review of the rewrite, the span scanner's depth): a rewrite the hook answered ALWAYS
/// applies. A caller member nested past the span scanner's depth (`MAX_JSON_DEPTH`, 64; serde reads
/// to 128) left the caller's bytes unreadable, and the rewrite did not apply: the member a redaction
/// hook omitted went upstream. Now the replacement is written from the hook's document itself.
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

/// A rewrite that changes nothing, names no document, or meets a body that is no object, leaves the
/// request as it was (1.5.5).
#[test]
fn a_rewrite_that_does_not_apply_leaves_the_request() {
    let body = br#"{"state":{"amount":7}}"#;
    for rewrite in [
        &rewrite_of(r#"{"state":{"amount":7}}"#)[..],
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
    let view = project(body, tail::DIALECT_JEV, &routes, |d| *d);
    assert_eq!(
        view,
        Projected {
            entry: "a",
            dialect: "jev",
            chars: body.len() - 1,
        }
    );
    // The only model of its dialect is the default; a model of the other dialect is none.
    assert_eq!(
        project(b"{}", tail::DIALECT_NANOJEV, &routes, |d| *d).entry,
        "n"
    );
    assert_eq!(
        project(br#"{"model":"n"}"#, tail::DIALECT_JEV, &routes, |d| *d).entry,
        ""
    );
}

/// THE 1.5.5 PARITY TABLE FOR THE HEAD READ (`readable`): what 1.5.5's validating parse (sonic-rs
/// 0.5.8: top-level keys and the values of `HEAD_KEYS` decoded, every other value SKIPPED by
/// `skip_one(true)`) answered for each body. `Ok(model)` = accepted; `Err` = its 400. Derived from
/// sonic-rs 0.5.8 `parser.rs` (`skip_escaped_chars`: after a backslash the next byte is consumed and
/// for `u` four more, unchecked, with six bytes needed from the `u`; the other escapes come from
/// `ESCAPED_TAB`; control bytes refuse) and `lazy_body.rs` (`Head`, `HeadKey`).
#[test]
fn the_head_read_matches_sonic_rs_0_5_8_on_escapes() {
    let a = || Ok(Some(serde_json::json!("a")));
    type Read = Result<Option<serde_json::Value>, Unrouted>;
    let table: &[(&str, &[u8], Read)] = &[
        // A skipped value's \u escape is never checked: invalid hex, lone surrogates (either half).
        (
            "invalid hex, skipped",
            br#"{"model":"a","x":"\uZZZZ"}"#,
            a(),
        ),
        ("short hex, skipped", br#"{"model":"a","x":"\u12G4"}"#, a()),
        ("lone high, skipped", br#"{"model":"a","x":"\ud800"}"#, a()),
        ("lone low, skipped", br#"{"model":"a","x":"\udc00"}"#, a()),
        (
            "reversed pair, skipped",
            br#"{"model":"a","x":"\udc00\ud800"}"#,
            a(),
        ),
        (
            "valid pair, skipped",
            br#"{"model":"a","x":"\ud83d\ude00"}"#,
            a(),
        ),
        (
            "nested in an array",
            br#"{"model":"a","x":["\uXYZW",1]}"#,
            a(),
        ),
        (
            "a skipped object's key",
            br#"{"model":"a","x":{"\ud800":"\uZZZZ"}}"#,
            a(),
        ),
        (
            "a skipped key before the model",
            br#"{"x":"\uZZZZ","model":"a"}"#,
            a(),
        ),
        ("root array element", br#"["\uZZZZ"]"#, Ok(None)),
        // The four swallowed bytes may hold the closing quote, or an escaped one.
        ("quote swallowed", br#"{"model":"a","x":"\u12\"}"}"#, a()),
        (
            "quote swallowed, then more",
            br#"{"model":"a","x":"\u"   "}"#,
            a(),
        ),
        ("escaped quote", br#"{"model":"a","x":"q\"uote"}"#, a()),
        (
            "escaped backslash then quote",
            br#"{"model":"a","x":"q\\"}"#,
            a(),
        ),
        (
            "every simple escape",
            br#"{"model":"a","x":"\"\\\/\b\f\n\r\t"}"#,
            a(),
        ),
        // Refused in a skipped value all the same: a bad simple escape, a control byte.
        (
            "bad simple escape",
            br#"{"model":"a","x":"\q"}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "bad escape x",
            br#"{"model":"a","x":"\x41"}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "raw newline",
            b"{\"model\":\"a\",\"x\":\"a\nb\"}",
            Err(Unrouted::Unparsable),
        ),
        (
            "raw tab",
            b"{\"model\":\"a\",\"x\":\"a\tb\"}",
            Err(Unrouted::Unparsable),
        ),
        // A backslash at the buffer's end: nothing to escape, or fewer than six bytes from the `u`.
        (
            "backslash at the end",
            br#"{"model":"a","x":"\"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "u at the end",
            br#"{"model":"a","x":"\u"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "u1234 at the end",
            br#"["\u1234"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "five bytes from the u",
            br#"["\u12"]"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "six bytes from the u, quote eaten",
            br#"["\u123"]"#,
            Err(Unrouted::Unparsable),
        ),
        ("valid, six bytes from the u", br#"["\u1234"]"#, Ok(None)),
        // The values 1.5.5 DECODED refuse the same escapes: keys, the `HEAD_KEYS` values, a root string.
        (
            "invalid hex in a key",
            br#"{"\uZZZZ":1}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "lone surrogate in a key",
            br#"{"model":"a","\ud800":1}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "invalid hex in the model",
            br#"{"model":"\uZZZZ"}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "lone surrogate in the model",
            br#"{"model":"a\ud800"}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "invalid hex in system",
            br#"{"model":"a","system":"\uZZZZ"}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "lone surrogate in system",
            br#"{"model":"a","system":"\ud800"}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "lone surrogate in stream",
            br#"{"model":"a","stream":"\ud800"}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "in stream_options",
            br#"{"model":"a","stream_options":{"k":"\uZZZZ"}}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "in the shim key",
            br#"{"model":"a","__busbar_gemini_json_array":["\ud800"]}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "in a non-string model",
            br#"{"model":{"\ud800":1}}"#,
            Err(Unrouted::Unparsable),
        ),
        ("root string", br#""\uZZZZ""#, Err(Unrouted::Unparsable)),
        // An escape decoded: a key spelled with one still names the model.
        ("escaped key", br#"{"mod\u0065l":"a"}"#, a()),
        // The rest of the grammar stays strict.
        (
            "trailing bytes",
            br#"{"model":"a"} x"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "trailing comma",
            br#"{"model":"a","x":[1,]}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "leading zero",
            br#"{"model":"a","x":01}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "bare minus",
            br#"{"model":"a","x":-}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "fraction needs a digit",
            br#"{"model":"a","x":1.}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "exponent needs a digit",
            br#"{"model":"a","x":1e}"#,
            Err(Unrouted::Unparsable),
        ),
        ("numbers", br#"{"model":"a","x":[-0,1.5e+3,2E-2,0.0]}"#, a()),
        (
            "bad literal",
            br#"{"model":"a","x":tru}"#,
            Err(Unrouted::Unparsable),
        ),
        (
            "truncated",
            br#"{"model":"a","x":[1,2"#,
            Err(Unrouted::Unparsable),
        ),
        ("empty", b"", Err(Unrouted::Unparsable)),
    ];
    for (name, body, want) in table {
        assert_eq!(
            &readable(body),
            want,
            "{name}: {}",
            String::from_utf8_lossy(body)
        );
    }
}

/// THE 1.5.5 PARITY TABLE FOR A ROOT SCALAR'S RANGE AND A SKIPPED VALUE'S DEPTH (`readable`), each
/// outcome what 1.5.5's head parse answered (v1.5.5 `lazy_body.rs`, `Head`, over sonic-rs 0.5.8).
///
/// RED (opus review, B2): a root number 1.5.5 DECODED (`deserialize_any`, `parse_number`) refuses
/// when it overflows to infinity (sonic-number 0.1.2 `FloatMustBeFinite`), where the head read only
/// skipped it and served it. An integer past `u64` or `i64` is read as a finite double, served; a
/// number inside a skipped value is never ranged; one inside a head value is.
///
/// RED (opus review, B3): sonic's checked skip has no depth bound, and a `\u` escape that swallows
/// the closing quote leaves 1.5.5's depth scan reading the rest of the body as a string, so a body
/// nested past 128 behind one was served, where the head read refused it at 128. A head value is
/// held to the 253 levels sonic's deserializer left it (255, less one for the root object, less the
/// level that refuses).
#[test]
fn the_head_read_matches_1_5_5_on_root_numbers_and_unbounded_skips() {
    type Read = Result<Option<serde_json::Value>, Unrouted>;
    let a = || Ok(Some(serde_json::json!("a")));
    let served = || Ok(None);
    let refused = || Err(Unrouted::Unparsable);
    let nested = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
    // `"\uAAA\"`: sonic eats `uAAA\` after the backslash and the next quote closes the string; the
    // depth scan reads `\"` as an escaped quote, and the string as still open.
    let trick = r#"{"model":"a","x":"\uAAA\","#;
    let one_then_zeros = |n: usize| format!("1{}", "0".repeat(n));
    let table: Vec<(&str, String, Read)> = vec![
        ("1e400", "1e400".into(), refused()),
        ("-1e400", "-1e400".into(), refused()),
        ("1e308", "1e308".into(), served()),
        ("1e309", "1e309".into(), refused()),
        ("-1e309", "-1e309".into(), refused()),
        (
            "the largest double",
            "1.7976931348623157e308".into(),
            served(),
        ),
        (
            "rounds to the largest double",
            "1.7976931348623158e308".into(),
            served(),
        ),
        ("rounds past it", "1.7976931348623159e308".into(), refused()),
        ("u64::MAX", "18446744073709551615".into(), served()),
        ("past u64", "18446744073709551616".into(), served()),
        ("i64::MIN", "-9223372036854775808".into(), served()),
        ("past i64", "-9223372036854775809".into(), served()),
        ("309 digits", one_then_zeros(308), served()),
        ("310 digits", one_then_zeros(309), refused()),
        ("401 digits", one_then_zeros(400), refused()),
        (
            "-401 digits",
            format!("-{}", one_then_zeros(400)),
            refused(),
        ),
        ("underflow", "1e-400".into(), served()),
        ("zero, a large exponent", "0e99999".into(), served()),
        (
            "zero fraction, a large exponent",
            "0.0e99999".into(),
            served(),
        ),
        (
            "exponent past i32",
            "1e99999999999999999999".into(),
            refused(),
        ),
        ("spaced", " 7 ".into(), served()),
        ("literal", "true".into(), served()),
        ("in a root array, skipped", "[1e400]".into(), served()),
        (
            "in a skipped member",
            r#"{"model":"a","x":1e400}"#.into(),
            a(),
        ),
        (
            "401 digits in a skipped member",
            format!(r#"{{"model":"a","x":[{}]}}"#, one_then_zeros(400)),
            a(),
        ),
        (
            "in stream, decoded",
            r#"{"model":"a","stream":1e400}"#.into(),
            refused(),
        ),
        (
            "in stream_options, decoded",
            r#"{"model":"a","stream_options":{"k":-1e400}}"#.into(),
            refused(),
        ),
        // Depth: the scan's own edge, without the escape.
        (
            "127 under the root",
            format!(r#"{{"model":"a","y":{}}}"#, nested(127)),
            a(),
        ),
        (
            "128 under the root",
            format!(r#"{{"model":"a","y":{}}}"#, nested(128)),
            refused(),
        ),
        // Behind the escape: a skipped value has no bound.
        (
            "200 behind the escape",
            format!(r#"{trick}"y":{}}}"#, nested(200)),
            a(),
        ),
        (
            "1000 behind the escape",
            format!(r#"{trick}"y":{}}}"#, nested(1000)),
            a(),
        ),
        (
            "1000 behind the escape, unclosed",
            format!(r#"{trick}"y":{}}}"#, &nested(1000)[1..]),
            refused(),
        ),
        (
            "1000 in a root array",
            format!(r#"["\uAAA\",{}]"#, nested(1000)),
            served(),
        ),
        // A head value behind the escape: 253 levels, not 254.
        (
            "system, 253",
            format!(r#"{trick}"system":{}}}"#, nested(253)),
            a(),
        ),
        (
            "system, 254",
            format!(r#"{trick}"system":{}}}"#, nested(254)),
            refused(),
        ),
        (
            "system object, 253",
            format!(r#"{trick}"system":{{"a":{}}}}}"#, nested(252)),
            a(),
        ),
        (
            "system object, 254",
            format!(r#"{trick}"system":{{"a":{}}}}}"#, nested(253)),
            refused(),
        ),
    ];
    for (name, body, want) in &table {
        assert_eq!(
            &readable(body.as_bytes()),
            want,
            "{name}: {}",
            body.get(..80).unwrap_or(body)
        );
    }
}

/// The skipped-escape cases 1.5.5 SERVED are the ones its DOM parse (a rewrite hook's pass) refused:
/// the whole document does not parse, which is what a rewrite over such a body must FAULT on.
#[test]
fn a_body_the_head_read_accepts_may_not_parse_as_a_document() {
    for body in [
        &br#"{"model":"a","x":"\uZZZZ"}"#[..],
        br#"{"model":"a","x":"\ud800"}"#,
        br#"{"model":"a","x":"\u12\"}"}"#,
        br#"["\uZZZZ"]"#,
    ] {
        assert!(readable(body).is_ok(), "{}", String::from_utf8_lossy(body));
        assert!(parsed(body).is_err(), "{}", String::from_utf8_lossy(body));
        assert!(!valid(body), "{}", String::from_utf8_lossy(body));
    }
    for body in [
        &br#"{"model":"a","x":"\u00e9\ud83d\ude00"}"#[..],
        br#"[1,{"a":null}]"#,
    ] {
        assert!(parsed(body).is_ok() && valid(body));
    }
}

/// RED (review, B2): a rewrite over a body whose document does not parse is a FAULT, decided before
/// the hook's own document is read: a hook that echoes the bad member back and drops `pii` answers a
/// rewrite that does not read either, which was "does not apply" and let the body through
/// un-redacted. A top-level array that does not parse is the same FAULT, never replaced by an
/// object; an unreadable rewrite over a body that parses still leaves the request.
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
    // A body the head cannot read at all is a FAULT too, never "does not apply".
    assert_eq!(
        rewritten(b"{\"model\":", &rewrite_of(r#"{"state":{}}"#), "m"),
        Err(NotApplied::Fault)
    );
}

/// B3: the splice check reads the document without building it, as strictly as the DOM parse.
#[test]
fn spliceable_validates_without_a_second_head_read() {
    let bad = br#"{"model":"jev","x":"\uZZZZ"}"#;
    assert!(readable(bad).is_ok());
    assert!(!spliceable(bad, "jev-1"));
    assert!(
        spliceable_named(bad, Some("jev"), "jev"),
        "no rewrite needed"
    );
    assert!(spliceable_named(bad, None, "jev-1"), "no model named");
    assert!(!spliceable_named(bad, Some("jev"), "jev-1"));
    assert!(spliceable_named(
        br#"{"model":"jev"}"#,
        Some("jev"),
        "jev-1"
    ));
}

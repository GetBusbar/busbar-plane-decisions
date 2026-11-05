use super::*;

#[path = "../../../testing/settings/integer_token.rs"]
mod settings_profile;

use crate::ops;

/// `section`'s routes, the host stating provider `typesafe`'s protocol `jev`.
fn routed(section: &DecisionsSection) -> BTreeMap<String, ModelRoute> {
    let facts = BTreeMap::from([("typesafe".to_string(), "jev".to_string())]);
    routes(section, &facts).expect("every model's dialect is spoken")
}

#[test]
fn each_dialects_claim_is_published_with_a_model_of_it() {
    let jev = ClaimSpec::new("POST", ops::PATH_SYSTEMONE, claims::TRANSPORT, CLAIM_EXACT);
    let evaluate = ClaimSpec {
        refusal_dialect: 1,
        ..ClaimSpec::new("POST", ops::PATH_EVALUATE, claims::TRANSPORT, CLAIM_EXACT)
    };
    assert_eq!(
        snapshot_spec(&[tail::DIALECT_NANOJEV]).claims,
        vec![evaluate.clone()]
    );
    assert_eq!(
        snapshot_spec(&[tail::DIALECT_NANOJEV, tail::DIALECT_JEV]).claims,
        vec![jev.clone(), evaluate]
    );
    assert_eq!(snapshot_spec(&[tail::DIALECT_JEV]).claims, vec![jev]);
}

#[test]
fn a_models_dialect_is_its_override_or_its_providers_protocol() {
    let section = read_settings(
        br#"{"models":{"o":{"provider":"p","protocol":"nanojev"},"f":{"provider":"nano"},"j":{"provider":"p"}}}"#,
    )
    .expect("the section reads");
    let facts = BTreeMap::from([
        ("nano".to_string(), "nanojev".to_string()),
        ("p".to_string(), "jev".to_string()),
    ]);
    let r = routes(&section, &facts).expect("every dialect is spoken");
    assert_eq!(
        (r["o"].dialect, r["f"].dialect, r["j"].dialect),
        (
            tail::DIALECT_NANOJEV,
            tail::DIALECT_NANOJEV,
            tail::DIALECT_JEV
        )
    );
    let bogus = read_settings(br#"{"models":{"m":{"provider":"p","protocol":"bogus"}}}"#)
        .expect("the section reads");
    let words = routes(&bogus, &facts).expect_err("an unspoken dialect fails closed");
    assert!(
        words.contains("bogus") && words.contains("jev, nanojev"),
        "{words}"
    );
}

#[test]
fn a_model_of_an_unstated_provider_is_refused_by_name() {
    let facts = BTreeMap::from([("p".to_string(), "nanojev".to_string())]);
    let section = read_settings(br#"{"models":{"m":{"provider":"ghost"},"n":{"provider":"p"}}}"#)
        .expect("the section reads");
    let words = routes(&section, &facts).expect_err("an unstated provider resolves no dialect");
    assert!(
        words.contains("decisions.models.m") && words.contains("'ghost'"),
        "{words}"
    );

    let overridden =
        read_settings(br#"{"models":{"m":{"provider":"ghost","protocol":"nanojev"}}}"#)
            .expect("the section reads");
    assert_eq!(
        routes(&overridden, &facts).expect("the override routes")["m"].dialect,
        tail::DIALECT_NANOJEV
    );
}

#[test]
fn the_statement_names_the_crate_version() {
    assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
}

#[test]
fn an_empty_blob_is_the_empty_section() {
    let section = read_settings(b"").expect("an empty blob reads");
    assert!(section.models.is_empty());
}

#[test]
fn the_settings_read_as_the_decisions_section() {
    let section = read_settings(br#"{"models":{"jev":{"provider":"typesafe"}}}"#)
        .expect("a well-formed section reads");
    assert_eq!(section.models.len(), 1);
}

#[test]
fn a_member_the_section_does_not_declare_is_refused_in_the_grammars_words() {
    let words = read_settings(br#"{"modles":{}}"#).expect_err("a typo is refused");
    assert!(words.contains("modles"), "{words}");
}

#[test]
fn one_or_more_models_claim_systemone_and_none_claims_nothing() {
    let one = snapshot_spec(&[tail::DIALECT_JEV]);
    assert_eq!(
        one.claims,
        vec![ClaimSpec::new(
            "POST",
            ops::PATH_SYSTEMONE,
            claims::TRANSPORT,
            CLAIM_EXACT
        )]
    );
    assert!(one.admin_routes.is_empty());

    assert!(one.audience.is_none() && one.resource_metadata.is_none());
    assert!(
        snapshot_spec(&[]).claims.is_empty(),
        "no model claims nothing"
    );
    for models in [2, 3] {
        let jev = vec![tail::DIALECT_JEV; models];
        assert_eq!(snapshot_spec(&jev).claims, one.claims, "{models} models");
    }
}

#[test]
fn a_generation_lists_every_configured_model_in_name_order() {
    let two =
        read_settings(br#"{"models":{"b":{"provider":"typesafe"},"a":{"provider":"typesafe"}}}"#)
            .expect("the section reads");
    let spec = generation_spec(&routed(&two));
    assert_eq!(spec.listed, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(spec.claims, snapshot_spec(&[0, 0]).claims);

    let one =
        read_settings(br#"{"models":{"jev":{"provider":"typesafe"}}}"#).expect("the section reads");
    let spec = generation_spec(&routed(&one));
    assert_eq!(spec.listed, vec!["jev".to_string()]);
    assert_eq!(spec.claims, snapshot_spec(&[0]).claims);

    let empty = read_settings(b"").expect("an empty blob reads");
    assert!(generation_spec(&routed(&empty)).listed.is_empty());
}

/// `validate` reads the blob stage 3g deals (`{decisions: <section>}`): the section inside it is
/// judged (a typo refused), a blob that writes no `decisions:` is the empty section, and the bare
/// section is not the dealt shape.
#[test]
fn validate_reads_the_dealt_blob_at_its_section() {
    assert!(read_dealt(b"").is_ok());
    assert!(read_dealt(br#"{}"#).is_ok());
    assert!(read_dealt(br#"{"decisions":{"models":{"jev":{"provider":"typesafe"}}}}"#).is_ok());
    assert!(read_dealt(br#"{"decisions":{"modles":{}}}"#).is_err());
    assert!(read_dealt(br#"[]"#).is_err());
}

#[test]
fn the_section_under_its_verb_reads_as_the_section() {
    let wrapped = read_settings(br#"{"decisions":{"models":{"jev":{"provider":"typesafe"}}}}"#)
        .expect("the {section: value} blob reads");
    assert_eq!(wrapped.models.len(), 1);
    assert!(read_settings(br#"{"decisions":null}"#)
        .expect("an absent section is the empty one")
        .models
        .is_empty());
    let words = read_settings(br#"{"decisions":{"modles":{}}}"#).expect_err("a typo is refused");
    assert!(words.contains("modles"), "{words}");

    assert!(read_settings(br#"{"decisions":{},"models":{}}"#).is_err());
}

/// The unit ceiling `tests/door.rs` drives the door to, spelled there as a number because the
/// crate's only public item is the door: the two are one value.
#[test]
fn the_unit_ceiling_is_the_one_the_door_tests_drive() {
    assert_eq!(MAX_UNITS, 4096);
}

#[test]
fn the_keyed_settings_read_as_the_decisions_section_beside_the_consumed_ones() {
    let section = read_settings(
        br#"{"decisions":{"models":{"jev":{"provider":"typesafe"}}},"providers":{"typesafe":{"protocol":"jev"}}}"#,
    )
    .expect("the keyed object reads at the plane's own key");
    assert_eq!(section.models.len(), 1);
    let bare = read_settings(br#"{"models":{"jev":{"provider":"typesafe"}}}"#)
        .expect("the bare section still reads");
    assert_eq!(bare.models.len(), 1);
}

/// RED (audit r3 r3-prod0-5): two bad models are refused in model-name order, so the same config
/// always names the same one.
#[test]
fn the_refusal_names_the_first_bad_model_in_name_order() {
    let json = r#"{"models":{
        "b":{"provider":"typesafe","protocol":"bar"},
        "a":{"provider":"typesafe","protocol":"foo"}}}"#;
    let facts = BTreeMap::from([("typesafe".to_string(), "jev".to_string())]);
    for _ in 0..64 {
        let section: DecisionsSection = serde_json::from_str(json).expect("parses");
        let err = spoken_overrides(&section).expect_err("neither protocol is spoken");
        assert!(err.contains("decisions.models.a "), "{err}");
        let err = routes(&section, &facts).expect_err("neither protocol is spoken");
        assert!(err.contains("decisions.models.a "), "{err}");
    }
}

/// One unit's piece never holds every other unit's: while one unit's entry is held (its piece being
/// read, its body spliced), another unit's entry is reached at once.
#[test]
fn one_units_piece_does_not_hold_every_other_unit() {
    let door = std::sync::Arc::new(DecisionsDoor::new(BTreeMap::new()));
    for unit in 0..64_u64 {
        door.units.insert(unit, Unit::default());
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let reached = door.units.with(&0, |held| {
        assert!(held.is_some(), "unit 0 is held");
        for unit in 1..64_u64 {
            let (door, tx) = (std::sync::Arc::clone(&door), tx.clone());
            std::thread::spawn(move || {
                door.units.with(&unit, |_| ());
                let _ = tx.send(unit);
            });
        }
        rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok()
    });
    assert!(reached, "every other unit waited on the one held");
}

/// 5029 is benign-recurring: an instance warns of an unreadable count once per dialect and member.
#[test]
fn an_unreadable_member_is_warned_of_once_per_dialect() {
    let door = DecisionsDoor::new(BTreeMap::new());
    let units = crate::codec::PTR_USAGE_UNITS;
    assert!(door.first_unreadable(tail::DIALECT_JEV, units));
    assert!(!door.first_unreadable(tail::DIALECT_JEV, units));
    assert!(door.first_unreadable(tail::DIALECT_NANOJEV, crate::codec::PTR_EXECUTION_STATES));
    assert!(door.first_unreadable(tail::DIALECT_JEV, crate::codec::PTR_USAGE_INPUT_TOKENS));
}

/// Published settings must match the lifecycle's own grammar, including nested refusals.
#[test]
fn settings_schema_matches_lifecycle_grammar() {
    assert_eq!(STATEMENT.settings_schema.ptr, SETTINGS_SCHEMA.as_ptr());
    assert_eq!(STATEMENT.settings_schema.len, SETTINGS_SCHEMA.len());
    assert_eq!(
        STATEMENT.settings_schema.fmt,
        busbar_contract::abi::mechanism::call::BLOB_JSON
    );
    let schema: serde_json::Value = serde_json::from_slice(SETTINGS_SCHEMA).unwrap();
    let validator = settings_profile::validator(&schema).unwrap();
    for (source, accepted) in [
        (r#"{}"#, true),
        (r#"null"#, true),
        (r#"{"decisions":null}"#, true),
        (
            r#"{"models":{"m":{"provider":"p","protocol":"nanojev"}},"hooks":["audit"],"upstream_credentials":"own"}"#,
            true,
        ),
        (
            r#"{"decisions":{"models":{"m":{"provider":"p"}}},"providers":{}}"#,
            true,
        ),
        (r#"{"models":{"m":{}}}"#, false),
        (
            r#"{"models":{"m":{"provider":"p","protocol":"bogus"}}}"#,
            false,
        ),
        (r#"{"models":{"m":{"provider":"p","typo":1}}}"#, false),
        (r#"{"hooks":"audit"}"#, false),
        (r#"{"upstream_credentials":"inherit"}"#, false),
        (r#"{"typo":1}"#, false),
    ] {
        let value: serde_json::Value = serde_json::from_str(source).unwrap();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            read_settings(&bytes)
                .and_then(|s| spoken_overrides(&s))
                .is_ok(),
            accepted,
            "lifecycle: {source}"
        );
        assert_eq!(validator.is_valid(&value), accepted, "schema: {source}");
    }
}

#[test]
fn settings_schema_integer_tokens_match_native_parser() {
    let schema: serde_json::Value = serde_json::from_slice(SETTINGS_SCHEMA).unwrap();
    let profile = settings_profile::validator(&schema).unwrap();
    let standard = jsonschema::validator_for(&schema).unwrap();
    assert_eq!(schema["x-busbar-settings-profile"], "integer-token-v1");
    for (pointer, template, integer, kind) in [
        (
            r#"/$defs/model/properties/max_requests"#,
            r#"{"models":{"m":{"provider":"p","max_requests":TOKEN}}}"#,
            "-1",
            "signed",
        ),
        (
            r#"/$defs/model/properties/max_concurrent/anyOf/0"#,
            r#"{"models":{"m":{"provider":"p","max_concurrent":TOKEN}}}"#,
            "1",
            "unsigned",
        ),
        (
            r#"/$defs/model/properties/default_max_tokens/anyOf/0"#,
            r#"{"models":{"m":{"provider":"p","default_max_tokens":TOKEN}}}"#,
            "1",
            "unsigned",
        ),
        (
            r#"/$defs/model/properties/attempt_timeout_ms/anyOf/0"#,
            r#"{"models":{"m":{"provider":"p","attempt_timeout_ms":TOKEN}}}"#,
            "1",
            "unsigned",
        ),
    ] {
        assert_eq!(
            schema.pointer(pointer).unwrap()["x-busbar-integer-token"],
            kind
        );
        for (token, accepted) in [
            (integer.to_string(), true),
            (format!("{integer}.0"), false),
            (format!("{integer}e0"), false),
            (format!("{integer}E+0"), false),
        ] {
            let text = template.replace("TOKEN", &token);
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(
                standard.is_valid(&value),
                "standard mathematical integer: {text}"
            );
            assert_eq!(profile.is_valid(&value), accepted, "profile: {text}");
            assert_eq!(
                read_settings(text.as_bytes())
                    .and_then(|s| spoken_overrides(&s))
                    .is_ok(),
                accepted,
                "native: {text}"
            );
        }
        // Do not infer signed negative-zero behavior from a regular expression:
        // the profile must agree with the actual lifecycle parser.
        for token in [
            "null",
            "0",
            "-0",
            "-0.0",
            "0e0",
            "-1",
            "18446744073709551616",
        ] {
            let text = template.replace("TOKEN", token);
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(
                profile.is_valid(&value),
                read_settings(text.as_bytes())
                    .and_then(|s| spoken_overrides(&s))
                    .is_ok(),
                "native sign/range: {text}"
            );
        }
    }
}

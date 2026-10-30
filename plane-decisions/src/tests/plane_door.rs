use super::*;
use crate::ops;

/// `section`'s routes, the host stating provider `typesafe`'s protocol `jev`.
fn routed(section: &DecisionsSection) -> BTreeMap<String, ModelRoute> {
    let facts = BTreeMap::from([("typesafe".to_string(), "jev".to_string())]);
    routes(section, &facts).expect("every model's dialect is spoken")
}

/// RED (audit plane-decisions#3): each dialect's claim is published when a model of it is
/// configured, and wears its dialect for a refusal before `arrive`.
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

/// RED (audit plane-decisions#3): THE DESIGN section 2, "a model's dialect is its override or its
/// provider's protocol"; a dialect the plane does not speak fails closed, naming those it does.
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

/// RED (audit plane-decisions R2 #3): #51, "Resolution: model override if present, else the
/// provider/catalog default"; THE DESIGN section 2, "a model's dialect is its override or its
/// provider's protocol". A model with no override whose provider the host stated no protocol for
/// has no dialect: it is refused by name, never resolved to the first dialect.
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
    // Its own `protocol` override still routes it.
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
    // Served on the plain data plane to a keyed caller: no audience, no protected-resource document.
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

/// THE SPEC'S BLOB SHAPE (`BUSBAR-1.6.0.md` section 4): the section under its own verb, `{decisions:
/// <section>}`, is read as the section; so is the bare section; and the wrapped section is judged
/// by the same grammar (a typo inside it is refused, naming the member).
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
    // RED ARM: the verb beside another member is no wrapper; it is an unknown member.
    assert!(read_settings(br#"{"decisions":{},"models":{}}"#).is_err());
}

/// The unit ceiling `tests/door.rs` drives the door to, spelled there as a number because the
/// crate's only public item is the door: the two are one value.
#[test]
fn the_unit_ceiling_is_the_one_the_door_tests_drive() {
    assert_eq!(MAX_UNITS, 4096);
}

/// RED-BEFORE-GREEN (busbar row 159; architect ruling 2026-10-23 R-6, spec Part 1: "a plane's
/// settings reach it as ONE validated JSON object `{section: value}`"): a host hands a plane that
/// consumes sections its own `decisions:` section beside each consumed section the deployment
/// writes (`providers:`), keyed. The plane reads its own key from that object, at open and at
/// refresh, and ignores the consumed sections there (their dialect facts reach it at open). RED:
/// only a one-key object was unwrapped, so the keyed object read as a section with an unknown
/// `decisions` member and the open was refused.
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

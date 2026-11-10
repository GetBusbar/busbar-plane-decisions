// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

use crate::root::serve::door_tests::*;

#[cfg(feature = "plane-decisions")]
use std::collections::BTreeMap;
#[cfg(feature = "plane-decisions")]
use std::sync::Arc;

#[cfg(feature = "plane-decisions")]
use axum::http::StatusCode;
#[cfg(feature = "plane-decisions")]
use busbar_contract::caps::ReasonCode;
#[cfg(feature = "plane-decisions")]
use busbar_contract::conn::{DeclaredConns, PollConns};
#[cfg(feature = "plane-decisions")]
use busbar_kernel::cost::CostModel;
#[cfg(feature = "plane-decisions")]
use busbar_kernel::governance::signing::{TokenSigner, DEFAULT_KID};
#[cfg(feature = "plane-decisions")]
use busbar_kernel::governance::{GovState, NewKeySpec, PLANE_LANE_SEP};
#[cfg(feature = "plane-decisions")]
use busbar_kernel::plane_driver::{refusal_status, EndPost, PlaneMoney};
// Catalogue metadata in these synthetic models declares the fixture contract released on
// 2026-11-09; it does not describe a real provider model or its release date.
#[cfg(feature = "plane-decisions")]
use busbar_plane_decisions::door as decisions_door;
#[cfg(feature = "plane-decisions")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(feature = "plane-decisions")]
use super::planes_tests::{composed_services, money, PUBLISHING};
#[cfg(feature = "plane-decisions")]
use super::{compose_planes, compose_planes_over, door_routes, DoorEgress};
#[cfg(feature = "plane-decisions")]
use crate::root::door_steps::{provider_routes, DoorReach, OutboundAuths};
#[cfg(feature = "plane-decisions")]
use crate::root::loader::dispatch::kinds::plane::Plane;
#[cfg(feature = "plane-decisions")]
use crate::root::loader::dispatch::{
    load_linked, Bind, DispatchConfig, Dispatcher, LinkedRow, NoSink,
};
#[cfg(feature = "plane-decisions")]
use crate::root::plane_node::{Node, NodeEndPost};

#[cfg(feature = "plane-decisions")]
static CARD: std::sync::LazyLock<crate::root::kernel::RootHistory> =
    std::sync::LazyLock::new(|| {
        let holder = crate::root::kernel::RootHistory::default();
        holder.apply(
            crate::root::kernel::card_from_config(
                std::iter::empty::<(&str, busbar_contract::billing::RawTierRates)>(),
                0,
                true,
            ),
            1_000,
        );
        holder
    });

#[cfg(feature = "plane-decisions")]
pub(crate) const CLAIMED: &str = "/v1/systemone";

#[cfg(feature = "plane-decisions")]
const CREDENTIAL: &str = "sk-door-test";

#[cfg(feature = "plane-decisions")]
const ANSWER: &str = r#"{"id":"d-1","decision":"approve","usage":{"units":1}}"#;

#[cfg(feature = "plane-decisions")]
const UNAVAILABLE: &str = r#"{"error":"unavailable"}"#;

#[cfg(feature = "plane-decisions")]
pub(crate) async fn send(
    router: &axum::Router,
    path: &str,
    token: Option<&str>,
) -> axum::response::Response {
    use tower::ServiceExt as _;
    let mut req = axum::http::Request::builder().method("POST").uri(path);
    if let Some(token) = token {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let req = req
        .body(axum::body::Body::from(r#"{"state":{"amount":7}}"#))
        .expect("a request");
    router
        .clone()
        .oneshot(req)
        .await
        .expect("the router answers")
}

#[cfg(feature = "plane-decisions")]
pub(crate) async fn far_end() -> (u16, tokio::sync::mpsc::UnboundedReceiver<String>) {
    far_end_failing_first(0).await
}

#[cfg(feature = "plane-decisions")]
async fn far_end_failing_first(
    failures: usize,
) -> (u16, tokio::sync::mpsc::UnboundedReceiver<String>) {
    far_end_answering(Arc::new(move |n| {
        if n < failures {
            Answer::Says("503 Service Unavailable", UNAVAILABLE)
        } else {
            Answer::Says("200 OK", ANSWER)
        }
    }))
    .await
}

#[cfg(feature = "plane-decisions")]
/// How the far end answers one connection.
#[derive(Debug, Clone, Copy)]
enum Answer {
    /// A status line and a JSON body.
    Says(&'static str, &'static str),
    /// Nothing: the request is read and the connection held open, unanswered.
    Holds,
}

#[cfg(feature = "plane-decisions")]
type Script = Arc<dyn Fn(usize) -> Answer + Send + Sync>;

#[cfg(feature = "plane-decisions")]
async fn far_end_answering(script: Script) -> (u16, tokio::sync::mpsc::UnboundedReceiver<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let (sent, heard) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut accepted = 0usize;
        while let Ok((mut socket, _)) = listener.accept().await {
            let answer = script(accepted);
            accepted += 1;
            let sent = sent.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 16 * 1024];
                let mut got = Vec::new();

                loop {
                    let Ok(n) = socket.read(&mut buf).await else {
                        return;
                    };
                    if n == 0 {
                        return;
                    }
                    got.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&got).to_string();
                    if let Some(at) = text.find("\r\n\r\n") {
                        let length = text[..at]
                            .lines()
                            .find_map(|l| {
                                let (name, value) = l.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        if got.len() >= at + 4 + length {
                            let _ = sent.send(text);
                            break;
                        }
                    }
                }
                let (line, body) = match answer {
                    Answer::Says(line, body) => (line, body),
                    Answer::Holds => {
                        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                        return;
                    }
                };
                let reply = format!(
                    "HTTP/1.1 {line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
                     connection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    (port, heard)
}

#[cfg(feature = "plane-decisions")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_claimed_request_is_served_through_the_door_and_its_money_posted() {
    let _one = PUBLISHING.lock().await;
    crate::dist_root_install::install();
    let instance = "serve-door-served";
    let (port, mut heard) = far_end().await;

    let judge = crate::root::connector::guard_for(&busbar_kernel::config::Destinations {
        block_private_addresses: false,
        ..Default::default()
    })
    .expect("the guard");
    let connector = busbar_core_connector::process::build_with_secrets(
        || {
            crate::root::connector::entries(
                crate::LINKED_TRANSPORT_DOORS,
                &busbar_contract::transport::TransportSettings::default(),
            )
        },
        judge,
        &[],
        Arc::new(|_, _| {}),
        busbar_core_connector::pool::PoolPosture::NONE,
        busbar_core_connector::spawn_env::linked_spawn_secret,
    )
    .expect("the connector builds");

    let dispatcher = Arc::new(Dispatcher::new(
        DispatchConfig::default(),
        busbar_kernel::host_services::caller_binding(),
    ));
    crate::root::connector::install_io(&dispatcher);

    let _ = dispatcher.lend_seal(busbar_kernel::seal::lend());
    let row = LinkedRow::of(decisions_door).expect("the door states its Statement");
    let plane = Arc::new(
        load_linked::<Plane>(
            &row,
            Bind {
                instance: Arc::from(instance),
                max_inflight_cap: 64,
                sink: Arc::new(NoSink),
                dispatcher: dispatcher.adopter(),
                conns: crate::root::loader::dispatch::ConnTable::Host(
                    crate::root::connector::admit(Arc::clone(&connector) as Arc<dyn DeclaredConns>),
                ),
            },
        )
        .expect("the linked door binds"),
    );
    let section_key = plane.served().section;

    let signer = TokenSigner::from_secret_bytes(&[7u8; 32], DEFAULT_KID);
    let gov = Arc::new(
        GovState::new_with_core(
            crate::root::serve::door_tests::process_store().await,
            None,
            Some(signer),
        )
        .await
        .expect("governance"),
    );
    let (key, token) = gov
        .mint_signed_async(
            NewKeySpec {
                name: "decider".to_string(),
                ..Default::default()
            },
            4_000_000_000,
            1_700_000_000,
        )
        .await
        .expect("mint");
    let key = Arc::new(key);
    gov.hydrate_budgets_async(&CostModel::flat(1), 0)
        .await
        .expect("hydrate");
    let node = Arc::new(Node::new());
    let mut book = crate::root::durability::node_book(
        busbar_kernel::persistence::journal::runtime::RuntimeJournal::open(
            gov.core_store_handle().expect("native Store"),
            busbar_kernel::door::op_id().counter().max(1),
            busbar_kernel::persistence::journal::runtime::BufferBudget {
                records: 4096,
                bytes: 16 * 1024 * 1024,
            },
        )
        .await
        .expect("native journal"),
        busbar_kernel::persistence::journal::runtime::BufferBudget {
            records: 4096,
            bytes: 16 * 1024 * 1024,
        },
    )
    .await
    .expect("native node book");
    crate::root::serve::door_tests::bind_process_amendments(&mut book.durability, gov.amendments())
        .await;
    node.bind_book(Arc::clone(&book.durability));
    let post = Arc::new(NodeEndPost::new(Arc::clone(&node)));
    let site = Arc::clone(&post);
    let book_money = Arc::clone(&gov);
    let plane_money = move || {
        Arc::new(PlaneMoney::new(
            Arc::clone(&book_money),
            Arc::clone(&site) as Arc<dyn EndPost>,
        ))
    };

    let key_file = std::env::temp_dir().join(format!("busbar-serve-door-{}", std::process::id()));
    std::fs::write(&key_file, CREDENTIAL).expect("the credential file");
    let provider: busbar_kernel::config::ProviderCfg = serde_yaml::from_str(&format!(
        "{{protocol: {}, base_url: 'http://127.0.0.1:{port}', api_key: {{file: '{}'}}, error_map: {{}}}}",
        plane.served().dialects[0],
        key_file.display()
    ))
    .expect("a provider entry");
    let providers = provider_routes(
        &std::collections::HashMap::from([(
            "typesafe".to_string(),
            busbar_kernel::config::ResolvedProvider::from(provider),
        )]),
        &busbar_kernel::config_validate::MetadataPosture::default(),
    );
    #[cfg(distribution_root)]
    crate::dist_root_install::install();
    let secrets: Arc<dyn busbar_contract::secret::SecretResolve> =
        Arc::new(busbar_kernel::config::secret::SecretResolver::builtins_only());
    let auths = OutboundAuths::new(
        Arc::clone(&dispatcher),
        crate::LINKED.auths,
        None,
        crate::root::loader::dispatch::ConnTable::Host(crate::root::connector::admit(Arc::clone(
            &connector,
        )
            as Arc<dyn DeclaredConns>)),
        busbar_kernel::diagnostics::Catalogue::NEUTRAL,
    );
    let reach = DoorReach {
        providers: &providers,
        secrets: Arc::clone(&secrets),
        auths: Arc::new(auths),
        conns: Arc::clone(&connector) as Arc<dyn PollConns>,
        stream_ceiling_secs: 600,
        catalog: None,
        models: None,
        upgrades: Vec::new(),
    };

    let mut sections = BTreeMap::new();
    sections.insert(
        section_key,
        serde_yaml::from_str("models: {m: {provider: typesafe}}\ncatalog: {m: {description: 'Busbar in-process Decisions test model (not a provider model)', release_date: '2026-11-09'}}").expect("a section"),
    );
    let mut served = compose_planes(
        &[(instance.to_string(), plane)],
        &dispatcher,
        &composed_services(),
        &sections,
        None,
        &plane_money,
        Some(&DoorEgress {
            reach: &reach,
            journal: Arc::clone(&post) as Arc<dyn busbar_kernel_egress::ports::Journal>,
        }),
        None,
        &busbar_kernel::plane_driver::serve::ServeTables::default(),
    )
    .await
    .expect("the door plane composes, its egress sealed");
    let _ = std::fs::remove_file(&key_file);
    let composed = &mut served.planes[0];
    assert!(
        composed.live.current().egress.is_some(),
        "the composition sealed its egress"
    );
    let money_steps = Arc::clone(&composed.money);
    served.post = Some(Arc::clone(&post));
    crate::dist_root_install::install();
    let library_path = crate::root::test_plugins::application_store_library();
    let statement = crate::root::loader::dispatch::rendering_of_library(&library_path)
        .expect("the actual store statement reads")
        .expect("the actual store exports its statement");
    let module = busbar_contract::abi::mechanism::rendering::read(&statement)
        .expect("the actual store statement is valid")
        .name;
    let mut manifest = crate::root::test_plugins::manifest(
        "store",
        &module,
        crate::root::test_plugins::FIXTURE_PUBLISHER,
    );
    manifest.statement = Some(hex::encode(statement));
    let library = std::fs::read(library_path).expect("the actual store library reads");
    let directory = crate::root::test_plugins::scratch("serve-production-498");
    let archive = crate::root::test_plugins::signed(
        &crate::dist_root_install::release_key(),
        manifest,
        &library,
    );
    std::fs::write(directory.join("store.tar.gz"), archive)
        .expect("stage the actual store archive");
    let settings = crate::root::test_plugins::application_store_settings(&directory);
    let signing_file = directory.join("signing.key");
    std::fs::write(&signing_file, [7u8; 32]).expect("the actual token signing secret");
    let deploy = busbar_kernel::config::deploy_from_deserializer(serde_json::json!({
        "providers": {},
        "models": {},
        "auth": { "chain": ["keys"], "admin_auth": [], "signing_key": {"file": signing_file} },
        "store": { "module": module, "settings": settings },
        "advanced": { "allow_destinations": ["127.0.0.1"] },
    }))
    .expect("the door deployment parses");
    let config = busbar_kernel::config::resolve(&deploy, &Default::default())
        .expect("the door deployment resolves");
    let plugins = crate::root::test_plugins::trusted_config(&directory);
    let rules = std::sync::Arc::new(crate::root::door_steps::Rules::default())
        as std::sync::Arc<dyn busbar_kernel::appbuild::RootRules>;
    let (mut app, rotate, limits) = crate::root::serve::door_tests::build_process_app(
        config,
        plugins,
        None,
        Default::default(),
        Default::default(),
        (None, None),
        None,
        &rules,
    )
    .await
    .expect("the actual product builds the door deployment");
    limits.stage_boot();
    crate::root::serve::door_tests::attach_process_prices(&app)
        .await
        .expect("native process price history");
    limits.keep().await.expect("the product limits commit");
    if let Some(rotate) = rotate {
        rotate.await;
    }
    app.amendments = Arc::clone(gov.amendments());
    app.governance = Some(Arc::clone(&gov));
    app.cost = Arc::new(CostModel::flat(1));
    let app = std::sync::Arc::new(app);

    let doors = door_routes(served, || CARD.pin(), &[], &[]).expect("its claims mount");
    let (router, _admin, _handle) =
        busbar_kernel::build_split_routers_serving(Arc::clone(&app), doors, 1 << 20, 0, false);

    let response = send(
        &router,
        CLAIMED,
        Some(token.expose(&busbar_kernel::seal::lend())),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "served through the door");
    let kind = response.headers()["content-type"].to_str().expect("a type");
    assert!(kind.starts_with("application/json"), "{kind}");
    let body = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .expect("the body");
    assert_eq!(
        &body[..],
        ANSWER.as_bytes(),
        "the far end's answer, relayed as it came"
    );

    let head = heard.recv().await.expect("the far end was dialled");
    assert!(head.starts_with("POST /v1/systemone "), "{head}");
    assert!(
        head.lines()
            .any(|l| l.eq_ignore_ascii_case(&format!("authorization: Bearer {CREDENTIAL}"))),
        "the member's credential, presented by the auth plugin serving its style: {head}"
    );
    assert!(
        !head.contains(token.expose(&busbar_kernel::seal::lend())),
        "the caller's own credential never reaches the far end: {head}"
    );
    assert!(
        head.ends_with(r#"{"state":{"amount":7}}"#),
        "the caller's body: {head}"
    );

    let usage = gov
        .usage_for_async(&app.cost, &key.id, busbar_kernel::store::now())
        .await
        .expect("a read")
        .expect("the key exists");
    assert_eq!(usage.requests, 1, "one admitted request");
    assert_eq!(
        money_steps.open_units(),
        0,
        "its money facts closed at its end"
    );
    assert_eq!(post.open_units(), 0, "its node facts closed at its end");

    let records = book
        .durability
        .lock()
        .expect("unpoisoned")
        .audit_records
        .clone();
    let decided: Vec<busbar_contract::Count> = records
        .iter()
        .flat_map(|r| r.usage.lines.iter())
        .filter(|l| l.class.as_str() == "decision")
        .map(|l| l.quantity)
        .collect();
    assert_eq!(
        decided,
        vec![1],
        "the decision the far end reported: {records:?}"
    );
}

#[cfg(feature = "plane-decisions")]
#[tokio::test]
async fn the_data_router_built_with_the_door_serves_only_its_claims() {
    let _one = PUBLISHING.lock().await;
    let instance = "serve-door";
    crate::dist_root_install::install();
    let library_path = crate::root::test_plugins::application_store_library();
    let statement = crate::root::loader::dispatch::rendering_of_library(&library_path)
        .expect("the actual store statement reads")
        .expect("the actual store exports its statement");
    let module = busbar_contract::abi::mechanism::rendering::read(&statement)
        .expect("the actual store statement is valid")
        .name;
    let mut manifest = crate::root::test_plugins::manifest(
        "store",
        &module,
        crate::root::test_plugins::FIXTURE_PUBLISHER,
    );
    manifest.statement = Some(hex::encode(statement));
    let library = std::fs::read(library_path).expect("the actual store library reads");
    let directory = crate::root::test_plugins::scratch("serve-production-578");
    let archive = crate::root::test_plugins::signed(
        &crate::dist_root_install::release_key(),
        manifest,
        &library,
    );
    std::fs::write(directory.join("store.tar.gz"), archive)
        .expect("stage the actual store archive");
    let settings = crate::root::test_plugins::application_store_settings(&directory);
    let deploy = busbar_kernel::config::deploy_from_deserializer(serde_json::json!({
        "providers": {},
        "models": {},
        "auth": { "chain": [], "admin_auth": [] },
        "store": { "module": module, "settings": settings },
        "advanced": { "allow_destinations": ["127.0.0.1"] },
    }))
    .expect("the door deployment parses");
    let config = busbar_kernel::config::resolve(&deploy, &Default::default())
        .expect("the door deployment resolves");
    let plugins = crate::root::test_plugins::trusted_config(&directory);
    let rules = std::sync::Arc::new(crate::root::door_steps::Rules::default())
        as std::sync::Arc<dyn busbar_kernel::appbuild::RootRules>;
    let (app, rotate, limits) = crate::root::serve::door_tests::build_process_app(
        config,
        plugins,
        None,
        Default::default(),
        Default::default(),
        (None, None),
        None,
        &rules,
    )
    .await
    .expect("the actual product builds the door deployment");
    limits.stage_boot();
    crate::root::serve::door_tests::attach_process_prices(&app)
        .await
        .expect("native process price history");
    limits.keep().await.expect("the product limits commit");
    if let Some(rotate) = rotate {
        rotate.await;
    }
    let app = std::sync::Arc::new(app);

    let (bare, _admin, _handle) =
        busbar_kernel::build_split_routers_serving(Arc::clone(&app), Vec::new(), 1 << 20, 0, false);
    let refused = u16::try_from(refusal_status(ReasonCode::Unauthenticated)).expect("a status");
    assert_ne!(send(&bare, CLAIMED, None).await.status().as_u16(), refused);

    let dispatcher = Arc::new(Dispatcher::new(
        DispatchConfig::default(),
        busbar_kernel::host_services::caller_binding(),
    ));
    crate::root::connector::install_io(&dispatcher);

    let _ = dispatcher.lend_seal(busbar_kernel::seal::lend());
    let row = LinkedRow::of(decisions_door).expect("the door states its Statement");
    let plane = Arc::new(
        load_linked::<Plane>(
            &row,
            Bind {
                instance: Arc::from(instance),
                max_inflight_cap: 64,
                sink: Arc::new(NoSink),
                dispatcher: dispatcher.adopter(),

                conns: crate::root::loader::dispatch::ConnTable::Probe,
            },
        )
        .expect("the linked door binds"),
    );
    let section = plane.served().section;
    let mut sections = BTreeMap::new();

    sections.insert(
        section,
        serde_yaml::from_str(&format!(
            "models: {{m: {{provider: typesafe, protocol: {}}}}}\ncatalog: {{m: {{description: 'Busbar in-process Decisions test model (not a provider model)', release_date: '2026-11-09'}}}}",
            plane.served().dialects[0]
        ))
        .expect("a section"),
    );
    let empty_money = money().await;
    let served = compose_planes(
        &[(instance.to_string(), plane)],
        &dispatcher,
        &composed_services(),
        &sections,
        None,
        &|| Arc::clone(&empty_money),
        None,
        None,
        &app.serve_tables,
    )
    .await
    .expect("the door plane composes");
    let doors = door_routes(served, || CARD.pin(), &[], &[]).expect("its claims mount");
    assert!(!doors.is_empty(), "the claim is a route");
    let (router, _admin, _handle) =
        busbar_kernel::build_split_routers_serving(app, doors, 1 << 20, 0, false);

    let unclaimed = send(&router, "/v1/unclaimed", None).await;
    assert_ne!(
        unclaimed.status().as_u16(),
        refused,
        "a path the door does not claim"
    );

    let response = send(&router, CLAIMED, None).await;
    assert_eq!(response.status().as_u16(), refused);
    let body = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .expect("the body");
    let body: serde_json::Value = serde_json::from_slice(&body).expect("the plane's JSON");
    assert_eq!(
        body["detail"]["message"],
        ReasonCode::Unauthenticated.as_str(),
        "the kernel wrote the text, the plane rendered it: {body}"
    );
    assert_eq!(
        body["detail"]["error_type"], "authentication_error",
        "{body}"
    );
    assert!(
        door_routes(super::Served::default(), || CARD.pin(), &[], &[])
            .expect("nothing to mount")
            .is_empty(),
        "a composition that claims nothing mounts nothing"
    );
}

#[cfg(feature = "plane-decisions")]
#[tokio::test]
async fn a_door_claiming_one_path_over_two_carriers_mounts_it_once() {
    let _one = PUBLISHING.lock().await;
    let instance = "serve-door-carriers";
    crate::dist_root_install::install();
    let library_path = crate::root::test_plugins::application_store_library();
    let statement = crate::root::loader::dispatch::rendering_of_library(&library_path)
        .expect("the actual store statement reads")
        .expect("the actual store exports its statement");
    let module = busbar_contract::abi::mechanism::rendering::read(&statement)
        .expect("the actual store statement is valid")
        .name;
    let mut manifest = crate::root::test_plugins::manifest(
        "store",
        &module,
        crate::root::test_plugins::FIXTURE_PUBLISHER,
    );
    manifest.statement = Some(hex::encode(statement));
    let library = std::fs::read(library_path).expect("the actual store library reads");
    let directory = crate::root::test_plugins::scratch("serve-production-663");
    let archive = crate::root::test_plugins::signed(
        &crate::dist_root_install::release_key(),
        manifest,
        &library,
    );
    std::fs::write(directory.join("store.tar.gz"), archive)
        .expect("stage the actual store archive");
    let settings = crate::root::test_plugins::application_store_settings(&directory);
    let deploy = busbar_kernel::config::deploy_from_deserializer(serde_json::json!({
        "providers": {},
        "models": {},
        "auth": { "chain": [], "admin_auth": [] },
        "store": { "module": module, "settings": settings },
        "advanced": { "allow_destinations": ["127.0.0.1"] },
    }))
    .expect("the door deployment parses");
    let config = busbar_kernel::config::resolve(&deploy, &Default::default())
        .expect("the door deployment resolves");
    let plugins = crate::root::test_plugins::trusted_config(&directory);
    let rules = std::sync::Arc::new(crate::root::door_steps::Rules::default())
        as std::sync::Arc<dyn busbar_kernel::appbuild::RootRules>;
    let (app, rotate, limits) = crate::root::serve::door_tests::build_process_app(
        config,
        plugins,
        None,
        Default::default(),
        Default::default(),
        (None, None),
        None,
        &rules,
    )
    .await
    .expect("the actual product builds the door deployment");
    limits.stage_boot();
    crate::root::serve::door_tests::attach_process_prices(&app)
        .await
        .expect("native process price history");
    limits.keep().await.expect("the product limits commit");
    if let Some(rotate) = rotate {
        rotate.await;
    }
    let app = std::sync::Arc::new(app);
    let dispatcher = Arc::new(Dispatcher::new(
        DispatchConfig::default(),
        busbar_kernel::host_services::caller_binding(),
    ));
    crate::root::connector::install_io(&dispatcher);

    let _ = dispatcher.lend_seal(busbar_kernel::seal::lend());
    let row = LinkedRow::of(decisions_door).expect("the door states its Statement");
    let plane = Arc::new(
        load_linked::<Plane>(
            &row,
            Bind {
                instance: Arc::from(instance),
                max_inflight_cap: 64,
                sink: Arc::new(NoSink),
                dispatcher: dispatcher.adopter(),

                conns: crate::root::loader::dispatch::ConnTable::Probe,
            },
        )
        .expect("the linked door binds"),
    );
    let section = plane.served().section;
    let mut sections = BTreeMap::new();

    sections.insert(
        section,
        serde_yaml::from_str(&format!(
            "models: {{m: {{provider: typesafe, protocol: {}}}}}\ncatalog: {{m: {{description: 'Busbar in-process Decisions test model (not a provider model)', release_date: '2026-11-09'}}}}",
            plane.served().dialects[0]
        ))
        .expect("a section"),
    );
    let empty_money = money().await;
    let mut served = compose_planes(
        &[(instance.to_string(), plane)],
        &dispatcher,
        &composed_services(),
        &sections,
        None,
        &|| Arc::clone(&empty_money),
        None,
        None,
        &app.serve_tables,
    )
    .await
    .expect("the door plane composes");

    let claims = &mut served.planes[0].snapshot.claims;
    let mut twin = claims[0].clone();
    twin.carrier = format!("{}-stream", twin.carrier);
    claims.push(twin);
    let doors = door_routes(served, || CARD.pin(), &[], &[]).expect("its claims mount");
    let mut seen = std::collections::BTreeSet::new();
    for door in &doors {
        assert!(
            seen.insert((door.path.clone(), door.method.as_str())),
            "{} {} is mounted once",
            door.method.as_str(),
            door.path
        );
    }
    assert!(
        seen.contains(&(CLAIMED.to_string(), "POST")),
        "the claim is a route: {seen:?}"
    );

    let (router, _admin, _handle) =
        busbar_kernel::build_split_routers_serving(app, doors, 1 << 20, 0, false);
    let refused = u16::try_from(refusal_status(ReasonCode::Unauthenticated)).expect("a status");
    assert_eq!(
        send(&router, CLAIMED, None).await.status().as_u16(),
        refused
    );
}

#[cfg(feature = "plane-decisions")]
pub(crate) struct Serving {
    pub(crate) router: axum::Router,
    pub(crate) token: String,

    pub(crate) handle: Arc<busbar_kernel::state::AppHandle>,

    pub(crate) live: Arc<super::DoorApply>,

    egress: Arc<busbar_kernel::plane_driver::Egress>,

    plane: String,

    book: Arc<std::sync::Mutex<crate::root::durability::Durability>>,

    gov: Arc<GovState>,
    key: String,

    app: Arc<busbar_kernel::state::App>,

    op_classes: Vec<&'static str>,
}

#[cfg(feature = "plane-decisions")]
impl Serving {
    fn lane(&self) -> String {
        format!("{}{PLANE_LANE_SEP}m", self.plane)
    }

    fn ready(&self) -> bool {
        (self.egress.breaker)().ready(
            &self.lane(),
            MEMBER,
            self.egress.clock.now_secs(),
            &busbar_contract::caps::Pass::mint(&busbar_kernel::seal::acquire()),
        )
    }

    fn cooldown(&self) -> u64 {
        (self.egress.breaker)().cooldown_remaining(
            &self.lane(),
            MEMBER,
            self.egress.clock.now_secs(),
            &busbar_contract::caps::Pass::mint(&busbar_kernel::seal::acquire()),
        )
    }

    async fn scrape(&self) -> String {
        use tower::ServiceExt as _;
        let req = axum::http::Request::builder()
            .method("GET")
            .uri("/metrics")
            .header("authorization", format!("Bearer {}", self.token))
            .body(axum::body::Body::empty())
            .expect("a request");
        let response = self
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("the router answers");
        assert_eq!(response.status(), StatusCode::OK, "the scrape is served");
        let body = axum::body::to_bytes(response.into_body(), 1 << 22)
            .await
            .expect("the exposition");
        String::from_utf8(body.to_vec()).expect("the exposition is UTF-8")
    }

    fn audit_records(&self) -> Vec<busbar_kernel_audit::AuditRecord> {
        self.book.lock().expect("unpoisoned").audit_records.clone()
    }
}

#[cfg(feature = "plane-decisions")]
const MEMBER: busbar_contract::DestinationId = busbar_contract::DestinationId::new(0);

#[cfg(feature = "plane-decisions")]
fn counted(exposition: &str, family: &str, lane: &str) -> f64 {
    let lane = format!("lane=\"{lane}\"");
    exposition
        .lines()
        .filter(|l| l.starts_with(&format!("{family}{{")) && l.contains(&lane))
        .filter_map(|l| l.rsplit_once(' ')?.1.parse::<f64>().ok())
        .sum()
}

#[cfg(feature = "plane-decisions")]
async fn serve_over(linked: &crate::root::linked::Linked, instance: &str, port: u16) -> Serving {
    serve_limited(linked, instance, port, Vec::new()).await
}

#[cfg(feature = "plane-decisions")]
async fn serve_limited(
    linked: &crate::root::linked::Linked,
    instance: &str,
    port: u16,
    limits: Vec<busbar_kernel::config::groups::LimitCfg>,
) -> Serving {
    serve_governed(linked, instance, port, limits, 0, None).await
}

#[cfg(feature = "plane-decisions")]
async fn serve_governed(
    linked: &crate::root::linked::Linked,
    instance: &str,
    port: u16,
    limits: Vec<busbar_kernel::config::groups::LimitCfg>,
    fee: i64,
    allowed_pools: Option<Vec<String>>,
) -> Serving {
    let key_file = std::env::temp_dir().join(format!(
        "busbar-serve-door-{instance}-{}",
        std::process::id()
    ));
    std::fs::write(&key_file, CREDENTIAL).expect("the credential file");
    let api_key = format!("{{file: '{}'}}", key_file.display());
    let serving = serve_configured(linked, instance, port, limits, fee, allowed_pools, &api_key)
        .await
        .unwrap_or_else(|e| panic!("the door plane composes, its egress sealed: {e}"));
    let _ = std::fs::remove_file(&key_file);
    serving
}

#[cfg(linked_section_decisions)]
pub(crate) async fn serve_keyed(
    linked: &crate::root::linked::Linked,
    instance: &str,
    port: u16,
    api_key: &str,
) -> Result<Serving, String> {
    serve_configured(linked, instance, port, Vec::new(), 0, None, api_key).await
}

#[cfg(linked_section_decisions)]
async fn serve_configured(
    linked: &crate::root::linked::Linked,
    instance: &str,
    port: u16,
    limits: Vec<busbar_kernel::config::groups::LimitCfg>,
    fee: i64,
    allowed_pools: Option<Vec<String>>,
    api_key: &str,
) -> Result<Serving, String> {
    crate::dist_root_install::install();
    busbar_kernel::snapshot::init_for_test(std::time::Duration::from_secs(10));
    let judge = crate::root::connector::guard_for(&busbar_kernel::config::Destinations {
        block_private_addresses: false,
        ..Default::default()
    })
    .expect("the guard");
    let connector = busbar_core_connector::process::build_with_secrets(
        || {
            crate::root::connector::entries(
                crate::LINKED_TRANSPORT_DOORS,
                &busbar_contract::transport::TransportSettings::default(),
            )
        },
        judge,
        &[],
        Arc::new(|_, _| {}),
        busbar_core_connector::pool::PoolPosture::NONE,
        busbar_core_connector::spawn_env::linked_spawn_secret,
    )
    .expect("the connector builds");
    let dispatcher = Arc::new(Dispatcher::new(
        DispatchConfig::default(),
        busbar_kernel::host_services::caller_binding(),
    ));
    crate::root::connector::install_io(&dispatcher);

    let _ = dispatcher.lend_seal(busbar_kernel::seal::lend());
    let row = LinkedRow::of(decisions_door).expect("the door states its Statement");
    let plane = Arc::new(
        load_linked::<Plane>(
            &row,
            Bind {
                instance: Arc::from(instance),
                max_inflight_cap: 64,
                sink: Arc::new(NoSink),
                dispatcher: dispatcher.adopter(),
                conns: crate::root::loader::dispatch::ConnTable::Host(
                    crate::root::connector::admit(Arc::clone(&connector) as Arc<dyn DeclaredConns>),
                ),
            },
        )
        .expect("the linked door binds"),
    );
    let section_key = plane.served().section;

    let dialect = plane.served().dialects[0];
    let op_classes = plane.served().op_classes;

    let signer = TokenSigner::from_secret_bytes(&[7u8; 32], DEFAULT_KID);
    let gov = Arc::new(
        GovState::new_with_core(
            crate::root::serve::door_tests::process_store().await,
            None,
            Some(signer),
        )
        .await
        .expect("governance"),
    );
    let group = format!("decider-{instance}");
    let groups = if limits.is_empty() {
        BTreeMap::new()
    } else {
        BTreeMap::from([(
            group.clone(),
            busbar_kernel::config::GroupCfg {
                limits,
                ..Default::default()
            },
        )])
    };
    let cost = if groups.is_empty() {
        CostModel::flat(1)
    } else {
        CostModel::resolve_parts(None, 1, &groups)
    };
    let cost = if fee == 0 {
        cost
    } else {
        cost.with_plane_fees(&busbar_kernel::config::PlaneFeesMap::from([(
            plane.name().to_string(),
            busbar_kernel_ledger::cost::PlaneFees {
                per_request: fee,
                per_session: 0,
            },
        )]))
    };
    let (key, token) = gov
        .mint_signed_async(
            NewKeySpec {
                name: "decider".to_string(),
                allowed_pools,
                group: (!groups.is_empty()).then(|| group.clone()),
                ..Default::default()
            },
            4_000_000_000,
            1_700_000_000,
        )
        .await
        .expect("mint");
    gov.hydrate_budgets_async(&cost, 0).await.expect("hydrate");
    let node = Arc::new(Node::new());
    let mut book = crate::root::durability::node_book(
        busbar_kernel::persistence::journal::runtime::RuntimeJournal::open(
            gov.core_store_handle().expect("native Store"),
            busbar_kernel::door::op_id().counter().max(1),
            busbar_kernel::persistence::journal::runtime::BufferBudget {
                records: 4096,
                bytes: 16 * 1024 * 1024,
            },
        )
        .await
        .expect("native journal"),
        busbar_kernel::persistence::journal::runtime::BufferBudget {
            records: 4096,
            bytes: 16 * 1024 * 1024,
        },
    )
    .await
    .expect("native node book");
    crate::root::serve::door_tests::bind_process_amendments(&mut book.durability, gov.amendments())
        .await;
    node.bind_book(Arc::clone(&book.durability));
    let post = Arc::new(NodeEndPost::new(Arc::clone(&node)));
    let site = Arc::clone(&post);
    let book_money = Arc::clone(&gov);
    let plane_money = move || {
        Arc::new(PlaneMoney::new(
            Arc::clone(&book_money),
            Arc::clone(&site) as Arc<dyn EndPost>,
        ))
    };

    let provider: busbar_kernel::config::ProviderCfg = serde_yaml::from_str(&format!(
        "{{protocol: {dialect}, base_url: 'http://127.0.0.1:{port}', api_key: {api_key}, error_map: {{}}}}",
    ))
    .expect("a provider entry");
    let providers = provider_routes(
        &std::collections::HashMap::from([(
            "typesafe".to_string(),
            busbar_kernel::config::ResolvedProvider::from(provider),
        )]),
        &busbar_kernel::config_validate::MetadataPosture::default(),
    );
    #[cfg(distribution_root)]
    crate::dist_root_install::install();
    let secrets: Arc<dyn busbar_contract::secret::SecretResolve> =
        Arc::new(busbar_kernel::config::secret::SecretResolver::builtins_only());
    let auths = OutboundAuths::new(
        Arc::clone(&dispatcher),
        crate::LINKED.auths,
        None,
        crate::root::loader::dispatch::ConnTable::Host(crate::root::connector::admit(Arc::clone(
            &connector,
        )
            as Arc<dyn DeclaredConns>)),
        busbar_kernel::diagnostics::Catalogue::NEUTRAL,
    );
    let reach = DoorReach {
        providers: &providers,
        secrets: Arc::clone(&secrets),
        auths: Arc::new(auths),
        conns: Arc::clone(&connector) as Arc<dyn PollConns>,
        stream_ceiling_secs: 600,
        catalog: None,
        models: None,
        upgrades: Vec::new(),
    };
    let mut sections = BTreeMap::new();
    sections.insert(
        section_key,
        serde_yaml::from_str("models: {m: {provider: typesafe}}\ncatalog: {m: {description: 'Busbar in-process Decisions test model (not a provider model)', release_date: '2026-11-09'}}").expect("a section"),
    );
    let mut served = compose_planes_over(
        linked,
        &[(instance.to_string(), plane)],
        &dispatcher,
        &composed_services(),
        &sections,
        None,
        &plane_money,
        Some(&DoorEgress {
            reach: &reach,
            journal: Arc::clone(&post) as Arc<dyn busbar_kernel_egress::ports::Journal>,
        }),
        None,
        &(Arc::new(crate::root::egress_ports::NodeClock::new())
            as Arc<dyn busbar_kernel_egress::ports::Clock>),
        &busbar_contract::config_time::ConfigScope::config_time(),
        &busbar_kernel::plane_driver::serve::ServeTables::default(),
    )
    .await?;
    let live = Arc::clone(&served.planes[0].live);
    served.post = Some(Arc::clone(&post));
    let composed = &served.planes[0];
    let egress = composed
        .live
        .current()
        .egress
        .clone()
        .expect("the composition sealed its egress");
    let plane = composed.facts.plane.clone();
    crate::dist_root_install::install();
    let library_path = crate::root::test_plugins::application_store_library();
    let statement = crate::root::loader::dispatch::rendering_of_library(&library_path)
        .expect("the actual store statement reads")
        .expect("the actual store exports its statement");
    let module = busbar_contract::abi::mechanism::rendering::read(&statement)
        .expect("the actual store statement is valid")
        .name;
    let mut manifest = crate::root::test_plugins::manifest(
        "store",
        &module,
        crate::root::test_plugins::FIXTURE_PUBLISHER,
    );
    manifest.statement = Some(hex::encode(statement));
    let library = std::fs::read(library_path).expect("the actual store library reads");
    let directory = crate::root::test_plugins::scratch("serve-production-5097");
    let archive = crate::root::test_plugins::signed(
        &crate::dist_root_install::release_key(),
        manifest,
        &library,
    );
    std::fs::write(directory.join("store.tar.gz"), archive)
        .expect("stage the actual store archive");
    let settings = crate::root::test_plugins::application_store_settings(&directory);
    let signing_file = directory.join("signing.key");
    std::fs::write(&signing_file, [7u8; 32]).expect("the actual token signing secret");
    let deploy = busbar_kernel::config::deploy_from_deserializer(serde_json::json!({
        "providers": {},
        "models": {},
        "groups": groups,
        "auth": { "chain": ["keys"], "admin_auth": [], "signing_key": {"file": signing_file} },
        "store": { "module": module, "settings": settings },
        "advanced": { "allow_destinations": ["127.0.0.1"] },
    }))
    .expect("the door deployment parses");
    let mut config = busbar_kernel::config::resolve(&deploy, &Default::default())
        .expect("the door deployment resolves");
    config.plane_fees.insert(
        plane.clone(),
        busbar_kernel_ledger::cost::PlaneFees {
            per_request: fee,
            per_session: 0,
        },
    );
    let plugins = crate::root::test_plugins::trusted_config(&directory);
    let rules = std::sync::Arc::new(crate::root::door_steps::Rules::default())
        as std::sync::Arc<dyn busbar_kernel::appbuild::RootRules>;
    let (mut app, rotate, limits) = crate::root::serve::door_tests::build_process_app(
        config,
        plugins,
        None,
        Default::default(),
        Default::default(),
        (None, None),
        None,
        &rules,
    )
    .await
    .expect("the actual product builds the door deployment");
    limits.stage_boot();
    crate::root::serve::door_tests::attach_process_prices(&app)
        .await
        .expect("native process price history");
    limits.keep().await.expect("the product limits commit");
    if let Some(rotate) = rotate {
        rotate.await;
    }
    app.amendments = Arc::clone(gov.amendments());
    app.governance = Some(Arc::clone(&gov));
    app.cost = Arc::new(cost);
    app.plugin_routes = metrics_routes();
    let app = std::sync::Arc::new(app);
    let doors = door_routes(served, || CARD.pin(), &[], &[]).expect("its claims mount");
    let (router, _admin, handle) =
        busbar_kernel::build_split_routers_serving(Arc::clone(&app), doors, 1 << 20, 0, false);
    Ok(Serving {
        router,
        token: token.expose(&busbar_kernel::seal::lend()).to_string(),
        handle,
        live,
        egress,
        plane,
        book: Arc::clone(&book.durability),
        gov,
        key: key.id.to_string(),
        app,
        op_classes,
    })
}

#[cfg(feature = "plane-decisions")]
const STATES_NO_BENCH: &str = r#"{"breaker":{"bench_below_trip_threshold":false}}"#;

#[cfg(feature = "plane-decisions")]
fn declaring(declares: &'static str) -> crate::root::linked::Linked {
    crate::root::linked::Linked {
        plane_door_declares: vec![(
            "busbar-plane-decisions",
            decisions_door as busbar_contract::abi::mechanism::door::DoorFn,
            declares,
        )]
        .leak(),
        ..crate::LINKED
    }
}

#[cfg(feature = "plane-decisions")]
pub(crate) fn decisions_name() -> String {
    let row = LinkedRow::of(decisions_door).expect("the door states its Statement");
    busbar_contract::abi::mechanism::rendering::read(&row.statement)
        .expect("its Statement reads")
        .name
}

#[cfg(feature = "plane-decisions")]
async fn call_once(
    serving: &Serving,
    heard: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
) -> (StatusCode, Option<String>) {
    let status = send(&serving.router, CLAIMED, Some(&serving.token))
        .await
        .status();
    (status, heard.try_recv().ok())
}

#[cfg(feature = "plane-decisions")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plane_stating_no_bench_below_the_trip_keeps_its_sole_member_through_one_503() {
    let _one = PUBLISHING.lock().await;
    let instance = "serve-door-breaker-stated";
    let linked = declaring(STATES_NO_BENCH);
    assert_eq!(
        crate::root::linked::door_breaker(&linked, None, &decisions_name())
            .expect("the fact reads"),
        Some(crate::root::loader::sign::BreakerDecl {
            bench_below_trip_threshold: false
        }),
        "the root reads the fact off the door's declares, by its Statement name"
    );
    let with_needs =
        declaring(r#"{"needs":["scheme-a"],"breaker":{"bench_below_trip_threshold":false}}"#);
    assert_eq!(
        crate::root::linked::door_breaker(&with_needs, None, &decisions_name())
            .expect("the fact reads beside `needs`"),
        Some(crate::root::loader::sign::BreakerDecl {
            bench_below_trip_threshold: false
        })
    );
    let malformed = declaring(r#"{"needs":"scheme-a"}"#);
    let refused = crate::root::linked::door_breaker(&malformed, None, &decisions_name())
        .expect_err("a `needs` that is not a list");
    let row = malformed.plane_door_declares[0].0;
    assert!(refused.contains(row), "{refused}");
    let (port, mut heard) = far_end_failing_first(1).await;
    let serving = serve_over(&linked, instance, port).await;

    let (status, reached) = call_once(&serving, &mut heard).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "the far end's one transient failure"
    );
    assert!(reached.is_some(), "the failing call reached the far end");

    let (status, reached) = call_once(&serving, &mut heard).await;
    assert!(
        reached.is_some(),
        "one 503 below the trip benches nothing: the next call reaches the far end"
    );
    assert_eq!(status, StatusCode::OK, "and is served");
}

#[cfg(feature = "plane-decisions")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plane_stating_no_breaker_fact_keeps_the_default_bench() {
    let _one = PUBLISHING.lock().await;
    let instance = "serve-door-breaker-absent";

    let silent = declaring("{}");
    assert_eq!(
        crate::root::linked::door_breaker(&silent, None, &decisions_name()).expect("it reads"),
        None
    );
    assert_eq!(
        crate::root::linked::door_breaker(&crate::LINKED, None, &decisions_name())
            .expect("nothing to read"),
        None,
        "no linked door states the fact for this plane in this build"
    );

    let (port, mut heard) = far_end_failing_first(1).await;
    let serving = serve_over(&silent, instance, port).await;
    let (status, reached) = call_once(&serving, &mut heard).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "the far end's one transient failure"
    );
    assert!(reached.is_some(), "the failing call reached the far end");

    let (status, reached) = call_once(&serving, &mut heard).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "the benched member's call is refused"
    );
    assert_eq!(
        reached, None,
        "under the default one 503 benches the sole member: the next call never reaches the far end"
    );
}

#[cfg(feature = "plane-decisions")]
const MAX_FAILURES: usize = 8;

#[cfg(feature = "plane-decisions")]
const CALLER_FAULT: &str = r#"{"error":"the state names no amount"}"#;

#[cfg(feature = "plane-decisions")]
async fn trip(
    serving: &Serving,
    heard: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
) -> usize {
    for n in 1..=MAX_FAILURES {
        let (status, reached) = call_once(serving, heard).await;
        assert_ne!(status, StatusCode::OK, "failure {n} is not served");
        assert!(reached.is_some(), "failure {n} reached the far end");
        if serving.cooldown() > 0 {
            return n;
        }
    }
    panic!("{MAX_FAILURES} failures in a row never opened the member's breaker cell");
}

#[cfg(feature = "plane-decisions")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_decisions_provider_records_into_its_breaker_cell_until_the_cell_opens() {
    let _one = PUBLISHING.lock().await;
    let instance = "serve-door-breaker-trip";
    let (port, mut heard) = far_end_failing_first(MAX_FAILURES).await;
    let serving = serve_over(&declaring(STATES_NO_BENCH), instance, port).await;
    assert!(serving.ready(), "the member's cell starts closed");
    let trips = counted(
        &serving.scrape().await,
        busbar_kernel::snapshot::BREAKER_TRIPS_TOTAL,
        &serving.lane(),
    );

    let failures = trip(&serving, &mut heard).await;
    assert!(
        failures > 1,
        "the cell recorded failures below its threshold before it opened, not one: {failures}"
    );
    assert!(!serving.ready(), "the opened cell takes no request");
    assert_eq!(
        counted(
            &serving.scrape().await,
            busbar_kernel::snapshot::BREAKER_TRIPS_TOTAL,
            &serving.lane(),
        ),
        trips + 1.0,
        "one fresh trip, counted under the model's lane"
    );
}

#[cfg(feature = "plane-decisions")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tripped_decisions_provider_is_refused_before_the_dial_at_once() {
    let _one = PUBLISHING.lock().await;
    let instance = "serve-door-breaker-fastfail";
    let holding = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let held = Arc::clone(&holding);
    let (port, mut heard) = far_end_answering(Arc::new(move |_| {
        if held.load(std::sync::atomic::Ordering::SeqCst) {
            Answer::Holds
        } else {
            Answer::Says("503 Service Unavailable", UNAVAILABLE)
        }
    }))
    .await;
    let serving = serve_over(&declaring(STATES_NO_BENCH), instance, port).await;
    trip(&serving, &mut heard).await;

    holding.store(true, std::sync::atomic::Ordering::SeqCst);
    let started = std::time::Instant::now();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        send(&serving.router, CLAIMED, Some(&serving.token)),
    )
    .await
    .expect("the tripped member's unit is answered, not held at its far end");
    let took = started.elapsed();
    assert_ne!(response.status(), StatusCode::OK, "the unit is refused");
    assert!(
        heard.try_recv().is_err(),
        "the refused unit never reached the far end"
    );
    assert!(
        took < std::time::Duration::from_secs(1),
        "refused before the dial, in milliseconds: {took:?}"
    );
}

#[cfg(feature = "plane-decisions")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_decisions_providers_answers_are_classified_before_the_plane_relays_them() {
    let _one = PUBLISHING.lock().await;
    let instance = "serve-door-disposition";
    let (port, mut heard) = far_end_answering(Arc::new(|n| match n {
        0 | 1 => Answer::Says("400 Bad Request", CALLER_FAULT),
        2 => Answer::Says("503 Service Unavailable", UNAVAILABLE),
        _ => Answer::Says("200 OK", ANSWER),
    }))
    .await;

    let serving = serve_over(&crate::LINKED, instance, port).await;
    let failures = || async {
        counted(
            &serving.scrape().await,
            busbar_kernel::telemetry::UPSTREAM_FAILURES_TOTAL,
            &serving.lane(),
        )
    };
    let before = failures().await;

    for n in 1..=2 {
        let response = send(&serving.router, CLAIMED, Some(&serving.token)).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "caller fault {n} is relayed with the far end's status"
        );
        let body = axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .expect("the body");
        assert_eq!(
            &body[..],
            CALLER_FAULT.as_bytes(),
            "caller fault {n} is relayed as the far end said it"
        );
        assert!(
            heard.try_recv().is_ok(),
            "caller fault {n} reached the far end"
        );
        assert!(serving.ready(), "a caller fault benches nothing ({n})");
    }
    assert_eq!(
        failures().await,
        before,
        "a caller fault is not the destination's: no upstream failure is counted"
    );

    let response = send(&serving.router, CLAIMED, Some(&serving.token)).await;
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .expect("the body");
    assert!(
        heard.try_recv().is_ok(),
        "the transient failure reached the far end"
    );
    assert!(
        status.is_server_error(),
        "the walk's terminal answers the transient failure: {status}"
    );
    assert_ne!(
        &body[..],
        UNAVAILABLE.as_bytes(),
        "a transient failure is failed over, never relayed"
    );
    assert!(!serving.ready(), "the transient failure benches the member");
    assert_eq!(
        failures().await,
        before + 1.0,
        "the transient failure is counted once under the model's lane"
    );
}

#[cfg(feature = "plane-decisions")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_decisions_unit_served_or_refused_seals_one_record_on_one_tamper_evident_chain() {
    let _one = PUBLISHING.lock().await;
    let instance = "serve-door-audit-chain";
    let (port, mut heard) = far_end().await;
    let one_an_hour = busbar_kernel::config::groups::LimitCfg {
        metric: busbar_kernel::config::groups::LimitMetric::Requests,
        amount: 1,
        per: Some(busbar_kernel::config::groups::LimitWindow::Hour),
        scope: None,
        on_exhaust: None,
        downgrade_to: None,
        admission: None,
        on_exhaustion: None,
    };
    let serving = serve_limited(&crate::LINKED, instance, port, vec![one_an_hour]).await;

    let call = || async {
        let response = send(&serving.router, CLAIMED, Some(&serving.token)).await;
        let status = response.status();
        let _ = axum::body::to_bytes(response.into_body(), 1 << 16)
            .await
            .expect("the body");
        status
    };

    assert_eq!(call().await, StatusCode::OK, "the first unit is served");
    assert!(heard.try_recv().is_ok(), "and dispatched");
    assert_ne!(
        call().await,
        StatusCode::OK,
        "the second is refused by the limit"
    );
    assert!(heard.try_recv().is_err(), "before its dial");

    let records = serving.audit_records();
    assert_eq!(records.len(), 2, "one record per unit: {records:?}");
    let (served, refused) = (&records[0], &records[1]);
    for record in &records {
        assert_eq!(
            record.what.op_class.as_str(),
            serving.op_classes[0],
            "sealed under the plane's operation class: {record:?}"
        );
    }
    assert_eq!(
        served.outcome.unit_end,
        busbar_contract::caps::Outcome::Completed
    );
    assert!(
        matches!(
            refused.outcome.unit_end,
            busbar_contract::caps::Outcome::Refused(_, ReasonCode::RateLimited)
        ),
        "the refusal is chained as one: {:?}",
        refused.outcome
    );
    assert_eq!(refused.seq, served.seq + 1, "contiguous");
    assert_eq!(
        refused.prev_hash, served.hash,
        "linked to the record before"
    );
    assert!(busbar_kernel_audit::AuditChain::verify_window(&records).is_ok());
    assert!(
        serving
            .book
            .lock()
            .expect("unpoisoned")
            .retained_audit_findings()
            .is_empty(),
        "the node's own verify finds nothing"
    );

    let mut forged = records.clone();
    forged[0].outcome.unit_end = refused.outcome.unit_end;
    assert!(
        busbar_kernel_audit::AuditChain::verify_window(&forged).is_err(),
        "a record altered after its seal breaks the chain"
    );
}

#[cfg(feature = "plane-decisions")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_served_decisions_unit_counts_its_upstream_attempt_under_the_planes_lane() {
    let _one = PUBLISHING.lock().await;
    let instance = "serve-door-metrics";
    let (port, mut heard) = far_end().await;
    let serving = serve_over(&crate::LINKED, instance, port).await;
    let lane = serving.lane();
    assert!(lane.starts_with(&serving.plane), "{lane}");
    let before = counted(
        &serving.scrape().await,
        busbar_kernel::telemetry::UPSTREAM_ATTEMPTS_TOTAL,
        &lane,
    );

    let (status, reached) = call_once(&serving, &mut heard).await;
    assert_eq!(status, StatusCode::OK, "served");
    assert!(reached.is_some(), "dispatched");

    let exposition = serving.scrape().await;
    assert_eq!(
        counted(
            &exposition,
            busbar_kernel::telemetry::UPSTREAM_ATTEMPTS_TOTAL,
            &lane
        ),
        before + 1.0,
        "one attempt, counted under the plane's lane:\n{exposition}"
    );
    let pool = format!("pool=\"{lane}\"");
    assert!(
        exposition.lines().any(|l| l.starts_with(&format!(
            "{}{{",
            busbar_kernel::telemetry::UPSTREAM_ATTEMPTS_TOTAL
        )) && l.contains(&pool)),
        "its pool label is the plane's too:\n{exposition}"
    );
}

#[cfg(feature = "plane-decisions")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_decisions_provider_is_handed_the_planned_credential_and_never_the_callers() {
    use tower::ServiceExt as _;
    const CALLER_SECRET: &str = "caller-own-provider-key";
    let _one = PUBLISHING.lock().await;
    let instance = "serve-door-egress-auth";
    let (port, mut heard) = far_end().await;
    let serving = serve_over(&crate::LINKED, instance, port).await;

    let req = axum::http::Request::builder()
        .method("POST")
        .uri(CLAIMED)
        .header("authorization", format!("Bearer {}", serving.token))
        .header("api-key", CALLER_SECRET)
        .header("x-api-key", CALLER_SECRET)
        .body(axum::body::Body::from(r#"{"state":{"amount":7}}"#))
        .expect("a request");
    let response = serving
        .router
        .clone()
        .oneshot(req)
        .await
        .expect("the router answers");
    assert_eq!(response.status(), StatusCode::OK, "served");

    let head = heard.recv().await.expect("the far end was dialled");
    let presented: Vec<&str> = head
        .lines()
        .filter(|l| {
            l.split_once(':')
                .is_some_and(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        })
        .collect();
    assert_eq!(presented.len(), 1, "one credential is presented: {head}");
    assert!(
        presented[0].eq_ignore_ascii_case(&format!("authorization: Bearer {CREDENTIAL}")),
        "the planned credential, in its style: {head}"
    );
    assert!(
        !head.contains(&serving.token),
        "the caller's bearer never reaches the provider: {head}"
    );
    assert!(
        !head.contains(CALLER_SECRET),
        "nor a key the caller sent: {head}"
    );
}

#[cfg(feature = "plane-decisions")]
fn budget_of(
    amount: u64,
    per: busbar_kernel::config::groups::LimitWindow,
) -> busbar_kernel::config::groups::LimitCfg {
    busbar_kernel::config::groups::LimitCfg {
        metric: busbar_kernel::config::groups::LimitMetric::Budget,
        amount,
        per: Some(per),
        scope: None,
        on_exhaust: None,
        downgrade_to: None,
        admission: None,
        on_exhaustion: None,
    }
}

#[cfg(feature = "plane-decisions")]
async fn call_through(
    serving: &Serving,
    heard: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
) -> (StatusCode, Option<String>, bool) {
    let response = send(&serving.router, CLAIMED, Some(&serving.token)).await;
    let status = response.status();
    let wait = response
        .headers()
        .get("retry-after")
        .map(|v| v.to_str().expect("a header value").to_string());
    let _ = axum::body::to_bytes(response.into_body(), 1 << 16)
        .await
        .expect("the body");
    (status, wait, heard.try_recv().is_ok())
}

#[cfg(feature = "plane-decisions")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_decisions_unit_past_its_budget_is_refused_with_a_retry_after() {
    const EXACT: &str = concat!(
        module_path!(),
        "::a_decisions_unit_past_its_budget_is_refused_with_a_retry_after"
    );
    let Some(case) = crate::root::serve::door_tests::independent_cases(EXACT, &["hour", "total"])
    else {
        return;
    };
    use busbar_kernel::config::groups::LimitWindow;
    let _one = PUBLISHING.lock().await;
    if case == "hour" {
        let instance = "serve-door-budget";
        let (port, mut heard) = far_end().await;
        let serving = serve_governed(
            &crate::LINKED,
            instance,
            port,
            vec![budget_of(1, LimitWindow::Hour)],
            1,
            None,
        )
        .await;

        let (status, wait, reached) = call_through(&serving, &mut heard).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "within its budget the unit is served"
        );
        assert!(reached, "and dispatched");
        assert_eq!(wait, None, "a served unit names no wait");
        let usage = serving
            .gov
            .usage_for_async(&serving.app.cost, &serving.key, busbar_kernel::store::now())
            .await
            .expect("a read")
            .expect("the key exists");
        assert_eq!(usage.requests, 1, "its spend is the presenting key's");

        let (status, wait, reached) = call_through(&serving, &mut heard).await;
        assert_eq!(
            status,
            StatusCode::TOO_MANY_REQUESTS,
            "past its budget the unit is refused"
        );
        assert!(!reached, "before its dial");
        let secs: u64 = wait
            .expect("the refusal names its wait")
            .parse()
            .expect("whole seconds");
        assert!(
            (1..=3600).contains(&secs),
            "the seconds until the hour's window rolls: {secs}"
        );
        drop(serving);
        crate::root::serve::door_tests::independent_case_done(EXACT, &case);
    }

    if case == "total" {
        let instance = "serve-door-budget-total";
        let (port, mut heard) = far_end().await;
        let serving = serve_governed(
            &crate::LINKED,
            instance,
            port,
            vec![budget_of(1, LimitWindow::Total)],
            1,
            None,
        )
        .await;
        assert_eq!(call_through(&serving, &mut heard).await.0, StatusCode::OK);
        let (status, wait, reached) = call_through(&serving, &mut heard).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert!(!reached);
        assert_eq!(wait, None, "a total never rolls: no wait is named");
        crate::root::serve::door_tests::independent_case_done(EXACT, &case);
    }
}

#[cfg(feature = "plane-decisions")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_decisions_unit_from_a_key_naming_another_pool_is_refused_before_its_dial() {
    let _one = PUBLISHING.lock().await;
    let instance = "serve-door-verify";
    let (port, mut heard) = far_end().await;
    let other = Some(vec!["elsewhere".to_string()]);
    let serving = serve_governed(&crate::LINKED, instance, port, Vec::new(), 0, other).await;
    let (status, _wait, reached) = call_through(&serving, &mut heard).await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a pool the key does not name is not served"
    );
    assert!(!reached, "the far end is never dialled");
    let records = serving.audit_records();
    assert_eq!(records.len(), 1, "the refusal is sealed: {records:?}");
    assert!(
        matches!(
            records[0].outcome.unit_end,
            busbar_contract::caps::Outcome::Refused(
                busbar_contract::caps::StepName::Approve,
                ReasonCode::ScopeDenied
            )
        ),
        "refused on its pool grant, before admit: {:?}",
        records[0].outcome
    );
    drop(serving);

    let instance = "serve-door-verify-control";
    let (port, mut heard) = far_end().await;
    let serving = serve_governed(&crate::LINKED, instance, port, Vec::new(), 0, None).await;
    let (status, _wait, reached) = call_through(&serving, &mut heard).await;
    assert_eq!(status, StatusCode::OK, "the granted key is served");
    assert!(reached, "and dispatched");
}

#[cfg(feature = "plane-decisions")]
#[test]
fn the_decisions_doors_facing_lists_its_configured_models() {
    let reg = crate::root::loader::dispatch::kinds::plane::registration(
        crate::root::loader::dispatch::kinds::plane::linked_probe(
            decisions_door,
            "decisions-listed",
            busbar_kernel::host_services::caller_binding(),
        ),
    )
    .expect("the linked door binds");
    let facing = (reg.facing)(

        br#"{"decisions":{"models":{"b":{"provider":"typesafe","protocol":"jev"},"a":{"provider":"typesafe","protocol":"jev"}},"catalog":{"a":{"description":"Busbar listed-model test fixture a (not a provider model)","release_date":"2026-11-09"},"b":{"description":"Busbar listed-model test fixture b (not a provider model)","release_date":"2026-11-09"}}}}"#,
        b"",
        None,
        &[],
    )
    .expect("the door faces the world");
    assert_eq!(facing.listed, vec!["a".to_string(), "b".to_string()]);
    let none = (reg.facing)(b"{}", b"", None, &[]).expect("the door faces the world");
    assert!(
        none.listed.is_empty(),
        "no models, no names: {:?}",
        none.listed
    );
}

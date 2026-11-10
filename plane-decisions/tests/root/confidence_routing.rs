// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! Actual LLM → dropped complexity → Decisions composition. Synthetic provider responses prove
//! custody and routing mechanics, not classifier quality.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::{Json, Router};
use busbar_contract::conn::{DeclaredConns, PollConns};
use busbar_kernel::governance::NewKeySpec;
use busbar_kernel::plane_driver::{EndPost, PlaneMoney};
use busbar_kernel::state::AppHandle;
use busbar_plugin_loader::dispatch::kinds::plane::Plane;
use busbar_plugin_loader::dispatch::{load_linked, Bind, ConnTable, LinkedRow, NoSink};
use serde_json::{json, Value};
use tower::ServiceExt as _;

use crate::root::test_plugins as plugins;

fn isolated_case() -> bool {
    const CHILD: &str = "BUSBAR_CONFIDENCE_ROUTING_CHILD";
    const WITNESS: &str = "actual confidence routing assertions completed";
    let name = std::thread::current()
        .name()
        .expect("the exact test is named")
        .to_owned();
    if std::env::var(CHILD).ok().as_deref() == Some(name.as_str()) {
        return false;
    }
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", &name, "--nocapture"])
        .env(CHILD, &name)
        .stdout(std::process::Stdio::piped());
    let mut settings: Value = serde_json::from_str(
        &std::env::var("BUSBAR_GOVERNANCE_TEST_STORE_SETTINGS").unwrap_or_else(|_| "{}".into()),
    )
    .expect("actual Store settings");
    if settings.get("db_path").is_some() {
        let directory = plugins::scratch("decisions-child-store");
        settings["db_path"] = json!(directory.join("governance.sqlite"));
        command.env(
            "BUSBAR_GOVERNANCE_TEST_STORE_SETTINGS",
            settings.to_string(),
        );
    }
    let mut child = Child(
        command
            .spawn()
            .expect("start the exact isolated Decisions case"),
    );
    let until = std::time::Instant::now() + std::time::Duration::from_secs(90);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            use std::io::Read;
            let mut output = String::new();
            child
                .0
                .stdout
                .take()
                .unwrap()
                .read_to_string(&mut output)
                .unwrap();
            assert!(
                status.success(),
                "the actual Decisions child failed: {status}: {output}"
            );
            assert!(
                output.contains(WITNESS),
                "the exact child did not complete its assertions: {output}"
            );
            return true;
        }
        assert!(
            std::time::Instant::now() < until,
            "the actual Decisions child timed out"
        );
        std::thread::park_timeout(std::time::Duration::from_millis(5));
    }
}

fn required_artifact(variable: &str) -> PathBuf {
    let path =
        PathBuf::from(std::env::var_os(variable).unwrap_or_else(|| {
            panic!("{variable} must name the freshly built actual product cdylib")
        }));
    assert!(path.is_absolute(), "{variable} must be absolute");
    assert!(
        path.is_file(),
        "{variable} artifact missing: {}",
        path.display()
    );
    path
}

fn stage(directory: &Path, file: &str, variable: &str, kind: &str) -> (String, String) {
    let path = if kind == "store" {
        plugins::application_store_library()
    } else {
        required_artifact(variable)
    };
    let statement = busbar_plugin_loader::dispatch::rendering_of_library(&path)
        .expect("the actual product statement reads")
        .expect("the actual product exports its statement");
    let facts = busbar_contract::abi::mechanism::rendering::read(&statement)
        .expect("the actual product statement is valid");
    let mut manifest = plugins::manifest(kind, &facts.name, "third-party");
    manifest.version = facts.version;
    manifest.statement = Some(hex::encode(statement));
    if kind == "hook" {
        assert_eq!(
            facts.name, "busbar-hook-complexity",
            "the supplied hook must be the actual complexity product"
        );

        manifest.needs = busbar_plugin_loader::sign::HookNeeds {
            prompt: busbar_plugin_loader::sign::NeedLevel::Ro,
            user: busbar_plugin_loader::sign::NeedLevel::No,
        };
    }
    let library = std::fs::read(path).expect("read the actual product cdylib");
    let archive = plugins::signed(&plugins::key(0x33), manifest, &library);
    let hash = busbar_contract::redacted::sha256_hex(&archive);
    std::fs::write(directory.join(file), archive).expect("stage the signed actual product");
    (facts.name, hash)
}

#[derive(Default)]
struct Upstream {
    requests: Mutex<Vec<(String, Value)>>,
}

async fn provider() -> (u16, Arc<Upstream>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let upstream = Arc::new(Upstream::default());
    let state = upstream.clone();
    let routes = Router::new().fallback(move |request: Request<Body>| {
        let state = state.clone();
        async move {
            assert_eq!(
                request.headers().get("authorization").and_then(|v|v.to_str().ok()),
                Some("Bearer fixture-provider-credential"),
                "the host applies the configured upstream credential on both parent and child"
            );
            let path = request.uri().path().to_owned();
            let bytes = axum::body::to_bytes(request.into_body(), 65_536).await.unwrap();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            state.requests.lock().unwrap().push((path.clone(), body.clone()));
            if path == "/v1/systemone" {
                let text = body["state"].to_string();
                let mut answer = json!({"type":"choice","choice":"easy","confidence":0.8});
                if text.contains("missing-confidence") {
                    answer.as_object_mut().unwrap().remove("confidence");
                } else if text.contains("unknown-label") {
                    answer["choice"] = json!("not-a-tier");
                }
                (StatusCode::OK, Json(json!({"answers":{"complexity":answer},"usage":{"units":3},"private":"not-relayed"})))
            } else {
                assert_eq!(path, "/v1/chat/completions");
                let model = body["model"].as_str().unwrap();
                assert_ne!(model, "outside", "the classifier cannot create an authorized candidate");
                if model == "small" && body["messages"].to_string().contains("fail-small") {
                    return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error":{"message":"fixture unavailable","type":"server_error"}})));
                }
                (StatusCode::OK, Json(json!({"id":"chat-fixture","object":"chat.completion","created":1,"model":model,"choices":[{"index":0,"message":{"role":"assistant","content":format!("served-{model}")},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":7,"total_tokens":12}})))
            }
        }
    });
    let task = tokio::spawn(async move {
        axum::serve(listener, routes).await.unwrap();
    });
    (port, upstream, task)
}

struct Served {
    router: Router,
    token: String,
    upstream: Arc<Upstream>,
    _handle: Arc<AppHandle>,
    book: crate::root::durability::NodeBook,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Served {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn hook_config(module: &str, prompt: &str, signals: Value) -> Value {
    json!({"kind":"gate","module":module,"timeout_ms":5000,"on_error":"weighted",
    "prompt":prompt,"user":"no","signals":signals,
    "child_dispatch":{"revision":1,"plane":"confidence-decisions","operation":"POST",
        "target":"/busbar/decisions/v1/classify","target_binding":"classifier",
        "max_input_bytes":16384,"max_output_bytes":65536,"max_children":1,
        "timeout_ms":2000,"max_budget_micros":1000000},
    "settings":{"classifier_model":"classifier","previous_messages":1,"tiers":[
        {"label":"easy","description":"Simple fixture text","candidates":["outside","small"]},
        {"label":"hard","description":"Complex fixture text","candidates":["large"]}
    ]}})
}

async fn served() -> Served {
    crate::dist_root_install::install();
    let (provider_port, upstream, task) = provider().await;
    let directory = plugins::scratch("confidence-routing");
    let (store, store_pin) = stage(
        &directory,
        "store.tar.gz",
        "BUSBAR_TEST_SQLITE_CDYLIB",
        "store",
    );
    let (complexity, _) = stage(
        &directory,
        "complexity.tar.gz",
        "BUSBAR_TEST_COMPLEXITY_CDYLIB",
        "hook",
    );
    let signing_key = directory.join("signing.key");
    let provider_key = directory.join("provider.key");
    std::fs::write(&signing_key, hex::encode([9u8; 32])).unwrap();
    std::fs::write(&provider_key, "fixture-provider-credential").unwrap();
    let document = json!({
        "store":{"module":store,"settings":plugins::application_store_settings(&directory)},
        "providers":{"typesafe":{"api_key":{"file":provider_key}},"text":{"api_key":{"file":provider_key}}},
        "models":{"large":{"provider":"text"},"small":{"provider":"text"},"outside":{"provider":"text"}},
        "pools":{
            "classified":{"members":["large","small"],"weights":{"large":1000,"small":1},"hooks":["confidence"],"failover":{"max_hops":3}},
            "private":{"members":["large","small"],"weights":{"large":1000,"small":1},"hooks":["no-prompt"]},
            "unattested":{"members":["large","small"],"weights":{"large":1000,"small":1},"hooks":["no-signal"]}
        },
        "hooks":{
            "confidence":hook_config(&complexity,"ro",json!(["request_text_complete"])),
            "no-prompt":hook_config(&complexity,"no",json!(["request_text_complete"])),
            "no-signal":hook_config(&complexity,"ro",json!([]))
        },
        "decisions":{
            "models":{"classifier":{"provider":"typesafe"}}
        },
        "auth":{"chain":["keys"],"signing_key":{"file":signing_key},"admin_auth":[]},
        "advanced":{"allow_destinations":["127.0.0.1"]}
    });
    let deploy = busbar_kernel::config::deploy_from_deserializer(document)
        .expect("confidence deployment parses");
    let definitions = std::collections::HashMap::from([
        (
            "typesafe".to_owned(),
            serde_json::from_value(
                json!({"protocol":"jev","base_url":format!("http://127.0.0.1:{provider_port}")}),
            )
            .unwrap(),
        ),
        (
            "text".to_owned(),
            serde_json::from_value(
                json!({"protocol":"openai","base_url":format!("http://127.0.0.1:{provider_port}")}),
            )
            .unwrap(),
        ),
    ]);
    let cfg = busbar_kernel::config::resolve(&deploy, &definitions)
        .expect("actual decisions deployment resolves");
    let providers = crate::root::door_steps::provider_routes(
        &cfg.providers,
        &busbar_kernel::config_validate::MetadataPosture::default(),
    );
    let model_pools = crate::root::model_egress::ModelPools::of(&cfg);
    let mut sections: BTreeMap<_, _> = deploy.door_sections();
    for (key, value) in crate::root::door_steps::kernel_sections(&cfg).unwrap() {
        sections.entry(key).or_insert(value);
    }
    let sections =
        crate::root::serve::with_pools(sections, &crate::root::serve::door_pools(&cfg.plane_pools));
    let judge = crate::root::connector::dest_judge(&cfg);
    let late = crate::root::serve::LateServices::new();
    let (credentials, credential_handle) = crate::root::credentials::AppCredentials::late();
    crate::root::serve::compose(judge, &late, credentials);
    let dispatcher = Arc::new(crate::root::dispatch::build(2, &late));
    crate::root::connector::install_io(&dispatcher);
    let mut plugins = busbar_kernel::config::PluginsCfg {
        enabled: true,
        dir: directory.to_string_lossy().into_owned(),
        fetch: serde_json::from_value(
            json!([{"url":"https://plugins.example/store.tar.gz","sha256":store_pin}]),
        )
        .expect("actual Store archive pin"),
        ..Default::default()
    };
    plugins
        .trust
        .publishers
        .push(busbar_kernel::config::PluginPublisher {
            name: "third-party".into(),
            public_key: hex::encode(plugins::key(0x33).verifying_key().as_bytes()),
        });
    let rules = Arc::new(crate::root::door_steps::Rules::default())
        as Arc<dyn busbar_kernel::appbuild::RootRules>;
    let (app, rotate, limits) = crate::root::real_store::build_app(
        cfg,
        plugins,
        None,
        HashSet::new(),
        HashSet::new(),
        (None, None),
        None,
        &rules,
    )
    .await
    .expect("the production root builds the decisions application");
    limits.stage_boot();
    let book = crate::root::boot::book(&app, &store)
        .await
        .expect("the actual Store binds root card history before activation");
    limits
        .keep()
        .await
        .expect("commit actual application limits");
    let ctx = crate::root::linked::BookCtx {
        book: &book,
        app: &app,
    };
    for step in crate::ROOT_UNITS.iter().filter_map(|unit| unit.on_book) {
        step(&ctx);
    }
    if let Some(rotate) = rotate {
        rotate.await;
    }
    let app = Arc::new(app);
    let gov = app
        .governance
        .as_ref()
        .expect("actual signed Store governance")
        .clone();
    let (_, token) = gov
        .mint_signed_async(
            NewKeySpec {
                name: "decider".into(),
                ..Default::default()
            },
            4_000_000_000,
            1_700_000_000,
        )
        .await
        .expect("mint actual signed caller");
    let declarations: Vec<_> = busbar_kernel::plane_driver::registry::plane_decls()
        .iter()
        .map(|d| &d.declaration)
        .collect();
    crate::root::serve::attach(
        &late,
        Some(&gov),
        Some(gov.clone()),
        &app.demotion_record,
        &declarations,
    );
    let connector = crate::root::connector::the();
    let mut planes = Vec::new();
    for (instance, door) in [
        (
            "confidence-decisions",
            busbar_plane_decisions::door as busbar_contract::abi::mechanism::door::DoorFn,
        ),
        (
            "confidence-llm",
            busbar_plane_llm::door as busbar_contract::abi::mechanism::door::DoorFn,
        ),
    ] {
        let row = LinkedRow::of(door).expect("actual plane statement");
        let plane = Arc::new(
            load_linked::<Plane>(
                &row,
                Bind {
                    instance: Arc::from(instance),
                    max_inflight_cap: 64,
                    sink: Arc::new(NoSink),
                    dispatcher: dispatcher.adopter(),
                    conns: ConnTable::Host(crate::root::connector::admit(
                        connector.clone() as Arc<dyn DeclaredConns>
                    )),
                },
            )
            .expect("actual plane binds"),
        );
        planes.push((instance.to_owned(), plane));
    }
    let node = Arc::new(crate::root::plane_node::Node::new());
    node.bind_book(book.durability.clone());
    let post = Arc::new(crate::root::plane_node::NodeEndPost::new(node));
    let money_gov = gov.clone();
    let money_post = post.clone();
    let money = move || {
        Arc::new(PlaneMoney::new(
            money_gov.clone(),
            money_post.clone() as Arc<dyn EndPost>,
        ))
    };
    let stage = crate::root::serve::HookStage {
        host: {
            let app = app.clone();
            Arc::new(move || busbar_kernel::plane_driver::engine_host(&app))
        },
        gov: gov.clone(),
        journal: None,
    };
    let auths = crate::root::door_steps::OutboundAuths::new(
        dispatcher.clone(),
        crate::LINKED.auths,
        None,
        ConnTable::Host(crate::root::connector::admit(connector.clone())),
        busbar_kernel::diagnostics::Catalogue::NEUTRAL,
    );
    let models = crate::root::model_egress::ModelServing {
        pools: model_pools,
        lanes: app
            .engine_tables_view()
            .model_indices()
            .into_iter()
            .map(|(name, lane)| (name.to_string(), lane))
            .collect(),
        app: crate::root::model_egress::LiveApp::fixed(app.clone()),
    };
    let reach = crate::root::door_steps::DoorReach {
        providers: &providers,
        secrets: app.secret_resolver.clone(),
        auths: Arc::new(auths),
        conns: connector.clone() as Arc<dyn PollConns>,
        stream_ceiling_secs: 600,
        catalog: Some(&deploy.models),
        models: Some(&models),
        upgrades: Vec::new(),
    };
    let mut composed = crate::root::serve::compose_planes(
        &planes,
        &dispatcher,
        &late,
        &sections,
        None,
        &money,
        Some(&crate::root::serve::DoorEgress {
            reach: &reach,
            journal: post.clone(),
        }),
        Some(&stage),
        &app.serve_tables,
    )
    .await
    .expect("the actual root composes decisions egress and hooks");
    crate::root::serve::validate_hook_children(&composed, &app)
        .expect("declared child route is authorized at boot");
    composed.post = Some(post);
    let doors = crate::root::serve::door_routes(
        composed,
        || crate::root::kernel::ROOT_CARD.pin(),
        &[],
        &[],
    )
    .expect("actual decisions routes mount");
    let (router, _, handle) =
        busbar_kernel::build_split_routers_serving(app, doors, 1 << 20, 0, false);
    credential_handle.set(handle.clone());
    Served {
        router,
        token: token.expose(&busbar_kernel::seal::lend()).to_owned(),
        upstream,
        _handle: handle,
        book,
        task,
    }
}

impl Served {
    async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("authorization", format!("Bearer {}", self.token))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 65_536)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                panic!("invalid response: {}", String::from_utf8_lossy(&bytes))
            }),
        )
    }
    async fn chat(&self, pool: &str, text: &str) -> (StatusCode, Value) {
        self.post(
            "/v1/chat/completions",
            json!({"model":pool,"messages":[
            {"role":"user","content":"old-context"},{"role":"assistant","content":"recent-context"},
            {"role":"user","content":text}]}),
        )
        .await
    }
    fn take_requests(&self) -> Vec<(String, Value)> {
        std::mem::take(&mut *self.upstream.requests.lock().unwrap())
    }
    fn audit_len(&self) -> usize {
        self.book.durability.lock().unwrap().audit_records.len()
    }
    fn assert_usage(&self, start: usize, child: bool) {
        let book = self.book.durability.lock().unwrap();
        let records = &book.audit_records[start..];
        assert_eq!(
            records.len(),
            if child { 2 } else { 1 },
            "one settled parent and at most one child"
        );
        let parent = records.iter().find(|r| r.what.parent.is_none()).unwrap();
        let parent_units: Vec<_> = parent
            .usage
            .lines
            .iter()
            .map(|line| line.quantity)
            .collect();
        assert!(
            parent_units.contains(&busbar_contract::Count::from(5_u64)),
            "parent input stays independent: {parent_units:?}"
        );
        assert!(
            parent_units.contains(&busbar_contract::Count::from(7_u64)),
            "parent output stays independent: {parent_units:?}"
        );
        assert!(
            !parent
                .usage
                .lines
                .iter()
                .any(|line| line.class.as_str() == "decision"),
            "classifier work never enters parent usage"
        );
        if child {
            let child = records.iter().find(|r| r.what.parent.is_some()).unwrap();
            assert_eq!(child.what.parent, Some(parent.what.unit_key));
            assert_eq!(child.subject, parent.subject, "host carries the caller");
            let work: Vec<_> = child
                .usage
                .lines
                .iter()
                .filter(|line| line.class.as_str() == "decision")
                .collect();
            assert_eq!(work.len(), 1);
            assert_eq!(
                work[0].quantity,
                busbar_contract::Count::from(3_u64),
                "classifier work billed separately even when answer rejected"
            );
        }
    }
}

#[tokio::test]
async fn assembled_confidence_routing_preserves_custody_usage_and_fallback() {
    if isolated_case() {
        return;
    }
    let _one = crate::root::PUBLISHING.lock().await;
    let g = served().await;
    // Assert the actual Decisions normalization before consuming the same operation as a child.
    for (text, confidence) in [("direct", json!(0.8)), ("missing-confidence", Value::Null)] {
        let (status, answer) = g.post("/busbar/decisions/v1/classify",json!({"version":1,"model":"classifier",
            "messages":[{"role":"user","text":text}],"tiers":[{"label":"easy","description":"simple"},{"label":"hard","description":"complex"}]})).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(answer["label"], "easy");
        assert_eq!(answer["confidence"], confidence);
        assert_eq!(answer["schema"], "jev-choice-v1");
        assert!(answer.get("private").is_none());
        assert_eq!(g.take_requests().len(), 1);
    }
    let start = g.audit_len();
    let (status, answer) = g
        .post(
            "/v1/chat/completions",
            json!({"model":"classified","messages":[{"role":"user","content":[
                {"type":"text","text":"image-prompt-must-not-be-classified-as-complete-text"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,AA=="}}
            ]}]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer["choices"][0]["message"]["content"], "served-large");
    let requests = g.take_requests();
    assert_eq!(
        requests.len(),
        1,
        "an incomplete modality projection cannot start a classifier child"
    );
    assert_eq!(requests[0].0, "/v1/chat/completions");
    g.assert_usage(start, false);
    for (pool, text, expected, child, attempts) in [
        ("classified", "confident", "small", true, vec!["small"]),
        (
            "classified",
            "missing-confidence",
            "large",
            true,
            vec!["large"],
        ),
        ("classified", "unknown-label", "large", true, vec!["large"]),
        (
            "private",
            "raw-prompt-must-not-reach-classifier",
            "large",
            false,
            vec!["large"],
        ),
        (
            "classified",
            "fail-small",
            "large",
            true,
            vec!["small", "large"],
        ),
        (
            "unattested",
            "no-completeness-grant",
            "large",
            false,
            vec!["large"],
        ),
    ] {
        let start = g.audit_len();
        let (status, answer) = g.chat(pool, text).await;
        assert_eq!(status, StatusCode::OK, "{pool}/{text}: {answer}");
        assert_eq!(
            answer["choices"][0]["message"]["content"],
            format!("served-{expected}"),
            "{text}"
        );
        let requests = g.take_requests();
        let classifications: Vec<_> = requests
            .iter()
            .filter(|(path, _)| path == "/v1/systemone")
            .collect();
        assert_eq!(
            classifications.len(),
            usize::from(child),
            "one memoized classifier child even through fallback: {text}"
        );
        if child {
            assert_eq!(
                classifications[0].1["state"],
                json!([
                    {"role":"assistant","text":"recent-context"},{"role":"user","text":text}
                ]),
                "only the granted bounded projection crosses to classifier"
            );
            assert!(!classifications[0]
                .1
                .to_string()
                .contains("fixture-provider-credential"));
        }
        let models: Vec<_> = requests
            .iter()
            .filter(|(path, _)| path != "/v1/systemone")
            .map(|(_, body)| body["model"].as_str().unwrap())
            .collect();
        assert_eq!(
            models, attempts,
            "preferred authorized set, ordinary fallback: {text}"
        );
        g.assert_usage(start, child);
    }
    println!("actual confidence routing assertions completed");
}

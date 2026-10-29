// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE DECISIONS PLANE DOOR'S ROOT CELLS, as the composition root's door-cells row reads them: this
//! file is the one this crate states under its `links` name (`build.rs`, `cargo::metadata=cells=`),
//! compiled as a child of the composition root's serve module in its test builds (`super::` is
//! `root::serve`), the same for this pinned dependency as for a path one. The kernel and the loader
//! are the root's to link, never a plane's, so this directory has no `main.rs`,
//! and `cargo test -p busbar-plane-decisions` does not build it.
//!
//! What it states is the plane's answer to busbar's capability-equality ledger: its column, and the
//! root-leg cells the ledger marks not applicable on it, reason for reason, which the composition
//! root holds equal to the ledger's (`root/tests/door_cells.rs`). The leg's proven cells are where
//! the ledger's `root_legs` row `root-decisions` names them (busbar's
//! `crates/busbar/src/root/tests/serve_tests.rs`), and the hook cells below: every plane gets every
//! capability (THE DESIGN section 1), so the deployment's hooks bind to a decisions unit as to any
//! other, over the view the door's `project` answers. The door's secret-source driver is the
//! composition root's own (`root/tests/secret_source.rs`, through `decisions_name()`).

/// THIS PLANE'S CAPABILITY-EQUALITY COLUMNS (`qa/capability-equality.json`): the cells its root leg
/// answers for.
pub(crate) const COLUMNS: &[&str] = &["decisions"];

/// `(capability, column, reason)`: the root-leg cells this plane is NOT owed, each the ledger's own
/// not-applicable cell with its reason, word for word. The composition root holds the two equal: a
/// plane opts out of nothing the ledger says it has, and the ledger excuses nothing the plane does
/// not state.
pub(crate) const NOT_APPLICABLE: &[(&str, &str, &str)] = &[
    (
        "failover-reroute",
        "decisions",
        "the decisions plane has no candidate set to walk: a jev request names no model, so it is served only when decisions.models holds exactly one, and the door routes it ROUTE_DIRECT to that one model's provider; there is no second interchangeable candidate to try before the first byte",
    ),
    (
        "trust-pinning",
        "decisions",
        "decision providers are operator-declared config endpoints (a providers: entry named by decisions.models), not peer-published artifacts; there is no upstream-authored schema or card to pin, so the trust lifecycle has nothing of this plane's to govern",
    ),
    (
        "net-guard",
        "decisions",
        "the decisions plane's only outbound destination is its model's operator-configured provider base_url; no request-authored URL reaches its egress (the jev body is relayed, never read for a destination), so there is no SSRF surface the plane derives",
    ),
    (
        "catalogue",
        "decisions",
        "the decisions plane publishes no listing a caller could SEE: it claims one operation (POST /v1/systemone) and nothing else, and GET /v1/models keeps the bytes the model catalogue's own plane serves, so there is no per-item visibility of its own to answer",
    ),
];

/// THE SECRET-SOURCE SCAN'S DRIVEN DOOR (the composition root's `secret_source.rs`, which holds
/// every linked door plane to a driver): the Statement names the cells in this file drive. None:
/// this door's driver is the composition root's own (see the module docs), so this file adds no
/// name to the scan.
pub(crate) fn secret_source_driven() -> Vec<String> {
    Vec::new()
}

/// THE DECISIONS DOOR'S HOOK CELLS: the door served end to end as the exit cell serves it (its
/// egress sealed by the composition over the process's connector, a keyed caller on the data
/// router), with the kernel's hook stage composed over the generation, each cell's hooks defined in
/// its `hooks:` registry on the kernel's in-process hook double and bound either deployment-wide
/// (`global`) or by the section's own `decisions.hooks`.
mod hooks {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use axum::http::StatusCode;
    use busbar_contract::conn::{DeclaredConns, PollConns};
    use busbar_kernel::cost::CostModel;
    use busbar_kernel::governance::signing::{TokenSigner, DEFAULT_KID};
    use busbar_kernel::governance::{GovState, MemoryStore, NewKeySpec};
    use busbar_kernel::plane_driver::{EndPost, PlaneMoney};

    use super::super::door_tests::{far_end, send, CLAIMED};
    use super::super::planes_tests::{composed_services, PUBLISHING};
    use super::super::{compose_planes, door_routes, DoorEgress, HookStage};
    use crate::root::door_steps::{provider_routes, DoorReach, OutboundAuths};
    use crate::root::loader::dispatch::kinds::plane::Plane;
    use crate::root::loader::dispatch::{
        load_linked, Bind, DispatchConfig, Dispatcher, LinkedRow, NoSink,
    };
    use crate::root::plane_node::{Node, NodeEndPost};

    /// The deployment's dated card history: one entry, no price.
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

    /// The plane's registry key, under which its section's `decisions.hooks` are filed.
    const PLANE_KEY: &str = "decision";

    /// The env whose `test-hook` is the kernel's in-process hook double (`prompt: rw`, `user: ro`):
    /// its `settings:` choose its answer.
    fn hook_env() -> busbar_kernel::hooks::HookEnv {
        busbar_kernel::test_support::test_hook_env(
            &["test-hook"],
            busbar_kernel::test_support::HookNeeds {
                prompt: busbar_plugin_loader::sign::NeedLevel::Rw,
                user: busbar_plugin_loader::sign::NeedLevel::Ro,
            },
        )
    }

    /// A `kind: gate` on the hook double holding the `prompt` grant, answering as `settings` say,
    /// deployment-wide when `global`.
    fn hook(
        prompt: &str,
        global: bool,
        settings: serde_json::Value,
    ) -> busbar_kernel::config::HookCfg {
        serde_json::from_value(serde_json::json!({
            "kind": "gate",
            "module": "test-hook",
            "timeout_ms": 10_000,
            "on_error": "weighted",
            "prompt": prompt,
            "user": "ro",
            "priority": 0,
            "settings": settings,
            "global": global,
            "default": false,
            "signals": [],
            "groups": [],
            "phase": [],
        }))
        .expect("a hook document")
    }

    /// Where a cell binds its hooks.
    #[derive(Clone, Copy)]
    enum Bound {
        /// Deployment-wide, on every plane's units.
        Global,
        /// By the section's own `decisions.hooks`.
        Section,
    }

    /// One served decisions door and what a cell reads of it.
    struct Served {
        router: axum::Router,
        token: String,
        heard: tokio::sync::mpsc::UnboundedReceiver<String>,
    }

    /// The decisions door served as `instance` with one model over a far end on loopback, the
    /// kernel's hook stage composed over the generation, and `hooks` defined and bound as `bound`
    /// says (none: the stage is composed, and nothing is bound).
    async fn served(
        instance: &'static str,
        hooks: Vec<(&'static str, busbar_kernel::config::HookCfg)>,
        bound: Bound,
    ) -> Served {
        let (port, heard) = far_end().await;
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
            busbar_kernel::config::secret::resolve_linked_string_at_config,
        )
        .expect("the connector builds");
        let dispatcher = Arc::new(Dispatcher::new(DispatchConfig::default()));
        crate::root::connector::install_io(&dispatcher);
        let _ = dispatcher.lend_seal(busbar_kernel::seal::lend());
        let row =
            LinkedRow::of(busbar_plane_decisions::door).expect("the door states its Statement");
        let plane = Arc::new(
            load_linked::<Plane>(
                &row,
                Bind {
                    instance: Arc::from(instance),
                    max_inflight_cap: 64,
                    sink: Arc::new(NoSink),
                    dispatcher: dispatcher.adopter(),
                    conns: crate::root::loader::dispatch::ConnTable::Host(
                        Arc::clone(&connector) as Arc<dyn DeclaredConns>
                    ),
                },
            )
            .expect("the linked door binds"),
        );
        let section_key = plane.served().section;

        let signer = TokenSigner::from_secret_bytes(&[7u8; 32], DEFAULT_KID);
        let gov = Arc::new(
            GovState::new_with_signer(Arc::new(MemoryStore::new()), None, Some(signer))
                .expect("governance"),
        );
        let (_key, token) = gov
            .mint_signed(
                NewKeySpec {
                    name: "decider".to_string(),
                    ..Default::default()
                },
                4_000_000_000,
                1_700_000_000,
            )
            .expect("mint");
        gov.hydrate_budgets(&CostModel::flat(1), 0)
            .expect("hydrate");
        let node = Arc::new(Node::new());
        let book = crate::root::durability::node_book();
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

        let key_file = std::env::temp_dir().join(format!(
            "busbar-decisions-hooks-{instance}-{}",
            std::process::id()
        ));
        std::fs::write(&key_file, "sk-decisions-hooks").expect("the credential file");
        let provider: busbar_kernel::config::ProviderCfg = serde_yaml::from_str(&format!(
            "{{protocol: jev, base_url: 'http://127.0.0.1:{port}', api_key: {{file: '{}'}}, error_map: {{}}}}",
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
        let secrets: Arc<dyn busbar_contract::secret::SecretResolve> =
            Arc::new(busbar_kernel::config::secret::SecretResolver::builtins_only());
        let auths = OutboundAuths::new(
            Arc::clone(&dispatcher),
            crate::LINKED.auths,
            None,
            crate::root::loader::dispatch::ConnTable::Host(
                Arc::clone(&connector) as Arc<dyn DeclaredConns>
            ),
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

        // The section, its `hooks:` the cell's names when they bind there.
        let names: Vec<String> = hooks.iter().map(|(n, _)| (*n).to_string()).collect();
        let section = match bound {
            Bound::Section if !names.is_empty() => format!(
                "{{models: {{m: {{provider: typesafe}}}}, hooks: [{}]}}",
                names.join(", ")
            ),
            _ => "models: {m: {provider: typesafe}}".to_string(),
        };
        let mut sections = BTreeMap::new();
        sections.insert(
            section_key,
            serde_yaml::from_str(&section).expect("a section"),
        );
        // THE KERNEL'S HOOK STAGE over the App built below, as production composes it.
        let live_app: Arc<std::sync::OnceLock<Arc<busbar_kernel::state::App>>> = Arc::default();
        let stage = HookStage {
            host: {
                let live_app = Arc::clone(&live_app);
                Arc::new(move || {
                    busbar_kernel::plane_driver::engine_host(
                        live_app
                            .get()
                            .expect("the cell's App is built before any unit"),
                    )
                })
            },
            gov: Arc::clone(&gov),
            journal: None,
        };
        // THE ADMIN TABLE the door is published into, lent to the App built below as the root lends
        // the boot generation its own (busbar #1834: the table is a value each generation carries).
        let tables = busbar_kernel::plane_driver::serve::ServeTables::default();
        let mut composed = compose_planes(
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
            Some(&stage),
            &tables,
        )
        .expect("the door plane composes, its egress sealed");
        let _ = std::fs::remove_file(&key_file);
        composed.post = Some(Arc::clone(&post));
        let mut app = busbar_kernel::test_support::TestApp::new()
            .serve_tables(tables.clone())
            .keys_chain();
        if matches!(bound, Bound::Section) && !names.is_empty() {
            app.set_container_hooks(PLANE_KEY, Vec::new(), names.clone());
        }
        let app = if hooks.is_empty() {
            app
        } else {
            let app = hooks
                .iter()
                .fold(app.hook_env(hook_env()), |app, (name, cfg)| {
                    app.hook(name, cfg.clone())
                });
            match bound {
                Bound::Global => names
                    .iter()
                    .fold(app, |app, name| app.global_hook(name))
                    .resolve_global_gates(),
                Bound::Section => app,
            }
        };
        let app = app
            .governance(Arc::clone(&gov))
            .cost(CostModel::flat(1))
            .build();
        let _ = live_app.set(Arc::clone(&app));
        let doors = door_routes(composed, || CARD.pin(), &[], &[]).expect("its claims mount");
        let (router, _admin, _handle) =
            busbar_kernel::build_split_routers_serving(Arc::clone(&app), doors, 1 << 20, 0, false);
        Served {
            router,
            token: token.expose(&busbar_kernel::seal::lend()).to_string(),
            heard,
        }
    }

    impl Served {
        /// The keyed caller's decision request: its status and body, and the request the far end
        /// heard, if it was dialled.
        async fn decide(&mut self) -> (StatusCode, serde_json::Value, Option<String>) {
            let response = send(&self.router, CLAIMED, Some(&self.token)).await;
            let status = response.status();
            let body = axum::body::to_bytes(response.into_body(), 1 << 16)
                .await
                .expect("the body");
            let body = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
            (status, body, self.heard.try_recv().ok())
        }
    }

    /// HOOKS-GATE: a deployment-wide decision gate that rejects is asked about the decisions unit
    /// after admission and before its dial, and its veto is the caller's answer, the hook's own
    /// status and words, rendered in the plane's dialect, the provider never dialled. RED: the
    /// door refused `project`, so the stage could not project the unit and refused it in the
    /// kernel's words instead of the gate's.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_global_gate_refuses_a_decisions_request_before_its_dial() {
        let _one = PUBLISHING.lock().await;
        let mut s = served(
            "decisions-hooks-gate",
            vec![(
                "closed",
                hook(
                    "ro",
                    true,
                    serde_json::json!({
                        "raw_decide_reply": {"reject": {"status": 409, "message": "no decisions today"}}
                    }),
                ),
            )],
            Bound::Global,
        )
        .await;
        let (status, body, heard) = s.decide().await;
        assert_eq!(status.as_u16(), 409, "the hook's own status: {body}");
        assert_eq!(body["error"]["message"], "no decisions today", "{body}");
        assert!(
            heard.is_none(),
            "the gated unit reached the provider: {heard:?}"
        );
    }

    /// HOOKS-TAP, BY THE SECTION'S OWN ATTACH: a `prompt: rw` hook the section names in
    /// `decisions.hooks` rewrites the request at the kernel's hook stage before the provider is
    /// dialled: the provider receives the document the hook wrote, which replaces the caller's
    /// request (1.5.5). RED: `decisions.hooks` was accepted and bound nothing, and the door refused
    /// `project`, so no rewrite reached a decisions unit.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_decisions_hooks_rewrite_alters_the_request_the_provider_receives() {
        let _one = PUBLISHING.lock().await;
        let mut s = served(
            "decisions-hooks-rewrite",
            vec![(
                "redact",
                hook(
                    "rw",
                    false,
                    serde_json::json!({ "raw_transform_reply": {
                        "rewrite": { "messages": [
                            { "role": "user", "content": { "state": { "amount": 0 } } }
                        ] }
                    } }),
                ),
            )],
            Bound::Section,
        )
        .await;
        let (status, body, heard) = s.decide().await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let head = heard.expect("the provider was dialled");
        assert!(
            head.ends_with(r#"{"state":{"amount":0}}"#),
            "the provider received the hook's document, not the caller's: {head}"
        );
    }

    /// `decisions.hooks` IS HONOURED, NOT SILENTLY ACCEPTED: a gate the section names refuses the
    /// unit in its own words, the provider never dialled.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_gate_named_in_decisions_hooks_refuses_the_unit() {
        let _one = PUBLISHING.lock().await;
        let mut s = served(
            "decisions-hooks-section-gate",
            vec![(
                "closed",
                hook(
                    "ro",
                    false,
                    serde_json::json!({
                        "raw_decide_reply": {"reject": {"status": 403, "message": "decisions closed"}}
                    }),
                ),
            )],
            Bound::Section,
        )
        .await;
        let (status, body, heard) = s.decide().await;
        assert_eq!(status.as_u16(), 403, "{body}");
        assert_eq!(body["error"]["message"], "decisions closed", "{body}");
        assert!(heard.is_none(), "{heard:?}");
    }

    /// WITH NO HOOK BOUND, THE BYTES ARE UNCHANGED: the hook stage is composed and binds nothing,
    /// the door is never asked to project, and the provider receives the caller's request byte for
    /// byte.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn with_no_hook_bound_the_provider_receives_the_callers_bytes() {
        let _one = PUBLISHING.lock().await;
        let mut s = served("decisions-hooks-none", Vec::new(), Bound::Global).await;
        let (status, body, heard) = s.decide().await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let head = heard.expect("the provider was dialled");
        assert!(head.ends_with(r#"{"state":{"amount":7}}"#), "{head}");
    }
}

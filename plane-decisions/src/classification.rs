// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! Versioned, bounded classification adapter. Provider evidence never leaves this adapter.
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MAX_BYTES: usize = 65_536;
pub(crate) const PATH: &str = "/busbar/decisions/v1/classify";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tier {
    label: String,
    description: String,
    #[serde(default)]
    examples: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    version: u32,
    model: String,
    messages: Vec<Message>,
    tiers: Vec<Tier>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Message {
    role: String,
    text: String,
}

impl Request {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > MAX_BYTES {
            return Err("classification input exceeds byte limit");
        }
        let request: Self =
            serde_json::from_slice(bytes).map_err(|_| "invalid classification request")?;
        let labels: BTreeSet<_> = request.tiers.iter().map(|t| &t.label).collect();
        if request.version != 1
            || request.model.is_empty()
            || !(2..=32).contains(&request.tiers.len())
            || labels.len() != request.tiers.len()
            || request
                .tiers
                .iter()
                .any(|t| t.label.is_empty() || t.label.len() > 128 || t.description.is_empty())
            || request.messages.is_empty()
            || request.messages.len() > 33
            || request
                .messages
                .last()
                .is_none_or(|m| m.role != "user" || m.text.is_empty())
            || request
                .messages
                .iter()
                .any(|m| !matches!(m.role.as_str(), "user" | "assistant"))
        {
            return Err("invalid classification version, tiers, or text window");
        }
        Ok(request)
    }
    pub(crate) fn provider_body(&self, dialect: u32, upstream: Option<&str>) -> Vec<u8> {
        let criteria: BTreeMap<_, _> = self
            .tiers
            .iter()
            .map(|t| {
                (
                    &t.label,
                    json!({"description":t.description,"examples":t.examples}).to_string(),
                )
            })
            .collect();
        let state = self
            .messages
            .iter()
            .map(|m| json!({"role":m.role,"text":m.text}))
            .collect::<Vec<_>>();
        let questions = json!({"complexity":{"type":"choice","instructions":"Select the tier that best describes the complexity of the latest user request, using preceding messages only as context.","criteria":criteria}});
        let value = if dialect == 1 {
            json!({"states":[{"id":"complexity","state":serde_json::to_string(&state).expect("JSON values serialize"),"questions":questions}]})
        } else {
            json!({"model":upstream.unwrap_or(&self.model),"state":state,"questions":questions})
        };
        serde_json::to_vec(&value).expect("JSON values serialize")
    }
    pub(crate) fn normalize(&self, bytes: &[u8], dialect: u32) -> Result<Vec<u8>, &'static str> {
        if bytes.len() > MAX_BYTES {
            return Err("classification response exceeds byte limit");
        }
        let value: Value = serde_json::from_slice(bytes).map_err(|_| "invalid classifier JSON")?;
        let answer = if dialect == 1 {
            let states = value["states"]
                .as_array()
                .filter(|v| v.len() == 1)
                .ok_or("invalid classifier states")?;
            if states[0]["id"] != "complexity" {
                return Err("classifier state mismatch");
            }
            &states[0]["answers"]["complexity"]
        } else {
            &value["answers"]["complexity"]
        };
        if answer["type"] != "choice" {
            return Err("unsupported classifier answer schema");
        }
        let label = answer["choice"]
            .as_str()
            .filter(|s| self.tiers.iter().any(|t| t.label == *s))
            .ok_or("unknown classifier label")?;
        // Native NanoJev Choice has probabilities but no confidence field. Do not give an
        // unknown extension the semantics of Jev's documented confidence field.
        let confidence = if dialect == 1 {
            None
        } else {
            match answer.get("confidence") {
                None | Some(Value::Null) => None,
                Some(v) => Some(probability(v).ok_or("invalid confidence")?),
            }
        };
        let probabilities = match answer.get("probabilities") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let map = value
                    .as_object()
                    .filter(|m| m.len() == self.tiers.len())
                    .ok_or("incomplete probabilities")?;
                let mut sum = 0.0;
                for tier in &self.tiers {
                    sum += map
                        .get(&tier.label)
                        .and_then(probability)
                        .ok_or("invalid probabilities")?;
                }
                if (sum - 1.0).abs() > 1e-6 {
                    return Err("unnormalized probabilities");
                }
                let selected = map
                    .get(label)
                    .and_then(probability)
                    .ok_or("missing selected probability")?;
                if map.values().filter_map(probability).any(|p| p > selected) {
                    return Err("choice contradicts probabilities");
                }
                Some(value)
            }
        };
        // No score-to-confidence formula is inferred. NanoJev calibration is not asserted.
        serde_json::to_vec(&json!({"version":1,"label":label,"confidence":confidence,"probabilities":probabilities,"source":if dialect==1 {"nanojev"} else {"jev"},"schema":if dialect==1 {"nanojev-choice-v1"} else {"jev-choice-v1"}})).map_err(|_|"classification encoding failed")
    }
}
fn probability(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        Request::parse(br#"{"version":1,"model":"classifier","messages":[{"role":"user","text":"hello"}],"tiers":[{"label":"easy","description":"simple"},{"label":"hard","description":"complex"}]}"#).unwrap()
    }
    #[test]
    fn classification_preserves_absent_confidence_and_validates_probabilities() {
        let r = request();
        let out=r.normalize(br#"{"answers":{"complexity":{"type":"choice","choice":"easy","probabilities":{"easy":0.9,"hard":0.1}}}}"#,0).unwrap();
        assert!(serde_json::from_slice::<Value>(&out).unwrap()["confidence"].is_null());
        for body in [
            json!({"choice":"arbitrary","confidence":1.0}),
            json!({"choice":"easy","confidence":1.1}),
            json!({"choice":"easy","probabilities":{"easy":0.8}}),
            json!({"choice":"easy","probabilities":{"easy":0.8,"hard":0.8}}),
            json!({"choice":"easy","probabilities":{"easy":0.2,"hard":0.8}}),
        ] {
            let mut body = body;
            body["type"] = json!("choice");
            assert!(r
                .normalize(
                    &serde_json::to_vec(&json!({"answers":{"complexity":body}})).unwrap(),
                    0
                )
                .is_err());
        }
    }
    #[test]
    fn classification_dialect_requests_and_nano_answer() {
        let r = request();
        let j: Value = serde_json::from_slice(&r.provider_body(0, Some("pinned"))).unwrap();
        assert_eq!(j["model"], "pinned");
        assert_eq!(j["questions"]["complexity"]["type"], "choice");
        let n: Value = serde_json::from_slice(&r.provider_body(1, None)).unwrap();
        assert_eq!(n["states"][0]["id"], "complexity");
        assert_eq!(
            n.as_object().unwrap().len(),
            1,
            "NanoJev only accepts the states envelope"
        );
        let result=r.normalize(br#"{"states":[{"id":"complexity","answers":{"complexity":{"type":"choice","choice":"hard","confidence":0.99}}}]}"#,1).unwrap();
        assert!(
            serde_json::from_slice::<Value>(&result).unwrap()["confidence"].is_null(),
            "unrecognized NanoJev confidence is not assigned Jev semantics"
        );
    }
}

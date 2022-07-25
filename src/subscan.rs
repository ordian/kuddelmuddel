use crate::InclusionEvent;
use anyhow::Context;
use indicatif::ProgressBar;
use std::str::FromStr as _;
use tokio::time::{sleep, Duration};

type SessionIndex = u32;
type ValidatorIndex = u32;

pub mod events {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    pub struct Request {
        pub row: u32,
        pub page: u32,
        pub module: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub call: Option<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub block_num: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub block_range: Option<String>,
    }

    pub mod inclusion {
        use super::*;

        #[derive(Debug, Deserialize)]
        pub struct Response {
            pub data: Option<Data>,
        }

        #[derive(Debug, Deserialize)]
        pub struct Data {
            pub events: Option<Vec<Event>>,
        }

        #[derive(Debug, Deserialize)]
        pub enum EventId {
            CandidateIncluded,
            CandidateBacked,
            CandidateTimedOut,
        }

        #[derive(Debug, Deserialize)]
        pub struct Event {
            pub block_num: u32,
            pub event_id: EventId,
            pub params: String,
        }
    }

    pub mod disputes {
        use super::*;

        #[derive(Debug, Deserialize)]
        pub struct Response {
            pub data: Data,
        }

        #[derive(Debug, Deserialize)]
        pub struct Data {
            pub events: Option<Vec<Event>>,
        }

        #[derive(Debug, Deserialize, PartialOrd, PartialEq, Eq, Ord)]
        pub struct Event {
            pub block_num: u32,
            pub extrinsic_idx: u32,
        }
    }
}

pub mod extrinsic {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    pub struct Request {
        pub extrinsic_index: String,
    }

    pub mod parainherent {
        use super::{
            super::{SessionIndex, ValidatorIndex},
            *,
        };
        use std::collections::HashMap;

        #[derive(Debug, Deserialize)]
        pub struct Response {
            pub data: Option<Data>,
        }

        #[derive(Debug, Deserialize)]
        pub struct Data {
            pub params: Vec<Params>,
        }

        #[derive(Debug, Deserialize)]
        pub struct Params {
            pub value: Value,
        }

        #[derive(Debug, Deserialize)]
        pub struct Value {
            pub disputes: Vec<DisputeVotes>,
        }

        #[derive(Debug, Deserialize)]
        pub struct DisputeVotes {
            pub session: SessionIndex,
            pub statements: Vec<DisputeVote>,
        }

        #[derive(Debug, Deserialize)]
        pub struct DisputeVote {
            #[serde(rename = "col1")]
            pub kind: HashMap<DisputeVoteKind, serde_json::Value>,
            #[serde(rename = "col2")]
            pub validator_index: ValidatorIndex,
        }

        #[derive(Debug, Deserialize, Eq, PartialEq, Hash)]
        pub enum DisputeVoteKind {
            Invalid,
            Valid,
        }
    }
}

impl TryFrom<events::inclusion::Event> for InclusionEvent {
    type Error = ();

    fn try_from(event: events::inclusion::Event) -> Result<Self, Self::Error> {
        use events::inclusion::EventId::*;

        let substr = "\"para_id\":";
        let idx = event.params.find(substr).ok_or(())? + substr.len();
        let para_id = u32::from_str(&event.params[idx..idx + 4]).map_err(|_| ())?;
        let block_num = event.block_num;

        let included = match event.event_id {
            CandidateIncluded => true,
            CandidateBacked => false,
            _ => {
                eprintln!("{block_num}: skipping CandidateTimedOut({para_id})");
                return Err(());
            }
        };

        Ok(Self {
            block_num,
            para_id,
            included,
        })
    }
}

pub async fn fetch_inclusion_events(
    network: &str,
    up_to_block: u32,
    para_id: u32,
    enough_events: usize,
) -> anyhow::Result<Vec<InclusionEvent>> {
    let url = format!("https://{network}.api.subscan.io/api/scan/events");
    let mut events: Vec<InclusionEvent> = Vec::new();
    eprintln!("Fetching {enough_events} events for {network}, para_id({para_id}) up to block {up_to_block}");
    let pb = ProgressBar::new(enough_events as u64);
    let mut block_num = up_to_block;
    while events.len() < enough_events {
        let request = events::Request {
            row: 100,
            page: 0,
            module: "parainclusion",
            block_num: Some(block_num),
            call: None,
            block_range: None,
        };
        let client = reqwest::Client::new();
        let res = client.post(&url).json(&request).send().await?;

        let response = res.json::<events::inclusion::Response>().await?;
        let new_events: Vec<InclusionEvent> = response
            .data
            .into_iter()
            .flat_map(|d| d.events)
            .flatten()
            .flat_map(|e| InclusionEvent::try_from(e).ok())
            .filter(|e| e.para_id == para_id)
            .collect();

        pb.inc(new_events.len() as u64);
        block_num -= 1;
        events.extend(new_events);
        // don't trigger rate limiting
        sleep(Duration::from_millis(150)).await;
    }
    pb.finish_with_message("Fetching complete!");

    events.reverse();
    let total = events.len();
    events.sort();
    events.dedup();
    if events.len() != total {
        eprintln!("{} duplicate events found", total - events.len());
    }

    Ok(events)
}

pub async fn fetch_disputes_events(
    network: &str,
    up_to_block: u32,
    enough_events: usize,
) -> anyhow::Result<Vec<events::disputes::Event>> {
    let url = format!("https://{network}.api.subscan.io/api/scan/events");
    let mut disputes_initiated: Vec<events::disputes::Event> = Vec::new();
    let pb = ProgressBar::new(enough_events as u64);
    let from_block = up_to_block.saturating_sub(400_000); // HACK
    let range = format!("{from_block}-{up_to_block}");
    let mut page = 0;
    while disputes_initiated.len() < enough_events {
        let request = events::Request {
            row: 100,
            page,
            module: "parasdisputes",
            call: Some("disputeinitiated"),
            block_range: Some(range.clone()),
            block_num: None,
        };
        let client = reqwest::Client::new();
        let res = client.post(&url).json(&request).send().await?;

        let response = res.json::<events::disputes::Response>().await?;
        let new_events: Vec<events::disputes::Event> =
            response.data.events.into_iter().flatten().collect();

        pb.inc(new_events.len() as u64);
        page += 1;

        if new_events.is_empty() {
            break;
        }
        disputes_initiated.extend(new_events);
        // don't trigger rate limiting
        sleep(Duration::from_millis(150)).await;
    }
    pb.finish_with_message("Fetching complete!");

    disputes_initiated.sort();
    disputes_initiated.dedup();

    eprintln!(
        "Fetched {} DisputeInitiated unique events",
        disputes_initiated.len()
    );

    Ok(disputes_initiated)
}

#[derive(serde::Serialize)]
pub struct DisputeInitiated {
    pub session_index: SessionIndex,
    pub block_num: u32,
    pub validator_index: ValidatorIndex,
}

pub async fn fetch_dispute_initiators(
    network: &str,
    events: Vec<events::disputes::Event>,
) -> anyhow::Result<Vec<DisputeInitiated>> {
    let url = format!("https://{network}.api.subscan.io/api/scan/extrinsic");
    let mut initiators = Vec::new();
    for event in events {
        let events::disputes::Event {
            block_num,
            extrinsic_idx,
        } = event;

        let request = extrinsic::Request {
            extrinsic_index: format!("{block_num}-{extrinsic_idx}"),
        };

        let client = reqwest::Client::new();
        let res = client.post(&url).json(&request).send().await?;

        let response = res
            .json::<extrinsic::parainherent::Response>()
            .await
            .with_context(|| {
                format!("unexpected response for parainherent {block_num}-{extrinsic_idx}")
            })?;
        let mut data = match response.data {
            Some(data) => data,
            None => {
                eprintln!("null response for extrinsic {block_num}-{extrinsic_idx}, skipping",);
                continue;
            }
        };
        let disputes: Vec<extrinsic::parainherent::DisputeVotes> =
            data.params.remove(0).value.disputes;

        for votes in disputes {
            let session_index = votes.session;
            for vote in votes.statements {
                let invalid = extrinsic::parainherent::DisputeVoteKind::Invalid;
                if vote.kind.contains_key(&invalid) {
                    initiators.push(DisputeInitiated {
                        session_index,
                        block_num,
                        validator_index: vote.validator_index,
                    });
                }
            }
        }
    }
    Ok(initiators)
}

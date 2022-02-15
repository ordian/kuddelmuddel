use crate::Event;
use indicatif::ProgressBar;
use std::str::FromStr as _;
use tokio::time::{sleep, Duration};

pub mod events {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    pub struct Request {
        pub row: u32,
        pub page: u32,
        pub module: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub block_num: Option<u32>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Response {
        pub data: Data,
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

impl TryFrom<events::Event> for Event {
    type Error = ();

    fn try_from(event: events::Event) -> Result<Self, Self::Error> {
        use events::EventId::*;

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

pub async fn fetch(
    network: &str,
    up_to_block: u32,
    para_id: u32,
    enough_events: usize,
) -> anyhow::Result<Vec<Event>> {
    let url = format!("https://{network}.api.subscan.io/api/scan/events");
    let mut events: Vec<Event> = Vec::new();
    eprintln!("Fetching events for {network}, para_id={para_id} up to block {up_to_block}");
    let pb = ProgressBar::new(enough_events as u64);
    let mut block_num = up_to_block;
    while events.len() < enough_events {
        let request = events::Request {
            row: 100,
            page: 0,
            module: "parainclusion",
            block_num: Some(block_num),
        };
        let client = reqwest::Client::new();
        let res = client.post(&url).json(&request).send().await?;

        let response = res.json::<events::Response>().await?;
        let new_events: Vec<Event> = response
            .data
            .events
            .into_iter()
            .flatten()
            .flat_map(|e| Event::try_from(e).ok())
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

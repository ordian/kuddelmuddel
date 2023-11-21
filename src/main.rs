use crate::primitives::{AccountId32, SessionIndex, H256};

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::str::FromStr;

mod candidate_validation;
mod povs_today;
mod primitives;
mod subscan;
mod subxt;

// The current version, including the latest commit hash.
//
// We probably don't need the node/worker version check for this project, but it also doesn't hurt.
// The same kuddelmuddel binary provides both the node and the workers. The only use case for the
// version check is if the binary gets replaced while the node is running.
const NODE_VERSION: &'static str = env!("SUBSTRATE_CLI_IMPL_VERSION");

#[derive(Parser)]
#[clap(version)]
struct Cli {
    #[clap(subcommand)]
    commands: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Fetches the backing and inclusion events and writes out csv files to `./out/`.
    ///
    /// Example:
    /// ```bash
    /// cargo run -- inclusion --network kusama --para-id 2023 --up-to-block 11324714
    /// ```
    Inclusion {
        /// Name of the network, e.g. "kusama".
        #[clap(long, default_value = "kusama")]
        network: String,

        /// Parachain ID to be processed.
        #[clap(long)]
        para_id: u32,

        /// The block number up to which we should
        /// be fetching events, e.g. 13524714.
        #[clap(long)]
        up_to_block: u32,

        /// How many events to fetch
        #[clap(long, default_value_t = 500)]
        num_events: usize,
    },
    /// Fetches the dispute invalid votes and writes out a csv file to `./out/`.
    ///
    /// Example:
    /// ```bash
    /// cargo run -- disputes --network kusama --from-block 11324714 --num-events 200 \
    ///  --rpc-url "wss://kusama-rpc.polkadot.io:443"
    /// ```
    Disputes {
        /// Name of the network, e.g. "kusama".
        #[clap(long, default_value = "kusama")]
        network: String,

        /// How many events to fetch.
        #[clap(long, default_value_t = 100)]
        num_events: usize,

        /// The block number up to which we should
        /// be fetching events, e.g. 13524714.
        #[clap(long)]
        up_to_block: u32,

        /// Url for an RPC node to query the historical data.
        ///
        /// Example:
        /// `wss://kusama-rpc.polkadot.io:443` or `http://localhost:9933/`
        #[clap(long)]
        rpc_url: String,
    },
    /// Given the candidate hash, fetch candidate's available data
    /// and receipt from `povs.today` and the corresponding validation code
    /// from the runtime, compile validation code and validate the candidate.
    ///
    /// All the data will be cached in the `--cache` folder.
    ///
    /// Example:
    /// ```bash
    /// cargo run --release -- validate-candidate --network kusama \
    ///  --candidate-hash "0x03134f027883df8db3ce71602412d906024c96eaef06cda403c48cfb6661e5a8" \
    ///  --rpc-url "wss://kusama-rpc.polkadot.io:443"
    /// ```
    ValidateCandidate {
        /// Name of the network, e.g. "kusama".
        #[clap(long, default_value = "kusama")]
        network: String,

        /// Url for an RPC node to query the runtime.
        ///
        /// Example:
        /// `wss://kusama-rpc.polkadot.io:443` or `http://localhost:9933/`
        #[clap(long)]
        rpc_url: String,

        /// Hash of the candidate.
        #[clap(long)]
        candidate_hash: H256,

        /// Cache folder storing candidate receipts, available data, validation code.
        ///
        /// Default: `./.cache`.
        #[clap(long)]
        cache: Option<PathBuf>,
    },

    // These are needed for candidate validation:
    #[allow(missing_docs)]
    #[clap(name = "prepare-worker", hide = true)]
    PvfPrepareWorker(ValidationWorkerCommand),

    #[allow(missing_docs)]
    #[clap(name = "execute-worker", hide = true)]
    PvfExecuteWorker(ValidationWorkerCommand),
}

#[allow(missing_docs)]
#[derive(Debug, Parser)]
pub struct ValidationWorkerCommand {
    /// The path to the validation host's socket.
    #[arg(long)]
    pub socket_path: String,
    /// The path to the worker-specific temporary directory.
    #[arg(long)]
    pub worker_dir_path: String,
    /// Calling node implementation version
    #[arg(long)]
    pub node_impl_version: String,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InclusionEvent {
    pub block_num: u32,
    pub para_id: u32,
    pub included: bool,
}

#[derive(serde::Serialize, Clone, Copy)]
pub struct InclusionPlottingPoint {
    pub block_num: u32,
    pub blocks: u32,
}

#[derive(serde::Serialize)]
pub struct DisputeInitiator {
    pub session_index: SessionIndex,
    pub account_id: AccountId32,
}

async fn handle_inclusion(
    network: String,
    para_id: u32,
    up_to_block: u32,
    num_events: usize,
) -> anyhow::Result<()> {
    let events =
        subscan::fetch_inclusion_events(&network, up_to_block, para_id, num_events).await?;

    let mut last_backed = None;
    let mut last_included = None;
    let mut backing_times = Vec::new();
    let mut inclusion_times = Vec::new();

    for event in events.into_iter().filter(|e| e.para_id == para_id) {
        if event.included {
            let block_num = event.block_num;
            if let Some(b) = last_backed {
                let blocks = block_num.saturating_sub(b);
                inclusion_times.push(InclusionPlottingPoint { block_num, blocks });
            }
            last_included = Some(block_num);
        } else {
            let block_num = event.block_num;
            if let Some(i) = last_included {
                let blocks = block_num.saturating_sub(i);
                backing_times.push(InclusionPlottingPoint { block_num, blocks });
            }
            last_backed = Some(block_num);
        }
    }

    std::fs::create_dir_all("out")?;

    for (data, name) in [(backing_times, "backing"), (inclusion_times, "inclusion")] {
        if data.is_empty() {
            eprintln!("No {name} events found for {para_id}");
            continue;
        }
        let csv_file = format!("out/{up_to_block}-{name}-{para_id}.csv");
        let mut wrt = csv::Writer::from_path(&csv_file)?;
        for p in data.iter().copied() {
            wrt.serialize(p)?;
        }
        wrt.flush()?;
        eprintln!("Saved the data to {csv_file}");
    }
    Ok(())
}

async fn handle_disputes(
    network: String,
    num_events: usize,
    up_to_block: u32,
    rpc_url: String,
) -> anyhow::Result<()> {
    let events = subscan::fetch_disputes_events(&network, up_to_block, num_events).await?;
    let initiators = subscan::fetch_dispute_initiators(&network, events).await?;
    let input = initiators.iter().map(|i| {
        (
            i.session_index.clone(),
            FromStr::from_str(&i.block_hash).expect("valid block_hash"),
        )
    });
    let account_map = subxt::historical_account_keys(rpc_url, input).await?;

    let initiators = initiators.into_iter().map(|i| DisputeInitiator {
        session_index: i.session_index,
        // TODO: handle missing keys
        account_id: account_map[&i.session_index][i.validator_index as usize].clone(),
    });

    std::fs::create_dir_all("out")?;

    let csv_file = format!("out/disputes-{network}-{up_to_block}.csv");
    let mut wrt = csv::Writer::from_path(&csv_file)?;
    for i in initiators.into_iter() {
        wrt.serialize(i)?;
    }
    wrt.flush()?;
    eprintln!("Saved the data to {csv_file}");
    Ok(())
}

async fn handle_validate_candidate(
    network: String,
    rpc_url: String,
    candidate_hash: H256,
    cache: Option<PathBuf>,
) -> anyhow::Result<()> {
    let default_cache = PathBuf::from(".cache");
    let cache = cache.unwrap_or(default_cache);
    let _ = std::fs::create_dir_all(cache.as_path());

    let povs_path = cache.as_path().join("povs");
    let _ = std::fs::create_dir_all(&povs_path);

    let pvfs_path = cache.as_path().join("pvfs");
    let _ = std::fs::create_dir_all(&pvfs_path);

    let (pov, receipt) =
        povs_today::get_or_fetch_candidate(povs_path, &candidate_hash, &network).await?;

    let code_hash = receipt.descriptor.validation_code_hash;
    let relay_parent = receipt.descriptor.relay_parent;

    let pvf = subxt::validation_code_by_hash(pvfs_path.as_path(), rpc_url, code_hash, relay_parent)
        .await?;

    let path = pvfs_path.as_path().join("compiled");
    candidate_validation::validate_candidate(path, pov, pvf, NODE_VERSION.into()).await
}

fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let cli = Cli::parse();
    let security_status = Default::default();

    match cli.commands {
        Commands::Inclusion {
            network,
            para_id,
            up_to_block,
            num_events,
        } => rt.block_on(handle_inclusion(network, para_id, up_to_block, num_events)),
        Commands::Disputes {
            network,
            num_events,
            up_to_block,
            rpc_url,
        } => rt.block_on(handle_disputes(network, num_events, up_to_block, rpc_url)),
        Commands::ValidateCandidate {
            network,
            rpc_url,
            candidate_hash,
            cache,
        } => rt.block_on(handle_validate_candidate(
            network,
            rpc_url,
            candidate_hash,
            cache,
        )),
        // TODO: Build separate workers. See github.com/paritytech/pvf-checker.
        Commands::PvfPrepareWorker(params) => {
            polkadot_node_core_pvf_prepare_worker::worker_entrypoint(
                params.socket_path.into(),
                params.worker_dir_path.into(),
                Some(&params.node_impl_version),
                Some(NODE_VERSION.into()),
                security_status,
            );
            Ok(())
        }
        Commands::PvfExecuteWorker(params) => {
            polkadot_node_core_pvf_execute_worker::worker_entrypoint(
                params.socket_path.into(),
                params.worker_dir_path.into(),
                Some(&params.node_impl_version),
                Some(NODE_VERSION.into()),
                security_status,
            );
            Ok(())
        }
    }
}

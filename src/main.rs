use clap::{Parser, Subcommand};

pub mod subscan;

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
    /// ```
    /// cargo run -- inclusion --network kusama --para-id 2023 --up-to-block 11324714
    /// ```
    Inclusion {
        /// Name of the network, e.g. "kusama"
        #[clap(long, default_value = "kusama")]
        network: String,

        /// Parachain ID to be processed
        #[clap(long)]
        para_id: u32,

        /// The block number up to which we should
        /// be fetching events, e.g. 13524714
        #[clap(long)]
        up_to_block: u32,

        /// How many events to fetch
        #[clap(long, default_value_t = 500)]
        num_events: usize,
    },
    /// Fetches the dispute invalid votes and writes out a csv file to `./out/`.
    ///
    /// Example:
    /// ```
    /// cargo run -- disputes --network kusama --from-block 11324714 --num-events 200
    /// ```
    Disputes {
        /// Name of the network, e.g. "kusama"
        #[clap(long, default_value = "kusama")]
        network: String,

        /// How many events to fetch
        #[clap(long, default_value_t = 100)]
        num_events: usize,

        /// The block number up to which we should
        /// be fetching events, e.g. 13524714
        #[clap(long)]
        up_to_block: u32,
    },
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InclusionEvent {
    pub block_num: u32,
    pub para_id: u32,
    pub included: bool,
}

#[derive(serde::Serialize, Clone, Copy)]
pub struct PlottingPoint {
    pub block_num: u32,
    pub blocks: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.commands {
        Commands::Inclusion {
            network,
            para_id,
            up_to_block,
            num_events,
        } => {
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
                        inclusion_times.push(PlottingPoint { block_num, blocks });
                    }
                    last_included = Some(block_num);
                } else {
                    let block_num = event.block_num;
                    if let Some(i) = last_included {
                        let blocks = block_num.saturating_sub(i);
                        backing_times.push(PlottingPoint { block_num, blocks });
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
        }
        Commands::Disputes {
            network,
            num_events,
            up_to_block,
        } => {
            let events = subscan::fetch_disputes_events(&network, up_to_block, num_events).await?;
            let initiators = subscan::fetch_dispute_initiators(&network, events).await?;

            std::fs::create_dir_all("out")?;

            let csv_file = format!("out/disputes-{network}-{up_to_block}.csv");
            let mut wrt = csv::Writer::from_path(&csv_file)?;
            for i in initiators.into_iter() {
                wrt.serialize(i)?;
            }
            wrt.flush()?;
            eprintln!("Saved the data to {csv_file}");
        }
    }

    Ok(())
}

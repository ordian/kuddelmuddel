use clap::Parser;
use plotters::prelude::*;

pub mod subscan;

/// cargo run -- --network kusama --para-id 2023 --up-to-block 11324714
#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Name of the network, e.g. "kusama"
    #[clap(long, default_value = "kusama")]
    network: String,

    /// Parachain ID to be processed
    #[clap(long)]
    para_id: u32,

    /// The block number up to which we should
    /// be fetching events, e.g. 11324714
    #[clap(long)]
    up_to_block: u32,

    /// How many events to fetch
    #[clap(long, default_value_t = 500)]
    enough_events: usize,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Event {
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
    let args = Args::parse();
    let network = &args.network;
    let block = args.up_to_block;
    let para_id: u32 = args.para_id;
    let enough_events = args.enough_events;
    let events = subscan::fetch(network, block, para_id, enough_events).await?;

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
        let csv_file = format!("out/{block}-{name}-{para_id}.csv");
        let mut wrt = csv::Writer::from_path(&csv_file)?;
        for p in data.iter().copied() {
            wrt.serialize(p)?;
        }
        wrt.flush()?;
        eprintln!("Saved the data to {csv_file}");

        let out_file_name = format!("out/{block}-{name}-{para_id}.png");
        let root = BitMapBackend::new(&out_file_name, (1024, 768)).into_drawing_area();

        root.fill(&WHITE)?;
        let x_first = data.first().unwrap().block_num;
        let x_last = data.last().unwrap().block_num;
        let y_max = data.iter().max_by_key(|p| p.blocks).unwrap().blocks as f64 + 1.0;
        eprintln!("Plotting {name} for blocks {x_first}..{x_last}");

        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .caption(
                format!("{network}: para_id({para_id}) {name} times"),
                ("sans-serif", 40),
            )
            .set_label_area_size(LabelAreaPosition::Left, 60)
            .set_label_area_size(LabelAreaPosition::Bottom, 40)
            .build_cartesian_2d(x_first..x_last, 0.0..y_max)?;

        chart
            .configure_mesh()
            .disable_x_mesh()
            .disable_y_mesh()
            .x_labels(10)
            .y_labels(5)
            .y_desc("Blocks")
            .draw()?;

        chart
            .draw_series(data.into_iter().map(|p| {
                Circle::new((p.block_num as _, p.blocks as _), 3, RED.mix(0.2).filled())
            }))?;

        root.present().unwrap();
        eprintln!("The {name} plot has been saved to {out_file_name}");
    }

    Ok(())
}

use crate::primitives::{AvailableData, BlockData, ValidationCode, ValidationParams};
use futures::channel::oneshot;
use parity_scale_codec::Encode as _;
use polkadot_node_core_pvf::{Config, Pvf};
use std::path::PathBuf;
use std::time::{Duration, Instant};

// TODO: proper errors
fn other_io_error(s: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, s)
}

pub async fn validate_candidate(
    pvfs_path: PathBuf,
    pov: AvailableData,
    pvf: ValidationCode,
) -> anyhow::Result<()> {
    let program_path = std::env::current_exe()?;
    let (mut validation_host, worker) =
        polkadot_node_core_pvf::start(Config::new(pvfs_path, program_path), Default::default());

    // CURSED
    let _detached_thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(worker);
    });

    let raw_block_data =
        sp_maybe_compressed_blob::decompress(&pov.pov.block_data.0, 20 * 1024 * 1024)?.to_vec();

    println!("PoV size: {}kb", raw_block_data.len() / 1024);
    let block_data = BlockData(raw_block_data);

    let persisted_validation_data = pov.validation_data;

    let params = ValidationParams {
        parent_head: persisted_validation_data.parent_head.clone(),
        block_data,
        relay_parent_number: persisted_validation_data.relay_parent_number,
        relay_parent_storage_root: persisted_validation_data.relay_parent_storage_root,
    };

    let raw_validation_code =
        sp_maybe_compressed_blob::decompress(&pvf.0, 12 * 1024 * 1024)?.to_vec();

    // precheck PVF
    println!("Pvf prechecking...");
    let pvf = Pvf::from_code(raw_validation_code);
    {
        let (tx, rx) = oneshot::channel();

        let now = Instant::now();
        validation_host
            .precheck_pvf(pvf.clone(), tx)
            .await
            .map_err(other_io_error)?;
        rx.await?.map_err(|e| other_io_error(format!("{e:?}")))?;
        let elapsed = now.elapsed().as_millis();

        println!("Pvf preparation took {elapsed}ms");
    }

    println!("Pvf execution...");
    let (tx, rx) = oneshot::channel();
    let now = Instant::now();
    validation_host
        .execute_pvf(
            pvf,
            Duration::from_secs(12),
            params.encode(),
            polkadot_node_core_pvf::Priority::Normal,
            tx,
        )
        .await
        .map_err(other_io_error)?;

    rx.await?.map_err(|e| other_io_error(format!("{e:?}")))?;
    let elapsed = now.elapsed().as_millis();

    println!("Execution took {elapsed}ms");

    Ok(())
}

use crate::primitives::{AvailableData, CandidateReceipt, H256};
use parity_scale_codec::Encode as _;
use std::path::PathBuf;

pub async fn get_or_fetch_candidate(
    path: PathBuf,
    candidate_hash: &H256,
    network: &str,
) -> anyhow::Result<(AvailableData, CandidateReceipt<H256>)> {
    let candidate = format!("{candidate_hash:?}");

    // check cache first
    let receipts_dir = path.as_path().join("receipts");
    let _ = std::fs::create_dir_all(receipts_dir.as_path());
    let pov_cache = path.as_path().join(&candidate);
    let receipt_cache = receipts_dir.as_path().join(&candidate);

    if receipt_cache.as_path().exists() {
        let pov_bytes = std::fs::read(pov_cache)?;
        let receipt_bytes = std::fs::read(receipt_cache)?;

        let pov = parity_scale_codec::decode_from_bytes(pov_bytes.into())?;
        let receipt: CandidateReceipt<H256> =
            parity_scale_codec::decode_from_bytes(receipt_bytes.into())?;

        println!(
            "Using cached PoV for {candidate}, para_id={}",
            receipt.descriptor.para_id.0
        );

        return Ok((pov, receipt));
    }

    // fetch available data and receipt from povs.today
    let candidate = format!("{candidate_hash:?}");
    let prefix = &candidate[2..4];
    let pov_url = format!("https://pov.data.paritytech.io/{network}/{prefix}/{candidate}");
    let receipt_url =
        format!("https://pov.data.paritytech.io/{network}/{prefix}/receipts/{candidate}");
    let client = reqwest::Client::new();

    let pov_req = client.get(&pov_url).send().await?;
    let pov_bytes = pov_req.bytes().await?;

    let receipt_req = client.get(&receipt_url).send().await?;
    let receipt_bytes = receipt_req.bytes().await?;

    let pov: AvailableData = parity_scale_codec::decode_from_bytes(pov_bytes)?;
    let receipt: CandidateReceipt<H256> = parity_scale_codec::decode_from_bytes(receipt_bytes)?;

    // store them in the cache
    println!(
        "Successfully fetched PoV for {candidate}, para_id={}",
        receipt.descriptor.para_id.0
    );

    std::fs::write(pov_cache, pov.encode())?;
    std::fs::write(receipt_cache, receipt.encode())?;

    Ok((pov, receipt))
}

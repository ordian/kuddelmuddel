use std::collections::{btree_map::Entry, BTreeMap};
use std::path::Path;

use crate::primitives::{SessionIndex, ValidationCode, ValidationCodeHash};
use parity_scale_codec::Encode as _;
use subxt::{
    utils::H256, utils::AccountId32, OnlineClient, PolkadotConfig,
};

#[subxt::subxt(runtime_metadata_path = "assets/kusama_metadata.scale")]
pub mod polkadot {}

pub async fn historical_account_keys(
    rpc_url: String,
    input: impl IntoIterator<Item = (SessionIndex, H256)>,
) -> anyhow::Result<BTreeMap<SessionIndex, Vec<AccountId32>>> {
    let api = OnlineClient::<PolkadotConfig>::from_url(rpc_url)
        .await?;

    let mut map: BTreeMap<SessionIndex, Vec<AccountId32>> = BTreeMap::new();

    for (session, block_hash) in input.into_iter() {
        if let Entry::Vacant(e) = map.entry(session) {
            let storage_query = polkadot::storage().para_session_info()
                .account_keys(&session);
            // TODO: handle errors here
            let keys = api
                .storage()
                .at(block_hash)
                .fetch(&storage_query)
                .await?;
            // TODO: handle None
            if let Some(keys) = keys {
                e.insert(keys);
            }
        }
    }

    Ok(map)
}

pub async fn validation_code_by_hash(
    pvfs_path: &Path,
    rpc_url: String,
    code_hash: ValidationCodeHash,
    relay_parent: H256,
) -> anyhow::Result<ValidationCode> {
    let validation_code_hash = format!("{:?}", code_hash.0);
    let file = pvfs_path.join(&validation_code_hash);
    if file.exists() {
        let bytes = std::fs::read(file)?;
        let pvf = parity_scale_codec::decode_from_bytes(bytes.into())?;

        println!("Using cached Pvf {validation_code_hash}");
        return Ok(pvf);
    }

    println!("Fetching Pvf {validation_code_hash}");

    let api = OnlineClient::<PolkadotConfig>::from_url(rpc_url)
        .await?;

    let storage_query = polkadot::storage().paras()
        .code_by_hash(&code_hash);

    let code = api
        .storage()
        .at(relay_parent)
        .fetch(&storage_query)
        .await?;

    // cache the Pvf
    let code = code.expect("relay_parent and code_hash are valid; qed");
    std::fs::write(file, code.encode())?;

    Ok(code)
}

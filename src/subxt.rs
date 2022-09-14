use std::collections::{btree_map::Entry, BTreeMap};
use std::path::Path;

use crate::primitives::{SessionIndex, ValidationCode, ValidationCodeHash};
use parity_scale_codec::Encode as _;
use subxt::{
    sp_core::H256, sp_runtime::AccountId32, ClientBuilder, DefaultConfig, PolkadotExtrinsicParams,
};

#[subxt::subxt(runtime_metadata_path = "assets/kusama_metadata.scale")]
pub mod polkadot {}

pub async fn historical_account_keys(
    rpc_url: String,
    input: impl IntoIterator<Item = (SessionIndex, H256)>,
) -> anyhow::Result<BTreeMap<SessionIndex, Vec<AccountId32>>> {
    let api = ClientBuilder::new()
        .set_url(rpc_url)
        .build()
        .await?
        .to_runtime_api::<
            polkadot::RuntimeApi<
                DefaultConfig,
                PolkadotExtrinsicParams<DefaultConfig>,
            >
        >();

    let mut map: BTreeMap<SessionIndex, Vec<AccountId32>> = BTreeMap::new();

    for (session, block_hash) in input.into_iter() {
        if let Entry::Vacant(e) = map.entry(session) {
            // TODO: handle errors here
            let keys = api
                .storage()
                .para_session_info()
                .account_keys(&session, Some(block_hash))
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

    let api = ClientBuilder::new()
        .set_url(rpc_url)
        .build()
        .await?
        .to_runtime_api::<
            polkadot::RuntimeApi<
                DefaultConfig,
                PolkadotExtrinsicParams<DefaultConfig>,
            >
        >();

    let code = api
        .storage()
        .paras()
        .code_by_hash(&code_hash, Some(relay_parent))
        .await?;

    // cache the Pvf
    let code = code.expect("relay_parent and code_hash are valid; qed");
    std::fs::write(file, code.encode())?;

    Ok(code)
}

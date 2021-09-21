// Copyright 2021 UINB Technologies Pte. Ltd.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;
use crate::{assets::Balance, core::*, event::*, matcher::*, orderbook::AskOrBid, output::Output};
use sha2::{Digest, Sha256};
use std::sync::mpsc::Sender;

const ACCOUNT_KEY: u8 = 0x00;
const ORDERBOOK_KEY: u8 = 0x01;

pub struct Prover(Sender<Proof>);

impl Prover {
    pub fn new(tx: Sender<Proof>) -> Self {
        Self(tx)
    }

    pub fn prove_trade_cmd(
        &self,
        data: &mut Data,
        nonce: u32,
        signature: Vec<u8>,
        encoded_cmd: FusoCommand,
        ask_size_before: Amount,
        bid_size_before: Amount,
        taker_base_before: &Balance,
        taker_quote_before: &Balance,
        outputs: &[Output],
    ) {
        let mut leaves = vec![];
        let taker = outputs.last().unwrap();
        let symbol = taker.symbol.clone();
        let event_id = taker.event_id;
        let user_id = taker.user_id;
        let orderbook = data.orderbooks.get(&symbol).unwrap();
        let size = orderbook.size();
        log::debug!(
            "generating merkle leaf of {:?}: orderbook = ({:?}, {:?}) -> ({:?}, {:?})",
            taker.event_id,
            ask_size_before,
            bid_size_before,
            size.0,
            size.1,
        );
        let (old_ask_size, old_bid_size, new_ask_size, new_bid_size) = (
            ask_size_before.to_amount(),
            bid_size_before.to_amount(),
            size.0.to_amount(),
            size.1.to_amount(),
        );
        leaves.push(new_orderbook_merkle_leaf(
            symbol,
            old_ask_size,
            old_bid_size,
            new_ask_size,
            new_bid_size,
        ));
        outputs
            .iter()
            .take_while(|o| o.role == Role::Maker)
            .for_each(|ref r| {
                let (ba, bf, qa, qf) = match r.ask_or_bid {
                    // -base_frozen, +quote_available
                    // base_frozen0 + r.base_delta = base_frozen
                    // quote_available0 + r.quote_delta + r.quote_charge = quote_available
                    AskOrBid::Ask => (
                        r.base_available,
                        r.base_frozen + r.base_delta.abs(),
                        r.quote_available - r.quote_delta.abs() + r.quote_charge.abs(),
                        r.quote_frozen,
                    ),
                    // +base_available, -quote_frozen
                    // quote_frozen0 + r.quote_delta = quote_frozen
                    // base_available0 + r.base_delta + r.base_charge = base_available
                    AskOrBid::Bid => (
                        r.base_available - r.base_delta.abs() + r.base_charge.abs(),
                        r.base_frozen,
                        r.quote_available,
                        r.quote_frozen + r.quote_delta.abs(),
                    ),
                };
                let (new_ba, new_bf, old_ba, old_bf) = (
                    r.base_available.to_amount(),
                    r.base_frozen.to_amount(),
                    ba.to_amount(),
                    bf.to_amount(),
                );
                leaves.push(new_account_merkle_leaf(
                    &r.user_id, symbol.0, old_ba, old_bf, new_ba, new_bf,
                ));
                let (new_qa, new_qf, old_qa, old_qf) = (
                    r.quote_available.to_amount(),
                    r.quote_frozen.to_amount(),
                    qa.to_amount(),
                    qf.to_amount(),
                );
                leaves.push(new_account_merkle_leaf(
                    &r.user_id, symbol.1, old_qa, old_qf, new_qa, new_qf,
                ));
            });
        let (new_taker_ba, new_taker_bf, old_taker_ba, old_taker_bf) = (
            taker.base_available.to_amount(),
            taker.base_frozen.to_amount(),
            taker_base_before.available.to_amount(),
            taker_base_before.frozen.to_amount(),
        );
        log::debug!(
            "generating merkle leaf of {:?}: taker base = [{:?}({:?}), {:?}({:?})] -> [{:?}({:?}), {:?}({:?})]",
            taker.event_id,
            old_taker_ba,
            taker_base_before.available,
            old_taker_bf,
            taker_base_before.frozen,
            new_taker_ba,
            taker.base_available,
            new_taker_bf,
            taker.base_frozen,
        );
        leaves.push(new_account_merkle_leaf(
            &user_id,
            symbol.0,
            old_taker_ba,
            old_taker_bf,
            new_taker_ba,
            new_taker_bf,
        ));
        let (new_taker_qa, new_taker_qf, old_taker_qa, old_taker_qf) = (
            taker.quote_available.to_amount(),
            taker.quote_frozen.to_amount(),
            taker_quote_before.available.to_amount(),
            taker_quote_before.frozen.to_amount(),
        );
        log::debug!(
            "generating merkle leaf of {:?}: taker quote = [{:?}({:?}), {:?}({:?})] -> [{:?}({:?}), {:?}({:?})]",
            taker.event_id,
            old_taker_qa,
            taker_quote_before.available,
            old_taker_qf,
            taker_quote_before.frozen,
            new_taker_qa,
            taker.quote_available,
            new_taker_qf,
            taker.quote_frozen,
        );
        leaves.push(new_account_merkle_leaf(
            &user_id,
            symbol.1,
            old_taker_qa,
            old_taker_qf,
            new_taker_qa,
            new_taker_qf,
        ));
        let (pr0, pr1) = gen_proofs(&mut data.merkle_tree, &leaves);
        self.0
            .send(Proof {
                event_id: event_id,
                user_id: user_id,
                nonce: nonce,
                signature: signature,
                cmd: encoded_cmd,
                leaves: leaves,
                proof_of_exists: pr0,
                proof_of_cmd: pr1,
                // TODO redundant clone because &H256 doesn't implement Into<[u8; 32]>
                root: data.merkle_tree.root().clone().into(),
            })
            .unwrap();
    }

    pub fn prove_assets_cmd(
        &self,
        merkle_tree: &mut GlobalStates,
        event_id: u64,
        cmd: AssetsCmd,
        account_before: &Balance,
        account_after: &Balance,
    ) {
        let (new_available, new_frozen, old_available, old_frozen) = (
            account_after.available.to_amount(),
            account_after.frozen.to_amount(),
            account_before.available.to_amount(),
            account_before.frozen.to_amount(),
        );
        let leaves = vec![new_account_merkle_leaf(
            &cmd.user_id,
            cmd.currency,
            old_available,
            old_frozen,
            new_available,
            new_frozen,
        )];
        let (pr0, pr1) = gen_proofs(merkle_tree, &leaves);
        self.0
            .send(Proof {
                event_id: event_id,
                user_id: cmd.user_id,
                nonce: cmd.nonce_or_block_number,
                signature: cmd.signature_or_hash.clone(),
                cmd: cmd.into(),
                leaves: leaves,
                proof_of_exists: pr0,
                proof_of_cmd: pr1,
                root: merkle_tree.root().clone().into(),
            })
            .unwrap();
    }
}

fn gen_proofs(merkle_tree: &mut GlobalStates, leaves: &Vec<MerkleLeaf>) -> (Vec<u8>, Vec<u8>) {
    let keys = leaves
        .iter()
        .map(|leaf| Sha256::digest(&leaf.key).into())
        .collect::<Vec<_>>();
    let poe = merkle_tree.merkle_proof(keys.clone()).unwrap();
    let pr0 = poe
        .compile(
            leaves
                .iter()
                .map(|leaf| (Sha256::digest(&leaf.key).into(), leaf.old_v.into()))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    leaves.iter().for_each(|leaf| {
        merkle_tree
            .update(Sha256::digest(&leaf.key).into(), leaf.new_v.into())
            .unwrap();
    });
    let poc = merkle_tree.merkle_proof(keys.clone()).unwrap();
    let pr1 = poc
        .compile(
            leaves
                .iter()
                .map(|leaf| (Sha256::digest(&leaf.key).into(), leaf.new_v.into()))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    (pr0.into(), pr1.into())
}

fn new_account_merkle_leaf(
    user_id: &UserId,
    currency: Currency,
    old_available: u128,
    old_frozen: u128,
    new_available: u128,
    new_frozen: u128,
) -> MerkleLeaf {
    let mut key = vec![ACCOUNT_KEY; 37];
    key[1..33].copy_from_slice(<B256 as AsRef<[u8]>>::as_ref(user_id));
    key[33..].copy_from_slice(&currency.to_le_bytes()[..]);
    MerkleLeaf {
        key: key,
        old_v: u128le_to_h256(old_available, old_frozen),
        new_v: u128le_to_h256(new_available, new_frozen),
    }
}

fn new_orderbook_merkle_leaf(
    symbol: Symbol,
    old_ask_size: u128,
    old_bid_size: u128,
    new_ask_size: u128,
    new_bid_size: u128,
) -> MerkleLeaf {
    let mut key = vec![ORDERBOOK_KEY; 9];
    key[1..5].copy_from_slice(&symbol.0.to_le_bytes()[..]);
    key[5..].copy_from_slice(&symbol.1.to_le_bytes()[..]);
    MerkleLeaf {
        key: key,
        old_v: u128le_to_h256(old_ask_size, old_bid_size),
        new_v: u128le_to_h256(new_ask_size, new_bid_size),
    }
}

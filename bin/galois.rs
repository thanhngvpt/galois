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

use galois::{config, event, output, sequence, server, snapshot, merkle_tree};

use std::sync::{atomic, mpsc, Arc};

use lru::LruCache;
use std::time::Duration;
use std::thread;
use output::AccountKey;
use sparse_merkle_tree::{H256, SparseMerkleTree};
use sparse_merkle_tree::blake2b::Blake2bHasher;
use sparse_merkle_tree::traits::Value;
use sparse_merkle_tree::default_store::DefaultStore;

fn main() {
    print_banner();
    lazy_static::initialize(&config::C);
    lazy_static::initialize(&config::ENABLE_START_FROM_GENESIS);
    let (id, coredump) = snapshot::load().unwrap();
    let (output_tx, output_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    let ready = Arc::new(atomic::AtomicBool::new(false));
    let mut account_id_cache: LruCache<AccountKey, String> = LruCache::new(10000000);
    output::init(output_tx.clone(), output_rx, account_id_cache);
    event::init(event_rx, output_tx, coredump);
    sequence::init(event_tx.clone(), id, ready.clone());
    server::init(event_tx, ready);
    let mut tree: SparseMerkleTree<Blake2bHasher, Value, DefaultStore<Value>> = sparse_merkle_tree::SparseMerkleTree::default();

    loop {
        thread::sleep(Duration::from_millis(100));
    }
}

fn print_banner() {
    const BANNER: &str = r#"

                 **       **
   *******     ******     **               **
  ***               **    **     *****     **    ******
 **              *****    **   ***   ***        **    *
 **            *******    **   **     **   **   **
 **    *****  **    **    **   *       *   **    **
  **     ***  **    **    **   **     **   **     ****
   *********   **  ****   **    *******    **        **
      *    *    ****  *   **      ***      **    ** ***
                                                  ****
"#;
    println!("{}", BANNER);
}

// Copyright 2019. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

//! Write 20 GB of data to a MMR by writing 20,000 1MB data blocks; rewinding to 19,000, and then writing the last
//! 1,000 blocks again. Follow this up by reading a few random blocks from the merkle tree.
//! Run this example with `cargo run --example stress_test`

use blake2::Blake2b;
use digest::Digest;
use log::*;
use merklemountainrange::{self, mmr::MerkleMountainRange};
use rand::{self, Rng};
use serde::{Deserialize, Serialize};
use std::{fs, time::Instant, vec::Vec};
use tari_infra_derive::Hashable;
use tari_storage::lmdb::LMDBBuilder;
use tari_utilities::{hex::Hex, ExtendBytes, Hashable};

const SIZE: usize = 131_072;
const MMR_SIZE: u64 = 2_000;
const PATH: &str = "./examples/stress_test";

#[derive(Debug, Deserialize, Serialize, Hashable)]
#[digest = "Blake2b"]
pub struct BigBlock {
    value: u64,
    #[Hashable(Ignore)]
    blob: Vec<u64>,
}

impl BigBlock {
    pub fn new(value: u64) -> BigBlock {
        let blob = vec![value; SIZE];
        BigBlock { value, blob }
    }
}

pub struct Timer {
    start: Instant,
    count: u32,
    log_every: u32,
    pub mmr: MerkleMountainRange<BigBlock, Blake2b>,
}

impl Timer {
    fn new(mmr: MerkleMountainRange<BigBlock, Blake2b>, log_every: u32) -> Timer {
        Timer {
            start: Instant::now(),
            count: 0,
            log_every,
            mmr,
        }
    }

    fn inc(&mut self) {
        self.count += 1;
        if self.count % self.log_every == 0 {
            self.log();
        }
    }

    fn reset(&mut self) {
        self.start = Instant::now();
        self.count = 0;
    }

    fn log(&self) {
        let time = self.start.elapsed();
        let ave_time = time / self.count;
        info!(
            "{} operations.. {:5}.{} sec. Average time per op: {:6} µs",
            self.count,
            time.as_secs(),
            time.subsec_millis(),
            ave_time.as_micros()
        );
        let root = self.mmr.get_merkle_root().to_hex();
        let size = self.mmr.len();
        info!("MMR size: {}, Merkle Root: {}", size, root);
    }
}

fn main() {
    let _ = simple_logger::init();
    info!("Setting up test...");
    let mut rng = rand::rngs::OsRng::new().unwrap();
    let mmr: MerkleMountainRange<BigBlock, Blake2b> = MerkleMountainRange::new();
    let mut timer = Timer::new(mmr, 100);
    timer.mmr.init_persistance_store(&"mmr".to_string(), 1500);

    let _ = fs::remove_dir_all(PATH);
    // create storage
    fs::create_dir(PATH).unwrap();
    let builder = LMDBBuilder::new();
    let mut store = builder
        .set_mapsize(20 * 1024)
        .set_path(PATH)
        .add_database(&"mmr_mmr_checkpoints".to_string())
        .add_database(&"mmr_mmr_objects".to_string())
        .add_database(&"mmr_init".to_string())
        .build()
        .unwrap();

    info!("Inserting {} objects into MMR...", MMR_SIZE);
    for i in 0u64..MMR_SIZE {
        let block = BigBlock::new(i);
        assert!(timer.mmr.push(block).is_ok());
        timer.mmr.checkpoint().unwrap();
        timer.mmr.apply_state(&mut store).unwrap();
        timer.inc();
    }
    timer.reset();
    info!("Checking consistency of 1000 random blocks..");
    for _ in 0..1000 {
        let i: u64 = rng.gen_range(0, MMR_SIZE);
        let obj = timer.mmr.get_object_by_object_index(i as usize).unwrap();
        assert_eq!(obj.value, i);
        assert!(obj.blob.iter().all(|v| *v == i));
        timer.inc();
    }

    timer.reset();
    info!("Deleting {} blocks...", MMR_SIZE / 10);
    for i in 0..MMR_SIZE / 10 {
        let index = i * 10;
        let hash = Blake2b::digest(&index.to_le_bytes());
        timer.mmr.prune_object_hash(hash.as_slice()).unwrap();
        timer.inc();
    }

    timer.reset();
    info!("Rewinding 1000 blocks..");

    timer.mmr.rewind(&mut store, 1000).unwrap();
    timer.log();

    timer.reset();
    info!("Replaying 1000 blocks..");
    for i in 0u64..1000 {
        let block = BigBlock::new(i);
        assert!(timer.mmr.push(block).is_ok());
        timer.mmr.checkpoint().unwrap();
        timer.mmr.apply_state(&mut store).unwrap();
        timer.inc();
    }

    timer.log();

    let mmr2: MerkleMountainRange<BigBlock, Blake2b> = MerkleMountainRange::new();
    timer.mmr.init_persistance_store(&"mmr".to_string(), 1500);
    let mut timer = Timer::new(mmr2, 1);
    info!("Reloading MMR from store...");
    assert!(timer.mmr.load_from_store(&mut store).is_ok());
    timer.inc();
    info!("MMR stress test complete.")
}

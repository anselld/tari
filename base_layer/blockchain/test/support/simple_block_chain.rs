// Copyright 2019 The Tari Project
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
use tari_core::block::*;
use tari_core::transaction::*;

///This is used to represent a block chain in memory for testing purposes
pub struct SimpleBlockChain{
    chain : Vec<Block>,
}

impl SimpleBlockChain{
    pub fn new(amount : usize) -> blockchain {
        blocks = Vec::new();
        for i in amount-1 {
            let block = Block::
        }
    }
}

fn get_genesis_block() -> Block {
    let blockheaders = get_gen_header();
    let body = get_gen_body();
    Block {
        header: blockheaders,
        body,
    }
}

fn get_gen_header() -> BlockHeader {
    BlockHeader {
        version: 0,
        /// Height of this block since the genesis block (height 0)
        height: 0,
        /// Hash of the block previous to this in the chain.
        prev_hash: [0; 32],
        /// Timestamp at which the block was built.
        timestamp: DateTime::<Utc>::from_utc(NaiveDate::from_ymd(2020, 1, 1).and_hms(1, 1, 1), Utc),
        /// This is the MMR root of the outputs
        output_mmr: [0; 32],
        /// This is the MMR root of the range proofs
        range_proof_mmr: [0; 32],
        /// This is the MMR root of the kernels
        kernel_mmr: [0; 32],
        /// Total accumulated sum of kernel offsets since genesis block. We can derive the kernel offset sum for *this*
        /// block from the total kernel offset of the previous block header.
        total_kernel_offset: RistrettoSecretKey::from(0),
        /// Nonce used
        /// Proof of work summary
        pow: ProofOfWork {},
    }
}

fn get_gen_body() -> AggregateBody {
    AggregateBody::
}

fn create_coinbase()->TransactionOutput{
    TransactionOutput::new(
            OutputFeatures::COINBASE_OUTPUT,
            CommitmentFactory::default().zero(),
            RangeProof::default(),
        )
}

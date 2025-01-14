//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use log::*;

use tari_comms::connection::{zmq::ZmqEndpoint, Connection, Direction, ZmqContext};

use std::{
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};
use tari_comms::message::FrameSet;

const LOG_TARGET: &str = "comms::test_support::connection_message_counter";

/// Set the allocated stack size for each ConnectionMessageCounter thread
const THREAD_STACK_SIZE: usize = 64 * 1024; // 64kb

pub struct ConnectionMessageCounter<'c> {
    counter: Arc<RwLock<u32>>,
    context: &'c ZmqContext,
    response: Option<FrameSet>,
}

impl<'c> ConnectionMessageCounter<'c> {
    pub fn new(context: &'c ZmqContext) -> Self {
        Self {
            counter: Arc::new(RwLock::new(0)),
            context,
            response: None,
        }
    }

    pub fn set_response(&mut self, response: FrameSet) -> &mut Self {
        self.response = Some(response);
        self
    }

    pub fn count(&self) -> u32 {
        let counter_lock = acquire_read_lock!(self.counter);
        *counter_lock
    }

    pub fn assert_count(&self, count: u32, num_polls: usize) -> () {
        for _i in 0..num_polls {
            thread::sleep(Duration::from_millis(100));
            let curr_count = self.count();
            if curr_count == count {
                return;
            }
            if curr_count > count {
                panic!(
                    "Message count exceeded the expected count. Expected={} Actual={}",
                    count, curr_count
                );
            }
        }
        panic!(
            "Message count did not reach {} within {}ms. Count={}",
            count,
            num_polls * 100,
            self.count()
        );
    }

    pub fn start<A: ZmqEndpoint + Send + Sync + Clone + 'static>(&self, address: A) {
        let counter = self.counter.clone();
        let context = self.context.clone();
        let address = address.clone();
        let response = self.response.clone();
        thread::Builder::new()
            .name("connection-message-counter-thread".to_string())
            .stack_size(THREAD_STACK_SIZE)
            .spawn(move || {
                let connection = Connection::new(&context, Direction::Inbound)
                    .set_name("Message Counter")
                    .establish(&address)
                    .unwrap();

                loop {
                    match connection.receive(1000) {
                        Ok(frames) => {
                            let mut counter_lock = acquire_write_lock!(counter);
                            *counter_lock += 1;
                            debug!(target: LOG_TARGET, "Added to message count (count={})", *counter_lock);
                            if let Some(ref r) = response {
                                let mut payload = vec![frames[0].clone()];
                                payload.extend(r.clone());
                                connection.send(payload).unwrap();
                            }
                        },
                        _ => {
                            debug!(target: LOG_TARGET, "Nothing received for 1 second...");
                        },
                    }
                }
            })
            .unwrap();
    }
}
